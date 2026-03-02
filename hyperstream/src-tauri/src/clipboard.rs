use arboard::Clipboard;
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;

lazy_static::lazy_static! {
    static ref URL_REGEX: Regex = Regex::new(
        r"https?://[^\s<>\[\]{}|\\^`\x00-\x1f]+\.(zip|rar|7z|exe|msi|iso|mp4|mkv|avi|mov|mp3|flac|pdf|doc|docx|dmg|tar|gz|xz)(\?[^\s]*)?"
    ).unwrap();
}

pub struct ClipboardMonitor {
    enabled: Arc<AtomicBool>,
    last_content: Arc<std::sync::Mutex<String>>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            last_content: Arc::new(std::sync::Mutex::new(String::new())),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn start<R: tauri::Runtime>(&self, app_handle: tauri::AppHandle<R>) {
        let enabled = self.enabled.clone();
        let last_content = self.last_content.clone();

        std::thread::spawn(move || {
            let mut clipboard = match Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to initialize clipboard: {}", e);
                    return;
                }
            };

            loop {
                std::thread::sleep(Duration::from_millis(500));

                if !enabled.load(Ordering::SeqCst) {
                    continue;
                }

                let text = match clipboard.get_text() {
                    Ok(t) => t,
                    Err(_) => continue, // Non-text content or error
                };

                // Check if content changed
                {
                    let mut last = last_content.lock().unwrap();
                    if *last == text {
                        continue;
                    }
                    *last = text.clone();
                }

                // Check if it looks like a downloadable URL
                if let Some(matched) = URL_REGEX.find(&text) {
                    let url = matched.as_str().to_string();
                    let filename = url.split('/').last()
                        .and_then(|s| s.split('?').next())
                        .unwrap_or("download")
                        .to_string();


                    // Emit event to frontend
                    let _ = app_handle.emit("clipboard_url", serde_json::json!({
                        "url": url,
                        "filename": filename
                    }));
                }
            }
        });
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    pub static ref CLIPBOARD_MONITOR: ClipboardMonitor = ClipboardMonitor::new();
}
