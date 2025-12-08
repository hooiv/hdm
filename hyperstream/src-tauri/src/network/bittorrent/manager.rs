use std::sync::Arc;
use std::path::PathBuf;
use librqbit::{Session, AddTorrent, AddTorrentOptions};
use librqbit::api::{Api, TorrentIdOrHash};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncSeek};

#[derive(Clone, Serialize)]
pub struct TorrentStatus {
    pub id: usize,
    pub name: String,
    pub progress_percent: f64,
    pub speed_download: u64,
    pub speed_upload: u64,
    pub peers: usize,
    pub state: String,
}

pub struct TorrentManager {
    session: Arc<Session>,
    api: Api,
}

impl TorrentManager {
    pub async fn new(output_dir: PathBuf) -> anyhow::Result<Self> {
        let session = Session::new(output_dir).await?;
        // Api::new(session, rust_log_reload_tx) - guessing 2nd arg is Option<Sender>
        let api = Api::new(session.clone(), None);
        Ok(Self { session, api })
    }

    pub async fn add_magnet(&self, magnet_url: &str) -> anyhow::Result<usize> {
        let handle_response = self.session.add_torrent(
            AddTorrent::from_url(magnet_url), 
            Some(AddTorrentOptions::default())
        ).await?;
        
        use librqbit::AddTorrentResponse;
        match handle_response {
            AddTorrentResponse::Added(id, _handle) => Ok(id),
            AddTorrentResponse::AlreadyManaged(id, _handle) => Ok(id),
            _ => anyhow::bail!("Unknown response from add_torrent"),
        }
    }

    pub fn get_largest_file_id(&self, id: usize) -> Option<usize> {
        // Use Api to get details
        let details = self.api.api_torrent_details(TorrentIdOrHash::Id(id)).ok()?;
        
        let mut max_len = 0;
        let mut best_id = None;
        
        if let Some(files) = details.files {
            for (idx, file) in files.iter().enumerate() {
                 if file.length > max_len {
                     max_len = file.length;
                     best_id = Some(idx);
                 }
            }
        }
        
        best_id
    }

    pub fn get_file_length(&self, torrent_id: usize, file_id: usize) -> Option<u64> {
        let details = self.api.api_torrent_details(TorrentIdOrHash::Id(torrent_id)).ok()?;
        let files = details.files?;
        files.get(file_id).map(|f| f.length)
    }

    // Return the stream directly. The caller will wrap it or use it.
    // We return a boxed object if traits are tricky, but try RPIT first.
    pub fn create_stream(&self, torrent_id: usize, file_id: usize) -> anyhow::Result<impl AsyncRead + AsyncSeek + Unpin + Send> {
        let stream = self.api.api_stream(TorrentIdOrHash::Id(torrent_id), file_id)?;
        Ok(stream)
    }

    // Kept to satisfy lib.rs calls but delegates to stream logic (which lib.rs currently does not use... wait, lib.rs call get_main_file_path!)
    // We need to keep get_main_file_path working or update lib.rs.
    // For now, let's update lib.rs to use create_stream instead, 
    // OR have get_main_file_path return a dummy path and let the http_server do the streaming.
    // BUT http_server accepts a PATH from the map.
    // So we need to change http_server architecture to accept a STREAM provider?
    // OR we change get_main_file_path to return a "p2p://id/fid" dummy path?
    // And http_server parses it?
    
    // For now, let's stub get_main_file_path to return the needed Metadata for lib.rs
    // lib.rs: registers (id -> path).
    // http_server: checks map.
    
    // New Plan: Store "virtual path" in map: `p2p_stream://{torrent_id}/{file_id}`.
    // http_server detects this prefix and calls `manager.create_stream`.
    
    pub fn get_main_file_path(&self, id: usize) -> Option<PathBuf> {
         let fid = self.get_largest_file_id(id)?;
         // Return a virtual path that http_server can parse.
         // e.g. "stream:<tid>:<fid>"
         // But PathBuf expects valid path.
         // We can return a fake path "C:\Stream\{id}\{fid}".
         let s = format!("C:\\Stream\\{l}\\{r}", l=id, r=fid);
         Some(PathBuf::from(s))
    }

    pub fn get_torrents(&self) -> Vec<TorrentStatus> {
        // Step 1: Collect IDs safely to avoid deadlock
        let ids: Vec<usize> = self.session.with_torrents(|torrents| {
             torrents.map(|(id, _)| id).collect()
        });
        
        // Step 2: Fetch details
        let mut result = Vec::new();
        
        for id in ids {
            if let Ok(details) = self.api.api_torrent_details(TorrentIdOrHash::Id(id)) {
                let name = details.name.unwrap_or_else(|| "Unknown".to_string());
                
                let (progress, state, down, up, peers) = if let Some(stats) = details.stats.as_ref() {
                     let total_bytes: u64 = details.files.as_ref()
                         .map(|files| files.iter().map(|f| f.length).sum())
                         .unwrap_or(0);
                     let done = stats.progress_bytes;
                     let p = if total_bytes > 0 { (done as f64 / total_bytes as f64) * 100.0 } else { 0.0 };
                     
                     let state_str = format!("{:?}", stats.state);
                     
                     (p, state_str, 0, 0, 0)
                } else {
                     (0.0, "Unknown".to_string(), 0, 0, 0)
                };
                
                result.push(TorrentStatus {
                    id: id,
                    name,
                    progress_percent: progress,
                    speed_download: down,
                    speed_upload: up,
                    peers: peers,
                    state,
                });
            }
        }
        result
    }
}
