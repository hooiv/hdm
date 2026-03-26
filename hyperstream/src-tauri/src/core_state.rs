use crate::downloader::manager::DownloadManager;
use crate::http_server;
use crate::network;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub type SlimSegment = (u32, u64, u64, u64, u8, u64);

#[derive(Clone, serde::Serialize)]
pub struct Payload {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: u64,
    pub segments: Vec<SlimSegment>,
}

/// Session information for an ongoing HLS download.  Unlike a normal
/// HTTP download we cannot simply rely on range requests, so we maintain
/// our own list of playlist segments, sizes and an atomic byte counter.
/// Resume is supported by re‑parsing the manifest and skipping the already
/// downloaded bytes.
pub struct HlsSession {
    /// URL of the playlist (could be a variant entry if the user selected one).
    pub manifest_url: String,
    /// Final destination path for the stitched output file.
    pub output_path: String,
    /// Segments as returned by `media::HlsParser` (url, duration, keys etc).
    pub segments: Vec<crate::media::HlsSegment>,
    /// Pre‑computed byte size of each segment; same index as `segments`.
    pub segment_sizes: Vec<u64>,
    /// Cumulative number of bytes written so far.
    pub downloaded: Arc<std::sync::atomic::AtomicU64>,
    pub speed_bps: Arc<std::sync::atomic::AtomicU64>,
    pub stop_tx: broadcast::Sender<()>,
    pub file_writer: Arc<Mutex<std::fs::File>>,
}

pub struct DownloadSession {
    #[allow(dead_code)]
    pub manager: Arc<Mutex<DownloadManager>>,
    pub stop_tx: broadcast::Sender<()>,
    #[allow(dead_code)]
    pub url: String,
    #[allow(dead_code)]
    pub path: String,
    #[allow(dead_code)]
    pub file_writer: Arc<Mutex<std::fs::File>>,
    /// Optional group context: (group_id, member_id)
    pub group_context: Option<(String, String)>,
}

pub struct AppState {
    pub downloads: Mutex<HashMap<String, DownloadSession>>,
    /// HLS downloads only, keyed by the same ID used for normal downloads so
    /// front‑end controls (pause/resume) can treat them uniformly.
    pub hls_sessions: Mutex<HashMap<String, HlsSession>>,
    /// DASH downloads (video+audio → mux), keyed by the same download ID.
    pub dash_sessions: Mutex<HashMap<String, crate::engine::dash::DashSession>>,
    pub p2p_node: Arc<network::p2p::P2PNode>,
    pub p2p_file_map: http_server::FileMap,
    pub torrent_manager: Option<Arc<network::bittorrent::manager::TorrentManager>>,
    pub connection_manager: network::connection_manager::ConnectionManager,
    pub chatops_manager: Arc<network::chatops::ChatOpsManager>,
    pub recovery_manager: crate::download_recovery::DownloadRecoveryManager,
}

impl AppState {
    pub fn unregister_streaming_source(&self, id: &str) {
        let mut map = self.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(id);
    }

    pub fn has_active_download_id(&self, id: &str) -> bool {
        {
            let downloads = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
            if downloads.contains_key(id) {
                return true;
            }
        }

        {
            let hls = self.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
            if hls.contains_key(id) {
                return true;
            }
        }

        let dash = self.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        dash.contains_key(id)
    }

    pub fn has_active_download_url(&self, url: &str) -> bool {
        let normalized = crate::normalize_download_url(url);
        if normalized.is_empty() {
            return false;
        }

        {
            let downloads = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
            if downloads
                .values()
                .any(|session| crate::normalize_download_url(&session.url) == normalized)
            {
                return true;
            }
        }

        {
            let hls = self.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
            if hls
                .values()
                .any(|session| crate::normalize_download_url(&session.manifest_url) == normalized)
            {
                return true;
            }
        }

        let dash = self.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        dash.values()
            .any(|session| crate::normalize_download_url(&session.manifest_url) == normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::manager::DownloadManager;
    use std::path::PathBuf;

    fn make_test_state() -> AppState {
        AppState {
            downloads: Mutex::new(HashMap::new()),
            hls_sessions: Mutex::new(HashMap::new()),
            dash_sessions: Mutex::new(HashMap::new()),
            p2p_node: Arc::new(network::p2p::P2PNode::disabled()),
            p2p_file_map: Arc::new(Mutex::new(HashMap::new())),
            torrent_manager: None,
            connection_manager: network::connection_manager::ConnectionManager::default(),
            chatops_manager: Arc::new(network::chatops::ChatOpsManager::new(Arc::new(Mutex::new(
                crate::settings::load_settings(),
            )))),
            recovery_manager: crate::download_recovery::DownloadRecoveryManager::new(),
        }
    }

    fn make_temp_writer(name: &str) -> Arc<Mutex<std::fs::File>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hyperstream-core-state-{name}-{unique}.tmp"));
        Arc::new(Mutex::new(
            std::fs::File::create(path).expect("temp writer"),
        ))
    }

    #[test]
    fn unregister_streaming_source_removes_registered_entry() {
        let state = make_test_state();
        {
            let mut map = state.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
            map.insert(
                "download-1".to_string(),
                crate::http_server::StreamingSource::FileSystem(PathBuf::from("/tmp/file.bin")),
            );
        }

        state.unregister_streaming_source("download-1");

        let map = state.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!map.contains_key("download-1"));
    }

    #[test]
    fn has_active_download_id_ignores_stale_stream_registration_without_session() {
        let state = make_test_state();
        {
            let mut map = state.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
            map.insert(
                "download-1".to_string(),
                crate::http_server::StreamingSource::FileSystem(PathBuf::from("/tmp/file.bin")),
            );
        }

        assert!(!state.has_active_download_id("download-1"));
    }

    #[test]
    fn has_active_download_url_matches_across_protocols() {
        let state = make_test_state();

        let (http_stop_tx, _) = broadcast::channel(1);
        state
            .downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                "http-1".to_string(),
                DownloadSession {
                    manager: Arc::new(Mutex::new(DownloadManager::new(100, 1))),
                    stop_tx: http_stop_tx,
                    url: "HTTPS://Example.com:443/file.bin#frag".to_string(),
                    path: "/tmp/file.bin".to_string(),
                    file_writer: make_temp_writer("http"),
                    group_context: None,
                },
            );

        let (hls_stop_tx, _) = broadcast::channel(1);
        state
            .hls_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                "hls-1".to_string(),
                HlsSession {
                    manifest_url: "https://example.com/live.m3u8".to_string(),
                    output_path: "/tmp/video.mp4".to_string(),
                    segments: Vec::new(),
                    segment_sizes: Vec::new(),
                    downloaded: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    speed_bps: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    stop_tx: hls_stop_tx,
                    file_writer: make_temp_writer("hls"),
                },
            );

        let (dash_stop_tx, _) = broadcast::channel(1);
        state
            .dash_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                "dash-1".to_string(),
                crate::engine::dash::DashSession {
                    manifest_url: "https://example.com/manifest.mpd".to_string(),
                    output_path: "/tmp/dash.mp4".to_string(),
                    video_rep: None,
                    audio_rep: None,
                    video_total: 0,
                    audio_total: 0,
                    downloaded: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    stop_tx: dash_stop_tx,
                },
            );

        assert!(state.has_active_download_url("https://example.com/file.bin"));
        assert!(state.has_active_download_url("https://example.com/live.m3u8#master"));
        assert!(state.has_active_download_url("https://example.com/manifest.mpd#player"));
        assert!(!state.has_active_download_url("https://example.com/other.bin"));
    }
}
