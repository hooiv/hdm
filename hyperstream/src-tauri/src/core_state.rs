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
    pub p2p_node: Arc<network::p2p::P2PNode>,
    pub p2p_file_map: http_server::FileMap,
    pub torrent_manager: Option<Arc<network::bittorrent::manager::TorrentManager>>,
    pub connection_manager: network::connection_manager::ConnectionManager,
    pub chatops_manager: Arc<network::chatops::ChatOpsManager>,
}
