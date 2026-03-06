use std::path::Path;
use std::sync::{Arc, Mutex};
use rquest::Client;
use crate::downloader::manager::DownloadManager;
use crate::downloader::disk;
use crate::persistence::SavedDownload;

/// Result of probing a URL: file size, etag, md5, and whether the server supports Range requests.
pub struct ProbeResult {
    pub total_size: u64,
    pub etag: Option<String>,
    pub md5: Option<String>,
    /// True when the server demonstrably supports byte-range requests (Accept-Ranges + 206 response).
    /// When false, the download engine MUST use a single segment to avoid duplicate full-file fetches.
    pub supports_range: bool,
}

/// Strategies to determine file size (HEAD, Range 0-1, etc.)
/// Also verifies range support so the engine can fall back to single-segment when needed.
pub async fn determine_total_size(client: &Client, url: &str) -> Result<ProbeResult, String> {
    // 1. Try HEAD request
    let head_resp = client.head(url).send().await.map_err(|e| e.to_string())?;
    
    let etag = head_resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let md5 = head_resp.headers().get("content-md5").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    
    // Check Accept-Ranges from HEAD
    let accept_ranges = head_resp.headers()
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");
    let head_claims_range = accept_ranges.eq_ignore_ascii_case("bytes");
    
    let head_size = head_resp.content_length()
        .filter(|&len| len > 0)
        .or_else(|| {
            head_resp.headers().get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&len| len > 0)
        });

    // 2. Verify with Range 0-1 request (also discovers size if HEAD didn't provide it)
    let range_resp = client.get(url).header("Range", "bytes=0-1").send().await.map_err(|e| e.to_string())?;
    let range_status = range_resp.status().as_u16();
    
    let range_etag = range_resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let range_md5 = range_resp.headers().get("content-md5").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    
    // Server returned 206 Partial Content → definitely supports range
    let supports_range = if range_status == 206 {
        true
    } else if head_claims_range {
        // HEAD said bytes but didn't return 206 — trust HEAD cautiously
        println!("[probe] Server advertises Accept-Ranges: bytes but returned {} for range probe", range_status);
        true
    } else {
        println!("[probe] Server does not support Range requests (HEAD: Accept-Ranges={}, probe status: {})", accept_ranges, range_status);
        false
    };
    
    // Try to extract size from Content-Range header (e.g., "bytes 0-1/12345")
    let range_size = range_resp.headers().get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.find('/'))
        .and_then(|slash_pos| {
            range_resp.headers().get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s[slash_pos + 1..].trim().parse::<u64>().ok())
        });
    
    // Prefer HEAD size, fall back to Content-Range
    let final_etag = etag.or(range_etag);
    let final_md5 = md5.or(range_md5);
    
    if let Some(size) = head_size.or(range_size) {
        return Ok(ProbeResult { total_size: size, etag: final_etag, md5: final_md5, supports_range });
    }
    
    // Last resort: try a full GET with streaming to read Content-Length
    Err("Could not determine file size".to_string())
}

/// Setup the output file (open existing for resume, or preallocate new)
pub fn setup_file(path: &str, resume_from: u64, total_size: u64) -> Result<Arc<Mutex<std::fs::File>>, String> {
    let file = if resume_from > 0 {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?
    } else {
        disk::preallocate_file(Path::new(path), total_size)
            .map_err(|e| format!("Failed to preallocate file: {}", e))?
    };
    Ok(Arc::new(Mutex::new(file)))
}

/// Initialize the DownloadManager (Resume from segments, simple resume, or new)
pub fn setup_manager(
    total_size: u64, 
    saved: Option<&SavedDownload>, 
    resume_from: u64,
    segment_count: u32,
) -> Arc<Mutex<DownloadManager>> {
    // Use adaptive thread recommendation if user hasn't set a custom count
    let parts = if segment_count == 0 {
        let adaptive = crate::adaptive_threads::recommended_threads();
        if adaptive >= 2 { adaptive } else { 8 }
    } else {
        segment_count
    };
    if let Some(saved_dl) = saved.filter(|s| s.segments.is_some()) {
        let segments = saved_dl.segments.as_ref().unwrap().clone();
        // Validate saved segments are compatible with the current total_size.
        // If the server-side file changed, old segment boundaries may be invalid.
        let segments_valid = !segments.is_empty()
            && segments.iter().all(|s| s.end_byte <= total_size && s.start_byte <= s.end_byte);
        if segments_valid {
            Arc::new(Mutex::new(DownloadManager::new_with_segments(total_size, segments)))
        } else {
            eprintln!("WARNING: Saved segments incompatible with current file size ({}), restarting download", total_size);
            Arc::new(Mutex::new(DownloadManager::new(total_size, parts)))
        }
    } else if resume_from > 0 {
        // Simple resume: single segment from resume_from to end
        let mgr = DownloadManager::new(total_size, 1);
        {
            let mut segs = mgr.segments.write().unwrap();
            segs[0].start_byte = resume_from;
            segs[0].downloaded_cursor = resume_from;
        }
        Arc::new(Mutex::new(mgr))
    } else {
        Arc::new(Mutex::new(DownloadManager::new(total_size, parts)))
    }
}
