use crate::audio_events::{AUDIO_PLAYER, SoundEvent};

pub fn play_complete() {
    tokio::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadComplete).await;
    });
}

pub fn play_error() {
    tokio::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadError).await;
    });
}

pub fn play_startup() {
    tokio::spawn(async {
        AUDIO_PLAYER.play(SoundEvent::DownloadStart).await;
    });
}
