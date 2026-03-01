use std::path::Path;
use std::sync::{Arc, Mutex};
use rquest::Client;
use crate::downloader::manager::DownloadManager;
use crate::downloader::disk;
use crate::persistence::SavedDownload;

/// Strategies to determine file size (HEAD, Range 0-1, etc.)
pub async fn determine_total_size(client: &Client, url: &str) -> Result<(u64, Option<String>, Option<String>), String> {
    // 1. Try HEAD request
    let head_resp = client.head(url).send().await.map_err(|e| e.to_string())?;
    
    let etag = head_resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let md5 = head_resp.headers().get("content-md5").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    
    if let Some(len) = head_resp.content_length() {
        if len > 0 { return Ok((len, etag, md5)); }
    }

    // Manual content-length check if not parsed automatically
    if let Some(len_header) = head_resp.headers().get("content-length") {
        if let Ok(len_str) = len_header.to_str() {
            if let Ok(len) = len_str.parse::<u64>() {
                return Ok((len, etag, md5));
            }
        }
    }

    // 2. Try Range 0-1 request
    let range_resp = client.get(url).header("Range", "bytes=0-1").send().await.map_err(|e| e.to_string())?;
    
    let etag = range_resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let md5 = range_resp.headers().get("content-md5").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    
    if let Some(content_range) = range_resp.headers().get("content-range") {
        let s = content_range.to_str().unwrap_or("");
        if let Some(slash_pos) = s.find('/') {
            if let Ok(size) = s[slash_pos + 1..].parse::<u64>() {
                return Ok((size, etag, md5));
            }
        }
    }

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
    resume_from: u64
) -> Arc<Mutex<DownloadManager>> {
    if let Some(saved_dl) = saved.filter(|s| s.segments.is_some()) {
        println!("DEBUG: Resuming from saved segments");
        let segments = saved_dl.segments.as_ref().unwrap().clone();
        Arc::new(Mutex::new(DownloadManager::new_with_segments(total_size, segments)))
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
        Arc::new(Mutex::new(DownloadManager::new(total_size, 8)))
    }
}
