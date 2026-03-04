use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::Deserialize;
use serde_json::json;
use crate::settings;
use crate::persistence;

#[derive(Clone)]
pub struct ChatOpsManager {
    settings: Arc<Mutex<settings::Settings>>,
    client: reqwest::Client,
    /// Pending URLs added via /add command, polled by frontend
    pending_urls: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: u64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    chat: TelegramChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
}

impl ChatOpsManager {
    pub fn new(settings: Arc<Mutex<settings::Settings>>) -> Self {
        Self {
            settings,
            client: reqwest::Client::new(),
            pending_urls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Take all pending URLs (called from Tauri command to drain the queue)
    pub fn take_pending_urls(&self) -> Vec<String> {
        let mut urls = self.pending_urls.lock().unwrap();
        std::mem::take(&mut *urls)
    }

    pub fn start(&self) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            this.run_loop().await;
        });
    }

    async fn run_loop(&self) {
        let mut last_update_id: u64 = 0;

        loop {
            // Check configuration
            let (token, enabled) = {
                let s = self.settings.lock().unwrap();
                (s.telegram_bot_token.clone(), s.chatops_enabled)
            };

            if !enabled || token.is_none() {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            let token = token.unwrap();
            let url = format!("https://api.telegram.org/bot{}/getUpdates", token);

            let params = [
                ("offset", last_update_id.to_string()),
                ("timeout", "30".to_string()),
            ];

            match self.client.get(&url).query(&params).send().await {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<TelegramResponse<Vec<TelegramUpdate>>>().await {
                        if body.ok {
                            if let Some(updates) = body.result {
                                for update in updates {
                                    last_update_id = update.update_id + 1;
                                    if let Some(msg) = update.message {
                                        self.handle_message(&token, msg).await;
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    eprintln!("[ChatOps] Polling error: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn handle_message(&self, token: &str, msg: TelegramMessage) {
        let chat_id = msg.chat.id;
        let text = msg.text.unwrap_or_default();
        
        // Auto-save chat_id if not set
        {
            let mut s = self.settings.lock().unwrap();
            if s.telegram_chat_id.is_none() {
                s.telegram_chat_id = Some(chat_id.to_string());
                // Persist via save_settings
                let _ = settings::save_settings(&s);
                println!("[ChatOps] Auto-detected Chat ID: {}", chat_id);
            }
        }

        let response = if text.starts_with("/start") || text.starts_with("/help") {
            "👋 *HyperStream ChatOps*\n\nCommands:\n/add <url> - Queue a download\n/status - Active downloads\n/ping - Check connection".to_string()
        } else if text.starts_with("/ping") {
            "🏓 Pong! HyperStream is online.".to_string()
        } else if text.starts_with("/add ") {
            let url = text.strip_prefix("/add ").unwrap_or("").trim();
            if url.is_empty() {
                "❌ Provide a URL: /add https://example.com/file.zip".to_string()
            } else {
                // Queue URL for the frontend to pick up
                self.pending_urls.lock().unwrap().push(url.to_string());
                format!("✅ Queued for download:\n{}", url)
            }
        } else if text.starts_with("/status") {
            // Read current downloads from persistence
            match persistence::load_downloads() {
                Ok(downloads) => {
                    let active: Vec<_> = downloads.iter()
                        .filter(|d| d.status == "Downloading")
                        .collect();
                    
                    if active.is_empty() {
                        let done_count = downloads.iter().filter(|d| d.status == "Done").count();
                        format!("💤 No active downloads.\n📊 {} completed total.", done_count)
                    } else {
                        let mut msg = format!("🚀 *Active ({})* :\n\n", active.len());
                        for d in active.iter().take(5) {
                            let pct = if d.total_size > 0 {
                                d.downloaded_bytes as f64 / d.total_size as f64 * 100.0
                            } else { 0.0 };
                            msg.push_str(&format!("📄 {}\n   {:.1}% of {}\n\n",
                                d.filename,
                                pct,
                                format_bytes(d.total_size),
                            ));
                        }
                        msg
                    }
                }
                Err(_) => "⚠️ Could not read download list.".to_string()
            }
        } else {
            "❓ Unknown command. Try /help".to_string()
        };

        self.send_message(token, chat_id, &response).await;
    }

    async fn send_message(&self, token: &str, chat_id: i64, text: &str) {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let _ = self.client.post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": text
            }))
            .send()
            .await;
    }

    /// Notify download completion via Telegram
    pub async fn notify_completion(&self, filename: &str) {
        let (token, chat_id, enabled) = {
            let s = self.settings.lock().unwrap();
            (s.telegram_bot_token.clone(), s.telegram_chat_id.clone(), s.chatops_enabled)
        };

        if enabled {
            if let (Some(t), Some(c)) = (token, chat_id) {
                if let Ok(cid) = c.parse::<i64>() {
                    self.send_message(&t, cid, &format!("🎉 Download Complete!\n📄 {}", filename)).await;
                }
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 { return "0 B".to_string(); }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(1024.0).floor() as usize;
    let i = i.min(units.len() - 1);
    format!("{:.1} {}", bytes as f64 / 1024_f64.powi(i as i32), units[i])
}
