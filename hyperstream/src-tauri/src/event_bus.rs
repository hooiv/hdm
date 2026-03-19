use tokio::sync::broadcast;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SystemEvent {
    DownloadStarted(String),
    DownloadProgress(String, u64),
    DownloadCompleted(String),
    ModuleAction(String, String), // ModuleName, ActionDetails
    SystemError(String, String),
}

pub struct EventBus {
    pub sender: broadcast::Sender<SystemEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn broadcast(&self, event: SystemEvent) -> Result<usize, broadcast::error::SendError<SystemEvent>> {
        self.sender.send(event)
    }
}

use std::sync::atomic::{AtomicBool, Ordering};

static EVENT_BUS_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init_event_bus(app: &AppHandle) {
    // Prevent duplicate initialization
    if EVENT_BUS_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    let bus = EventBus::new(1024);
    
    // Spawn a background listener that routes events to the frontend
    let mut receiver = bus.sender.subscribe();
    let app_handle = app.clone();
    
    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let _ = app_handle.emit("system-bus-event", event);
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    eprintln!("[EventBus] WARNING: Dropped {} events due to slow consumer", count);
                    // Continue receiving — don't break out of the loop
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break; // Channel closed, stop listening
                }
            }
        }
    });

    app.manage(Arc::new(bus));
}
