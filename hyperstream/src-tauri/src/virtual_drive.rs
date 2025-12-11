use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use warp::Filter;
use dav_server::{DavHandler, localfs::LocalFs};

lazy_static::lazy_static! {
    pub static ref DRIVE_MANAGER: Arc<DriveManager> = Arc::new(DriveManager::new());
}

pub struct DriveManager {
    // Map ID -> (Port, ShutdownSignalSender)
    mounts: Mutex<HashMap<String, (u16, tokio::sync::oneshot::Sender<()>)>>,
}

impl DriveManager {
    pub fn new() -> Self {
        Self {
            mounts: Mutex::new(HashMap::new()),
        }
    }

    pub async fn mount(&self, id: String, path: String) -> Result<u16, String> {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        
        let path_buf = std::path::PathBuf::from(&path);
        let root = if path_buf.is_file() {
            // For now, if file, serve parent. True ZipFS needed for T1 completion.
            path_buf.parent().unwrap().to_path_buf()
        } else {
            path_buf
        };

        // Create DavHandler
        let dav_server = DavHandler::builder()
            .filesystem(LocalFs::new(root, false, false, false)) // Read-only
            .locksystem(dav_server::fakels::FakeLs::new())
            .build_handler();

        // Wrap in Warp
        let dav_filter = warp::any()
            .and(warp::method())
            .and(warp::header::headers_cloned())
            .and(warp::body::stream())
            .and(warp::path::full())
            .and_then(move |method, headers, body, path_idx: warp::path::FullPath| {
                 let dav_server = dav_server.clone();
                 async move {
                     let mut req = warp::http::Request::builder()
                        .method(method)
                        .uri(path_idx.as_str())
                        .body(body)
                        .unwrap();
                     *req.headers_mut() = headers;
                     let res = dav_server.handle(req).await;
                     let res = res.map(|b| warp::hyper::Body::wrap_stream(b));
                     Ok::<_, std::convert::Infallible>(res)
                 }
            });

        // Bind to random port
        let (addr, server) = warp::serve(dav_filter)
            .bind_with_graceful_shutdown(([127, 0, 0, 1], 0), async move {
                 let _ = rx.await;
            });

        let port = addr.port();
        
        // Spawn server
        tokio::spawn(server);

        // Store handle
        self.mounts.lock().map_err(|e| e.to_string())?.insert(id.clone(), (port, tx));
        
        Ok(port)
    }

    pub fn unmount(&self, id: String) -> Result<(), String> {
        let mut mounts = self.mounts.lock().map_err(|e| e.to_string())?;
        if let Some((_, tx)) = mounts.remove(&id) {
            let _ = tx.send(());
            Ok(())
        } else {
            Err("Mount not found".to_string())
        }
    }
}
