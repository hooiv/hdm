use reqwest::Client;

/// A stateless, greedy data acquirer.
/// The Swarm knows nothing about "Files", "Pauses", "Progress Bars", or "Users".
/// It ONLY understands: "Go to U, get byte range R, verify against H, put in Cache."
pub struct StatelessSwarmWorker {
    pub worker_id: usize,
    pub http_client: Client,
}

impl StatelessSwarmWorker {
    pub fn new(id: usize) -> Self {
        Self {
            worker_id: id,
            http_client: Client::new(),
        }
    }

    /// Fetches a raw block of bytes. If the user "pauses" the app, the worker
    /// simply stops receiving `hunt` assignments from the global queue. It has no internal state to pause.
    pub async fn hunt(&self, source_url: &str, start_byte: u64, end_byte: u64) -> Result<Vec<u8>, String> {
        // SSRF protection: block requests to private/loopback addresses
        crate::api_replay::validate_url_not_private(source_url)?;

        let resp = self.http_client.get(source_url)
            .header("Range", format!("bytes={}-{}", start_byte, end_byte))
            .send()
            .await
            .map_err(|e| format!("Swarm worker network failure: {}", e))?;
            
        if !resp.status().is_success() {
            return Err(format!("Hostile source block: {}", resp.status()));
        }

        // Validate response size against expected range to prevent OOM
        let expected_size = end_byte.saturating_sub(start_byte) + 1;
        let max_allowed = expected_size.saturating_add(1024); // small tolerance
        if let Some(cl) = resp.content_length() {
            if cl > max_allowed {
                return Err(format!("Response too large: {} bytes (expected ~{})", cl, expected_size));
            }
        }

        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() as u64 > max_allowed {
            return Err(format!("Response body too large: {} bytes", bytes.len()));
        }
        // In reality, it would hash `bytes` and immediately drop into `cas_manager` storage.
        Ok(bytes.to_vec())
    }
}
