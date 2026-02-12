use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use crate::settings::Settings;
use crate::downloader::manager::DownloadManager;

#[derive(Clone)]
pub struct ChatOpsManager {
    download_manager: Arc<Mutex<DownloadManager>>,
    settings: Arc<Mutex<Settings>>,
    client: reqwest::Client,
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
    from: Option<TelegramUser>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    username: Option<String>,
    first_name: Option<String>,
}

#[derive(Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
}

impl ChatOpsManager {
    pub fn new(download_manager: Arc<Mutex<DownloadManager>>, settings: Arc<Mutex<Settings>>) -> Self {
        Self {
            download_manager,
            settings,
            client: reqwest::Client::new(),
        }
    }

    pub fn start(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            this.run_loop().await;
        });
    }

    async fn run_loop(&self) {
        let mut last_update_id = 0;

        loop {
            // check configuration
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

            // Long polling
            let params = [
                ("offset", last_update_id.to_string()),
                ("timeout", "30".to_string()),
            ];

            match self.client.get(&url).query(&params).send().await {
                Ok(resp) => {
                    if let Ok(json) = resp.json::<TelegramResponse<Vec<TelegramUpdate>>>().await {
                        if json.ok {
                            if let Some(updates) = json.result {
                                for update in updates {
                                    last_update_id = update.update_id + 1;
                                    if let Some(msg) = update.message {
                                        self.handle_message(token.clone(), msg).await;
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

    async fn handle_message(&self, token: String, msg: TelegramMessage) {
        let chat_id = msg.chat.id;
        let text = msg.text.unwrap_or_default();
        
        // Auto-save chat_id if not set (first contact)
        {
            let mut s = self.settings.lock().unwrap();
            if s.telegram_chat_id.is_none() {
                s.telegram_chat_id = Some(chat_id.to_string());
                let _ = s.save(); // Persist
                println!("[ChatOps] Auto-detected Chat ID: {}", chat_id);
            }
        }

        let response = if text.starts_with("/start") || text.starts_with("/help") {
            "👋 Welcome to HyperStream ChatOps!\n\nCommands:\n/add <url> - Start a download\n/status - Show active downloads\n/ping - Check connectivity".to_string()
        } else if text.starts_with("/ping") {
            "Pong! 🏓 HyperStream is online.".to_string()
        } else if text.starts_with("/add ") {
            let url = text.strip_prefix("/add ").unwrap().trim();
            if url.is_empty() {
                "❌ Please provide a URL.".to_string()
            } else {
                match self.download_manager.lock().unwrap().add_download(url.to_string(), None).await {
                    Ok(id) => format!("✅ Download started!\nID: {}\nURL: {}", id, url),
                    Err(e) => format!("❌ Failed to start download: {}", e),
                }
            }
        } else if text.starts_with("/status") {
            let dm = self.download_manager.lock().unwrap();
            let tasks = dm.get_tasks();
            let active: Vec<_> = tasks.iter().filter(|t| t.status == "Downloading").collect();
            
            if active.is_empty() {
                "💤 No active downloads.".to_string()
            } else {
                let mut status_msg = format!("🚀 Active Downloads ({}):\n\n", active.len());
                for task in active {
                    status_msg.push_str(&format!("📄 {}\n   Progress: {:.1}%\n   Speed: {:.2} MB/s\n\n", 
                        task.filename, 
                        task.progress,
                        task.speed as f64 / 1024.0 / 1024.0
                    ));
                }
                status_msg
            }
        } else {
            "❓ Unknown command. Try /help".to_string()
        };

        self.send_telegram_message(&token, chat_id.to_string(), &response).await;
    }

    async fn send_telegram_message(&self, token: &str, chat_id: String, text: &str) {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let _ = self.client.post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": text
            }))
            .send()
            .await;
    }

    // Public API to notify external events
    pub async fn notify_completion(&self, filename: &str) {
        let (token, chat_id, enabled) = {
            let s = self.settings.lock().unwrap();
            (s.telegram_bot_token.clone(), s.telegram_chat_id.clone(), s.chatops_enabled)
        };

        if enabled {
            if let (Some(t), Some(c)) = (token, chat_id) {
                self.send_telegram_message(&t, c, &format!("🎉 Download Completed!\n\n📄 {}", filename)).await;
            }
        }
    }
}
