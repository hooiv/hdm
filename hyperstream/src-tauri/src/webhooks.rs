use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WebhookEvent {
    DownloadStart,
    DownloadComplete,
    DownloadError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebhookTemplate {
    Discord,
    Slack,
    Plex,
    Gotify,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<WebhookEvent>,
    pub template: WebhookTemplate,
    pub enabled: bool,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub download_id: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub speed: u64,
    pub filepath: Option<String>,
    pub timestamp: i64,
}

pub struct WebhookManager {
    configs: Arc<Mutex<Vec<WebhookConfig>>>,
    client: reqwest::Client,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            configs: Arc::new(Mutex::new(Vec::new())),
            client: reqwest::Client::new(),
        }
    }

    pub async fn load_configs(&self, configs: Vec<WebhookConfig>) {
        let mut configs_lock = self.configs.lock().await;
        *configs_lock = configs;
    }

    #[allow(dead_code)]
    pub async fn get_configs(&self) -> Vec<WebhookConfig> {
        self.configs.lock().await.clone()
    }

    #[allow(dead_code)]
    pub async fn add_config(&self, config: WebhookConfig) {
        let mut configs = self.configs.lock().await;
        configs.push(config);
    }

    #[allow(dead_code)]
    pub async fn update_config(&self, id: &str, updated: WebhookConfig) -> Result<(), String> {
        let mut configs = self.configs.lock().await;
        if let Some(config) = configs.iter_mut().find(|c| c.id == id) {
            *config = updated;
            Ok(())
        } else {
            Err("Webhook not found".to_string())
        }
    }

    #[allow(dead_code)]
    pub async fn delete_config(&self, id: &str) -> Result<(), String> {
        let mut configs = self.configs.lock().await;
        let initial_len = configs.len();
        configs.retain(|c| c.id != id);
        if configs.len() < initial_len {
            Ok(())
        } else {
            Err("Webhook not found".to_string())
        }
    }

    pub async fn trigger(&self, event: WebhookEvent, payload: WebhookPayload) {
        let configs = self.configs.lock().await.clone();
        let client = self.client.clone();

        for config in configs {
            if !config.enabled {
                continue;
            }

            if !config.events.contains(&event) {
                continue;
            }

            let config_clone = config.clone();
            let payload_clone = payload.clone();
            let client_clone = client.clone();

            // Spawn async task for each webhook (non-blocking)
            tokio::spawn(async move {
                Self::send_webhook(client_clone, config_clone, payload_clone).await;
            });
        }
    }

    async fn send_webhook(
        client: reqwest::Client,
        config: WebhookConfig,
        payload: WebhookPayload,
    ) {
        let body = Self::render_template(&config.template, &payload);

        for attempt in 0..=config.max_retries {
            match client
                .post(&config.url)
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        println!("✅ Webhook '{}' sent successfully", config.name);
                        return;
                    } else {
                        eprintln!(
                            "⚠️  Webhook '{}' failed with status: {}",
                            config.name,
                            response.status()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("❌ Webhook '{}' error: {}", config.name, e);
                }
            }

            // Retry with exponential backoff (if not last attempt)
            if attempt < config.max_retries {
                let delay = Duration::from_secs(2_u64.pow(attempt));
                println!(
                    "🔄 Retrying webhook '{}' in {} seconds... (attempt {}/{})",
                    config.name,
                    delay.as_secs(),
                    attempt + 1,
                    config.max_retries
                );
                tokio::time::sleep(delay).await;
            }
        }

        eprintln!(
            "🚫 Webhook '{}' failed after {} retries",
            config.name, config.max_retries
        );
    }

    fn render_template(template: &WebhookTemplate, payload: &WebhookPayload) -> String {
        match template {
            WebhookTemplate::Discord => Self::render_discord(payload),
            WebhookTemplate::Slack => Self::render_slack(payload),
            WebhookTemplate::Plex => Self::render_plex(payload),
            WebhookTemplate::Gotify => Self::render_gotify(payload),
            WebhookTemplate::Custom => Self::render_custom(payload),
        }
    }

    fn render_discord(payload: &WebhookPayload) -> String {
        let color = match payload.event.as_str() {
            "DownloadComplete" => 3066993, // Green
            "DownloadError" => 15158332,   // Red
            "DownloadStart" => 3447003,    // Blue
            _ => 9807270,                  // Gray
        };

        let size_mb = payload.size as f64 / 1_048_576.0;
        let speed_mbps = payload.speed as f64 / 1_048_576.0;

        serde_json::json!({
            "embeds": [{
                "title": format!("Download {}", payload.event),
                "description": payload.filename,
                "color": color,
                "fields": [
                    {
                        "name": "Size",
                        "value": format!("{:.2} MB", size_mb),
                        "inline": true
                    },
                    {
                        "name": "Speed",
                        "value": format!("{:.2} MB/s", speed_mbps),
                        "inline": true
                    }
                ],
                "timestamp": chrono::DateTime::from_timestamp(payload.timestamp, 0)
                    .unwrap()
                    .to_rfc3339(),
            }]
        })
        .to_string()
    }

    fn render_slack(payload: &WebhookPayload) -> String {
        let size_mb = payload.size as f64 / 1_048_576.0;

        serde_json::json!({
            "blocks": [{
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("*Download {}*\n{}\nSize: {:.2} MB", 
                        payload.event, payload.filename, size_mb)
                }
            }]
        })
        .to_string()
    }

    fn render_plex(payload: &WebhookPayload) -> String {
        serde_json::json!({
            "event": format!("download.{}", payload.event.to_lowercase()),
            "file": payload.filepath.as_ref().unwrap_or(&payload.filename),
            "timestamp": payload.timestamp,
        })
        .to_string()
    }

    fn render_gotify(payload: &WebhookPayload) -> String {
        let size_mb = payload.size as f64 / 1_048_576.0;

        serde_json::json!({
            "title": format!("Download {}", payload.event),
            "message": format!("{}\nSize: {:.2} MB", payload.filename, size_mb),
            "priority": match payload.event.as_str() {
                "DownloadError" => 8,
                "DownloadComplete" => 5,
                _ => 3
            }
        })
        .to_string()
    }

    fn render_custom(payload: &WebhookPayload) -> String {
        serde_json::to_string(payload).unwrap_or_default()
    }
}

// Utility to generate unique IDs
pub fn generate_webhook_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("webhook_{}", timestamp)
}
