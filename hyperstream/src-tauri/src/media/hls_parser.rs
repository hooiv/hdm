use m3u8_rs::{MasterPlaylist, MediaPlaylist, Playlist};
use reqwest::Client;
use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct HlsVariant {
    pub bandwidth: u64,
    pub resolution: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HlsSegment {
    pub url: String,
    pub duration: f32,
    pub sequence: u64,
    pub key_uri: Option<String>,
    pub key_iv: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HlsStream {
    pub variants: Vec<HlsVariant>,
    pub segments: Vec<HlsSegment>,
    pub target_duration: f32,
    pub is_master: bool,
}

pub struct HlsParser {
    client: Client,
}

impl HlsParser {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Parse a URL and return the HLS stream info
    /// If it's a master playlist, it returns the variants.
    /// If it's a media playlist, it returns the segments.
    pub async fn parse(&self, url: &str) -> Result<HlsStream, String> {
        let content = self.fetch_manifest(url).await?;
        let base_url = Url::parse(url).map_err(|e| e.to_string())?;

        match m3u8_rs::parse_playlist(&content) {
            Ok((_, Playlist::MasterPlaylist(pl))) => {
                Ok(self.process_master_playlist(pl, &base_url))
            }
            Ok((_, Playlist::MediaPlaylist(pl))) => {
                Ok(self.process_media_playlist(pl, &base_url))
            }
            Err(e) => Err(format!("Failed to parse HLS manifest: {:?}", e)),
        }
    }

    async fn fetch_manifest(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = self.client.get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch manifest: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Server returned error: {}", response.status()));
        }

        response.bytes().await
            .map(|b| b.to_vec())
            .map_err(|e| format!("Failed to read manifest body: {}", e))
    }

    fn process_master_playlist(&self, pl: MasterPlaylist, base_url: &Url) -> HlsStream {
        let mut variants = Vec::new();

        for variant in pl.variants {
            let url = match base_url.join(&variant.uri) {
                Ok(u) => u.to_string(),
                Err(_) => variant.uri.clone(),
            };

            variants.push(HlsVariant {
                bandwidth: variant.bandwidth,
                resolution: variant.resolution.map(|r| format!("{}x{}", r.width, r.height)),
                url,
            });
        }

        // Sort by bandwidth descending (best quality first)
        variants.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));

        HlsStream {
            variants,
            segments: Vec::new(),
            target_duration: 0.0,
            is_master: true,
        }
    }

    fn process_media_playlist(&self, pl: MediaPlaylist, base_url: &Url) -> HlsStream {
        let mut segments = Vec::new();
        let mut sequence = pl.media_sequence;

        for segment in pl.segments {
            let url = match base_url.join(&segment.uri) {
                Ok(u) => u.to_string(),
                Err(_) => segment.uri.clone(),
            };

            let (key_uri, key_iv) = if let Some(key) = segment.key {
                let uri = key.uri.and_then(|u| base_url.join(&u).ok().map(|u| u.to_string()));
                let iv = key.iv;
                (uri, iv)
            } else {
                (None, None)
            };

            segments.push(HlsSegment {
                url,
                duration: segment.duration,
                sequence,
                key_uri,
                key_iv,
            });

            sequence += 1;
        }

        HlsStream {
            variants: Vec::new(),
            segments,
            target_duration: pl.target_duration as f32,
            is_master: false,
        }
    }
}
