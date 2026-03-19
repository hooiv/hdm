use crate::audio_events::{AUDIO_PLAYER, SoundEvent};

pub fn play_complete() {
    tauri::async_runtime::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadComplete).await;
    });
}

pub fn play_error() {
    tauri::async_runtime::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadError).await;
    });
}

pub fn play_startup() {
    tauri::async_runtime::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadStart).await;
    });
}
