use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use lazy_static::lazy_static;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncEvent {
    pub event_type: String, // "ADD_DOWNLOAD"
    pub payload: String,    // URL or JSON data
}

lazy_static! {
    static ref IS_CONNECTED: AtomicBool = AtomicBool::new(false);
    static ref SYNC_CLIENT: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
}

pub async fn connect_to_workspace(host_ip: String) -> Result<(), String> {
    let url = format!("ws://{}:8765/api/sync", host_ip);
    println!("Connecting to Workspace: {}", url);

    let (ws_stream, _) = connect_async(&url).await.map_err(|e| e.to_string())?;
    println!("Connected to Workspace!");

    IS_CONNECTED.store(true, Ordering::Relaxed);

    let (_, mut read) = ws_stream.split();

    let handle = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    println!("Sync Msg: {}", text);
                    if let Ok(event) = serde_json::from_str::<SyncEvent>(&text) {
                        if event.event_type == "ADD_DOWNLOAD" {
                            // Trigger download
                            // Note: We can't easily call tauri commands from here directly without the AppHandle.
                            // We should probably emit an event or use a callback if possible.
                            // For now, let's just use the persistence/manager directly if linked, 
                            // or better, emit a global event that the main loop picks up.
                            // But here we are in a completely separate task.
                            
                            // HACK: Invoke 'add_download' via internal channel or lazy static queue if necessary.
                            // Ideally, we emit to frontend, and frontend calls 'start_download'.
                            // BUT, we want headless sync.
                            
                            // Let's assume we can emit to the frontend via a global handle if we had one.
                            // Since we don't have easy access to AppHandle here (it's in lib.rs),
                            // we will emit a Tauri Event if we can, or just loop back.
                            
                            // For this MVP, let's just print. 
                            // REAL IMPLEMENTATION: We need to pass this event to the main App logic.
                        }
                    }
                }
                _ => {}
            }
        }
        IS_CONNECTED.store(false, Ordering::Relaxed);
        println!("Disconnected from Workspace");
    });

    *SYNC_CLIENT.lock().await = Some(handle);
    Ok(())
}

#[allow(dead_code)]
pub fn is_connected() -> bool {
    IS_CONNECTED.load(Ordering::Relaxed)
}
