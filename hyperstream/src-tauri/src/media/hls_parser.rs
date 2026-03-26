use m3u8_rs::{MasterPlaylist, MediaPlaylist, Playlist};
use reqwest::Client;
use serde::Serialize;
use url::Url;

/// A quality variant from a master playlist.
#[derive(Debug, Clone, Serialize)]
pub struct HlsVariant {
    pub bandwidth: u64,
    /// E.g. "1920x1080"
    pub resolution: Option<String>,
    pub url: String,
    /// Codec string from CODECS attribute (e.g. "avc1.640028,mp4a.40.2")
    pub codecs: Option<String>,
    /// Frames per second from FRAME-RATE attribute
    pub frame_rate: Option<f32>,
    /// Human-readable quality label derived from resolution / bandwidth
    pub quality_label: String,
}

/// One media segment within a variant playlist.
#[derive(Debug, Clone, Serialize)]
pub struct HlsSegment {
    pub url: String,
    pub duration: f32,
    pub sequence: u64,
    /// URI of the AES-128 key for this segment (if encrypted).
    pub key_uri: Option<String>,
    /// Hex-encoded IV (e.g. "0x000000000000000000000000000000XX").
    /// Stored exactly as provided by the playlist; `decrypt.rs` decodes it.
    pub key_iv: Option<String>,
}

/// Parsed HLS playlist.
#[derive(Debug, Clone, Serialize)]
pub struct HlsStream {
    /// Non-empty only for master playlists.
    pub variants: Vec<HlsVariant>,
    /// Non-empty only for media playlists.
    pub segments: Vec<HlsSegment>,
    pub target_duration: f32,
    pub is_master: bool,
    /// True when the media playlist does NOT contain `#EXT-X-ENDLIST`
    /// (i.e. this is a live / event stream, not a finished VOD).
    pub is_live: bool,
}

pub struct HlsParser {
    client: Client,
}

impl HlsParser {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Parse a URL and return the HLS stream info.
    /// For a master playlist, returns the variants list (`is_master = true`).
    /// For a media playlist, returns the segment list (`is_master = false`).
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
        // SSRF protection: block requests to private/loopback addresses
        crate::api_replay::validate_url_not_private(url)?;

        let response = self.client.get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
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

    pub fn process_master_playlist(&self, pl: MasterPlaylist, base_url: &Url) -> HlsStream {
        let mut variants = Vec::new();

        for variant in pl.variants {
            let url = match base_url.join(&variant.uri) {
                Ok(u) => u.to_string(),
                Err(_) => variant.uri.clone(),
            };

            let resolution = variant.resolution.map(|r| format!("{}x{}", r.width, r.height));
            let codecs = variant.codecs;
            let frame_rate = variant.frame_rate.map(|fr| fr as f32);
            let quality_label = build_quality_label(variant.bandwidth, &resolution);

            variants.push(HlsVariant {
                bandwidth: variant.bandwidth,
                resolution,
                url,
                codecs,
                frame_rate,
                quality_label,
            });
        }

        // Sort by bandwidth descending (best quality first)
        variants.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));

        HlsStream {
            variants,
            segments: Vec::new(),
            target_duration: 0.0,
            is_master: true,
            is_live: false,
        }
    }

    pub fn process_media_playlist(&self, pl: MediaPlaylist, base_url: &Url) -> HlsStream {
        let mut segments = Vec::new();
        let mut sequence = pl.media_sequence;

        // Track the current key — HLS allows a key to apply to multiple segments
        // until a new #EXT-X-KEY tag overrides it.
        let mut current_key_uri: Option<String> = None;
        let mut current_key_iv: Option<String> = None;

        for segment in &pl.segments {
            // Update current key if this segment declares one
            if let Some(ref key) = segment.key {
                let new_uri = key.uri.as_ref()
                    .and_then(|u| base_url.join(u).ok().map(|u| u.to_string()));
                // m3u8-rs 6.x represents method as a KeyMethod enum:
                // None | Aes128 | SampleAes | Other(String)
                // Anything that is not a real URI-bearing encryption method
                // (i.e., KeyMethod::None or completely absent) clears encryption.
                let is_no_encryption = matches!(
                    key.method,
                    m3u8_rs::KeyMethod::None
                );
                if is_no_encryption {
                    current_key_uri = None;
                    current_key_iv = None;
                } else {
                    if new_uri.is_some() {
                        current_key_uri = new_uri;
                    }
                    current_key_iv = key.iv.clone();
                }
            }

            let url = match base_url.join(&segment.uri) {
                Ok(u) => u.to_string(),
                Err(_) => segment.uri.clone(),
            };

            segments.push(HlsSegment {
                url,
                duration: segment.duration,
                sequence,
                key_uri: current_key_uri.clone(),
                key_iv: current_key_iv.clone(),
            });

            sequence += 1;
        }

        HlsStream {
            variants: Vec::new(),
            segments,
            target_duration: pl.target_duration as f32,
            is_master: false,
            // A live/event stream does NOT have EXT-X-ENDLIST
            is_live: !pl.end_list,
        }
    }
}

