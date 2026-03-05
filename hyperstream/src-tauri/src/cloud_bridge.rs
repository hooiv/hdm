use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::io::AsyncReadExt;
use crate::settings::Settings;

pub struct CloudBridge;

impl CloudBridge {
    pub async fn upload_file(
        settings: &Settings,
        file_path: &str,
        target_key: &str
    ) -> Result<String, String> {
        if !settings.cloud_enabled {
            return Err("Cloud upload is disabled in settings".to_string());
        }

        let endpoint = settings.cloud_endpoint.as_deref().ok_or("Missing Cloud Endpoint")?;
        let bucket_name = settings.cloud_bucket.as_deref().ok_or("Missing Cloud Bucket")?;
        let access_key = settings.cloud_access_key.as_deref().ok_or("Missing Access Key")?;
        let secret_key = settings.cloud_secret_key.as_deref().ok_or("Missing Secret Key")?;
        let region_str = settings.cloud_region.as_deref().unwrap_or("us-east-1");

        // Create Credentials
        let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|e| e.to_string())?;

        // Parse Region
        let region = if endpoint.contains("amazonaws.com") {
            // standard AWS
            match region_str.parse() {
                Ok(r) => r,
                Err(_) => Region::Custom { region: region_str.to_string(), endpoint: endpoint.to_string() }
            }
        } else {
            // Custom (MinIO, Wasabi, etc)
            Region::Custom {
                region: region_str.to_string(),
                endpoint: endpoint.to_string(),
            }
        };

        // Create Bucket
        let bucket = Bucket::new(bucket_name, region, credentials)
             .map_err(|e| e.to_string())?;

        // Path validation: ensure file is within the download directory
        let download_dir = dunce::canonicalize(&settings.download_dir)
            .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
        let canon = dunce::canonicalize(file_path)
            .map_err(|e| format!("Cannot resolve file path: {}", e))?;
        if !canon.starts_with(&download_dir) {
            return Err("File must be within the download directory".to_string());
        }

        // Read file using the canonicalized path (prevents TOCTOU symlink attacks)
        let mut file = tokio::fs::File::open(&canon).await.map_err(|e| e.to_string())?;

        // Read file into memory for S3 upload (with OOM protection).
        // TODO: Switch to streaming upload via put_object_stream for large files.
        let metadata = tokio::fs::metadata(&canon).await.map_err(|e| e.to_string())?;
        const MAX_UPLOAD_SIZE: u64 = 512 * 1024 * 1024; // 512 MB
        if metadata.len() > MAX_UPLOAD_SIZE {
            return Err(format!(
                "File too large for upload ({:.1} MB). Maximum is {} MB. Use rclone for larger files.",
                metadata.len() as f64 / (1024.0 * 1024.0),
                MAX_UPLOAD_SIZE / (1024 * 1024)
            ));
        }
        
        let mut buffer = Vec::new();
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
