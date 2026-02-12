use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::Mutex;

// Embed sound files in the binary
const SUCCESS_SOUND: &[u8] = include_bytes!("../assets/sounds/success.wav");
const ERROR_SOUND: &[u8] = include_bytes!("../assets/sounds/error.wav");
const START_SOUND: &[u8] = include_bytes!("../assets/sounds/start.wav");

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundEvent {
    DownloadStart,
    DownloadComplete,
    DownloadError,
}

pub struct AudioPlayer {
    enabled: Arc<Mutex<bool>>,
    volume: Arc<Mutex<f32>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(Mutex::new(true)),
            volume: Arc::new(Mutex::new(0.5)), // 50% default volume
        }
    }

    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.lock().await = enabled;
    }

    pub async fn set_volume(&self, volume: f32) {
        let clamped_volume = volume.clamp(0.0, 1.0);
        *self.volume.lock().await = clamped_volume;
    }

    pub async fn is_enabled(&self) -> bool {
        *self.enabled.lock().await
    }

    pub async fn get_volume(&self) -> f32 {
        *self.volume.lock().await
    }

    /// Play a sound event asynchronously (non-blocking)
    pub async fn play(&self, event: SoundEvent) {
        if !self.is_enabled().await {
            return;
        }

        let volume = self.get_volume().await;
        
        // Spawn a new thread to avoid blocking
        std::thread::spawn(move || {
            if let Err(e) = play_sound_blocking(event, volume) {
                eprintln!("Failed to play sound: {}", e);
            }
        });
    }
}

/// Blocking sound playback (called in separate thread)
/// Compatible with rodio 0.17-0.21
fn play_sound_blocking(event: SoundEvent, volume: f32) -> Result<(), String> {
    // Get the sound data based on event type
    let sound_data = match event {
        SoundEvent::DownloadStart => START_SOUND,
        SoundEvent::DownloadComplete => SUCCESS_SOUND,
        SoundEvent::DownloadError => ERROR_SOUND,
    };

    // Try to play sound - use Result for better error handling
    match rodio::OutputStream::try_default() {
        Ok((_stream, stream_handle)) => {
            match rodio::Sink::try_new(&stream_handle) {
                Ok(sink) => {
                    sink.set_volume(volume);
                    
                    let cursor = Cursor::new(sound_data);
                    match rodio::Decoder::new(cursor) {
                        Ok(source) => {
                            sink.append(source);
                            sink.sleep_until_end();
                            Ok(())
                        }
                        Err(e) => Err(format!("Failed to decode audio: {}", e))
                    }
                }
                Err(e) => Err(format!("Failed to create audio sink: {}", e))
            }
        }
        Err(e) => Err(format!("Failed to create audio output stream: {}", e))
    }
}

// Global audio player instance
lazy_static::lazy_static! {
    pub static ref AUDIO_PLAYER: AudioPlayer = AudioPlayer::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audio_player_creation() {
        let player = AudioPlayer::new();
        assert!(player.is_enabled().await);
        assert_eq!(player.get_volume().await, 0.5);
    }

    #[tokio::test]
    async fn test_volume_clamping() {
        let player = AudioPlayer::new();
        
        player.set_volume(1.5).await;
        assert_eq!(player.get_volume().await, 1.0);
        
        player.set_volume(-0.5).await;
        assert_eq!(player.get_volume().await, 0.0);
    }

    #[tokio::test]
    async fn test_enable_disable() {
        let player = AudioPlayer::new();
        
        player.set_enabled(false).await;
        assert!(!player.is_enabled().await);
        
        player.set_enabled(true).await;
        assert!(player.is_enabled().await);
    }
}