/// Build a human-readable quality label like "1080p", "720p", or "4.5 Mbps".
fn build_quality_label(bandwidth: u64, resolution: &Option<String>) -> String {
    if let Some(res) = resolution {
        // Extract height from "WxH"
        if let Some(height_str) = res.split('x').nth(1) {
            if let Ok(h) = height_str.parse::<u32>() {
                return format!("{}p", h);
            }
        }
    }
    // Fallback to bandwidth
    if bandwidth >= 1_000_000 {
        format!("{:.1} Mbps", bandwidth as f64 / 1_000_000.0)
    } else {
        format!("{} Kbps", bandwidth / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_parser() -> HlsParser {
        HlsParser::new(reqwest::Client::new())
    }

    fn base() -> Url {
        Url::parse("http://localhost/stream/").unwrap()
    }

    #[test]
    fn test_parse_simple_media_playlist() {
        let parser = make_parser();
        let stream = parser.process_media_playlist(m3u8_rs::MediaPlaylist {
            version: None,
            media_sequence: 0,
            target_duration: 5,
            segments: vec![
                m3u8_rs::MediaSegment {
                    uri: "seg1.ts".to_string(),
                    duration: 5.0,
                    key: None,
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
                m3u8_rs::MediaSegment {
                    uri: "seg2.ts".to_string(),
                    duration: 5.0,
                    key: None,
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
            ],
            end_list: true,
            ..Default::default()
        }, &base());
        assert_eq!(stream.segments.len(), 2);
        assert!(!stream.is_master);
        assert!(!stream.is_live, "VOD stream should not be live");
    }

    #[test]
    fn live_stream_detected_when_no_end_list() {
        let parser = make_parser();
        let stream = parser.process_media_playlist(m3u8_rs::MediaPlaylist {
            version: None,
            media_sequence: 100,
            target_duration: 4,
            segments: vec![
                m3u8_rs::MediaSegment {
                    uri: "live1.ts".to_string(),
                    duration: 4.0,
                    key: None,
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
            ],
            end_list: false, // <-- no EXT-X-ENDLIST
            ..Default::default()
        }, &base());
        assert!(stream.is_live, "Stream without EXT-X-ENDLIST must be detected as live");
        assert!(!stream.is_master);
    }

    #[test]
    fn key_propagates_across_multiple_segments() {
        let parser = make_parser();
        let key = m3u8_rs::Key {
            method: m3u8_rs::KeyMethod::AES128,
            uri: Some("https://cdn.example.com/key".to_string()),
            iv: Some("0x00000000000000000000000000000001".to_string()),
            keyformat: None,
            keyformatversions: None,
        };
        let stream = parser.process_media_playlist(m3u8_rs::MediaPlaylist {
            version: None,
            media_sequence: 0,
            target_duration: 6,
            segments: vec![
                m3u8_rs::MediaSegment {
                    uri: "enc1.ts".to_string(),
                    duration: 6.0,
                    key: Some(key),
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
                m3u8_rs::MediaSegment {
                    uri: "enc2.ts".to_string(),
                    duration: 6.0,
                    key: None, // inherits previous key
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
            ],
            end_list: true,
            ..Default::default()
        }, &base());

        assert_eq!(stream.segments.len(), 2);
        // Both segments must have the same key URI (key propagation)
        assert!(stream.segments[0].key_uri.is_some(), "Segment 0 must have key");
        assert!(stream.segments[1].key_uri.is_some(), "Segment 1 must inherit key");
        assert_eq!(
            stream.segments[0].key_uri,
            stream.segments[1].key_uri,
            "Key must propagate across segments"
        );
    }

    #[test]
    fn none_key_clears_encryption() {
        let parser = make_parser();
        let enc_key = m3u8_rs::Key {
            method: m3u8_rs::KeyMethod::AES128,
            uri: Some("https://cdn.example.com/key".to_string()),
            iv: None,
            keyformat: None,
            keyformatversions: None,
        };
        let clear_key = m3u8_rs::Key {
            method: m3u8_rs::KeyMethod::None,
            uri: None,
            iv: None,
            keyformat: None,
            keyformatversions: None,
        };
        let stream = parser.process_media_playlist(m3u8_rs::MediaPlaylist {
            version: None,
            media_sequence: 0,
            target_duration: 6,
            segments: vec![
                m3u8_rs::MediaSegment {
                    uri: "enc.ts".to_string(),
                    duration: 6.0,
                    key: Some(enc_key),
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
                m3u8_rs::MediaSegment {
                    uri: "clear.ts".to_string(),
                    duration: 6.0,
                    key: Some(clear_key),
                    byte_range: None,
                    discontinuity: false,
                    ..Default::default()
                },
            ],
            end_list: true,
            ..Default::default()
        }, &base());

        assert!(stream.segments[0].key_uri.is_some(), "First segment is encrypted");
        assert!(stream.segments[1].key_uri.is_none(), "Second segment cleared encryption");
    }

    #[test]
    fn quality_label_from_resolution() {
        assert_eq!(build_quality_label(5_000_000, &Some("1920x1080".to_string())), "1080p");
        assert_eq!(build_quality_label(2_500_000, &Some("1280x720".to_string())), "720p");
        assert_eq!(build_quality_label(500_000, &None), "500 Kbps");
        assert_eq!(build_quality_label(1_500_000, &None), "1.5 Mbps");
    }

    #[test]
    fn variant_sorted_descending_bandwidth() {
        let parser = make_parser();
        let pl = m3u8_rs::MasterPlaylist {
            version: None,
            variants: vec![
                m3u8_rs::VariantStream {
                    uri: "low.m3u8".to_string(),
                    bandwidth: 500_000,
                    average_bandwidth: None,
                    codecs: None,
                    resolution: None,
                    frame_rate: None,
                    audio: None,
                    video: None,
                    subtitles: None,
                    closed_captions: None,
                    is_i_frame: false,
                    ..Default::default()
                },
                m3u8_rs::VariantStream {
                    uri: "high.m3u8".to_string(),
                    bandwidth: 5_000_000,
                    average_bandwidth: None,
                    codecs: None,
                    resolution: None,
                    frame_rate: None,
                    audio: None,
                    video: None,
                    subtitles: None,
                    closed_captions: None,
                    is_i_frame: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let stream = parser.process_master_playlist(pl, &base());
        assert_eq!(stream.variants.len(), 2);
        assert!(
            stream.variants[0].bandwidth >= stream.variants[1].bandwidth,
            "Variants must be sorted best quality first"
        );
    }
}
