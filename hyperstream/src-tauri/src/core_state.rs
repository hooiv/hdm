use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::broadcast;
use crate::downloader::manager::DownloadManager;
use crate::http_server;
use crate::network;

pub type SlimSegment = (u32, u64, u64, u64, u8, u64);

#[derive(Clone, serde::Serialize)]
pub struct Payload {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
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
    /// Segments as returned by `media::HlsParser` (url, duration, keys etc).
    pub segments: Vec<crate::media::HlsSegment>,
    /// Pre‑computed byte size of each segment; same index as `segments`.
    pub segment_sizes: Vec<u64>,
    /// Cumulative number of bytes written so far.
    pub downloaded: Arc<std::sync::atomic::AtomicU64>,
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
}
