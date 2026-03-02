use tokio::sync::broadcast;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

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

pub fn init_event_bus(app: &AppHandle) {
    let bus = EventBus::new(1024);
    
    // Spawn a background listener that routes events to the frontend
    let mut receiver = bus.sender.subscribe();
    let app_handle = app.clone();
    
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let _ = app_handle.emit("system-bus-event", event);
        }
    });

    app.manage(Arc::new(bus));
}
