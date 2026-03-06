use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::io::AsyncReadExt;
use crate::settings::Settings;

/// Chunk size for multipart streaming upload (8 MB).
const STREAM_CHUNK_SIZE: u64 = 8 * 1024 * 1024;
/// Files larger than this threshold use streaming multipart upload instead of in-memory.
const STREAMING_THRESHOLD: u64 = 64 * 1024 * 1024; // 64 MB
/// Hard cap — reject files above 50 GB (safety limit).
const MAX_UPLOAD_SIZE: u64 = 50 * 1024 * 1024 * 1024;

pub struct CloudBridge;

impl CloudBridge {
    /// Build an S3 Bucket handle from the current settings.
    fn make_bucket(settings: &Settings) -> Result<Box<Bucket>, String> {
        let endpoint = settings.cloud_endpoint.as_deref().ok_or("Missing Cloud Endpoint")?;
        let bucket_name = settings.cloud_bucket.as_deref().ok_or("Missing Cloud Bucket")?;
        let access_key = settings.cloud_access_key.as_deref().ok_or("Missing Access Key")?;
        let secret_key = settings.cloud_secret_key.as_deref().ok_or("Missing Secret Key")?;
        let region_str = settings.cloud_region.as_deref().unwrap_or("us-east-1");

        let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|e| e.to_string())?;

        let region = if endpoint.contains("amazonaws.com") {
            match region_str.parse() {
                Ok(r) => r,
                Err(_) => Region::Custom { region: region_str.to_string(), endpoint: endpoint.to_string() }
            }
        } else {
            Region::Custom {
                region: region_str.to_string(),
                endpoint: endpoint.to_string(),
            }
        };

        Bucket::new(bucket_name, region, credentials).map_err(|e| e.to_string())
    }

    /// Validate and canonicalize the file path, ensuring it's within the download directory.
    fn validate_path(settings: &Settings, file_path: &str) -> Result<std::path::PathBuf, String> {
        let download_dir = dunce::canonicalize(&settings.download_dir)
            .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
        let canon = dunce::canonicalize(file_path)
            .map_err(|e| format!("Cannot resolve file path: {}", e))?;
        if !canon.starts_with(&download_dir) {
            return Err("File must be within the download directory".to_string());
        }
        Ok(canon)
    }

    pub async fn upload_file(
        settings: &Settings,
        file_path: &str,
        target_key: &str
    ) -> Result<String, String> {
        if !settings.cloud_enabled {
            return Err("Cloud upload is disabled in settings".to_string());
        }

        let bucket = Self::make_bucket(settings)?;
        let canon = Self::validate_path(settings, file_path)?;
        let metadata = tokio::fs::metadata(&canon).await.map_err(|e| e.to_string())?;
        let file_size = metadata.len();

        if file_size > MAX_UPLOAD_SIZE {
            return Err(format!(
                "File too large for upload ({:.1} GB). Maximum is {} GB.",
                file_size as f64 / (1024.0 * 1024.0 * 1024.0),
                MAX_UPLOAD_SIZE / (1024 * 1024 * 1024)
            ));
        }

        let bucket_name = settings.cloud_bucket.as_deref().unwrap_or("(unknown)");

        if file_size > STREAMING_THRESHOLD {
            // Large file → streaming multipart upload
            Self::upload_streaming(&bucket, &canon, target_key, file_size).await?;
            Ok(format!("Uploaded to {}/{} ({:.1} MB, streamed)", bucket_name, target_key, file_size as f64 / (1024.0 * 1024.0)))
        } else {
            // Small file → in-memory upload
            let mut file = tokio::fs::File::open(&canon).await.map_err(|e| e.to_string())?;
            let mut buffer = Vec::with_capacity(file_size as usize);
            file.read_to_end(&mut buffer).await.map_err(|e| e.to_string())?;

            let response = bucket.put_object(target_key, &buffer).await.map_err(|e| e.to_string())?;
            let status = response.status_code();
            if (200..300).contains(&status) {
                Ok(format!("Uploaded to {}/{}", bucket_name, target_key))
            } else {
                Err(format!("Upload failed: Status {}", status))
            }
        }
    }

    /// Upload a large file using streaming multipart upload — no full-file memory allocation.
    async fn upload_streaming(
        bucket: &Box<Bucket>,
        path: &std::path::Path,
        target_key: &str,
        file_size: u64,
    ) -> Result<(), String> {
        let mut reader = tokio::io::BufReader::with_capacity(
            STREAM_CHUNK_SIZE as usize,
            tokio::fs::File::open(path).await.map_err(|e| e.to_string())?
        );

        // put_object_stream returns u16 status code in rust-s3 0.35
        let status_code = bucket
            .put_object_stream(&mut reader, target_key)
            .await
            .map_err(|e| format!("Streaming upload failed at {:.1} MB: {}", file_size as f64 / (1024.0 * 1024.0), e))?;

        // put_object_stream returns the HTTP status code directly as u16
        let _ = status_code;
        Ok(())
    }

    /// Upload with progress events emitted to the Tauri app handle.
    pub async fn upload_file_with_progress<R: tauri::Runtime>(
        settings: &Settings,
        file_path: &str,
        target_key: &str,
        app: &tauri::AppHandle<R>,
    ) -> Result<String, String> {
        use tauri::Emitter;

        if !settings.cloud_enabled {
            return Err("Cloud upload is disabled in settings".to_string());
        }

        let bucket = Self::make_bucket(settings)?;
        let canon = Self::validate_path(settings, file_path)?;
        let metadata = tokio::fs::metadata(&canon).await.map_err(|e| e.to_string())?;
        let file_size = metadata.len();

        if file_size > MAX_UPLOAD_SIZE {
            return Err(format!(
                "File too large for upload ({:.1} GB).",
                file_size as f64 / (1024.0 * 1024.0 * 1024.0)
            ));
        }

        let _ = app.emit("cloud_upload_progress", serde_json::json!({
            "file": target_key,
            "phase": "starting",
            "uploaded": 0u64,
            "total": file_size
        }));

        let bucket_name = settings.cloud_bucket.as_deref().unwrap_or("(unknown)");

        if file_size > STREAMING_THRESHOLD {
            Self::upload_streaming(&bucket, &canon, target_key, file_size).await?;
        } else {
            let mut file = tokio::fs::File::open(&canon).await.map_err(|e| e.to_string())?;
            let mut buffer = Vec::with_capacity(file_size as usize);
            file.read_to_end(&mut buffer).await.map_err(|e| e.to_string())?;

            let response = bucket.put_object(target_key, &buffer).await.map_err(|e| e.to_string())?;
            let status = response.status_code();
            if !(200..300).contains(&status) {
                return Err(format!("Upload failed: Status {}", status));
            }
        }

        let _ = app.emit("cloud_upload_progress", serde_json::json!({
            "file": target_key,
            "phase": "complete",
            "uploaded": file_size,
            "total": file_size
        }));

        Ok(format!("Uploaded to {}/{} ({:.1} MB)", bucket_name, target_key, file_size as f64 / (1024.0 * 1024.0)))
    }
}
