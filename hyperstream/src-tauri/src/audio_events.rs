use std::io::Cursor;
use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::sync::Mutex;

// Embed sound files in the binary
const SUCCESS_SOUND: &[u8] = include_bytes!("../assets/sounds/success.wav");
const ERROR_SOUND: &[u8] = include_bytes!("../assets/sounds/error.wav");
const START_SOUND: &[u8] = include_bytes!("../assets/sounds/start.wav");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    DownloadStart,
    DownloadComplete,
    DownloadError,
}

pub struct AudioPlayer {
    enabled: Arc<Mutex<bool>>,
    volume: Arc<Mutex<f32>>,
    custom_sounds: Arc<Mutex<HashMap<SoundEvent, PathBuf>>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(Mutex::new(true)),
            volume: Arc::new(Mutex::new(0.5)), // 50% default volume
            custom_sounds: Arc::new(Mutex::new(HashMap::new())),
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

    /// Set a custom sound file for a specific event
    pub async fn set_custom_sound(&self, event: SoundEvent, path: PathBuf) {
        self.custom_sounds.lock().await.insert(event, path);
    }

    /// Clear custom sound for a specific event (reverts to embedded)
    pub async fn clear_custom_sound(&self, event: SoundEvent) {
        self.custom_sounds.lock().await.remove(&event);
    }

    /// Get all custom sound paths
    pub async fn get_custom_sounds(&self) -> HashMap<String, String> {
        let sounds = self.custom_sounds.lock().await;
        let mut result = HashMap::new();
        for (event, path) in sounds.iter() {
            let key = match event {
                SoundEvent::DownloadStart => "start",
                SoundEvent::DownloadComplete => "complete",
                SoundEvent::DownloadError => "error",
            };
            result.insert(key.to_string(), path.to_string_lossy().to_string());
        }
        result
    }

    /// Load custom sounds from settings
    pub async fn load_custom_sounds_from_settings(&self, settings: &crate::settings::Settings) {
        let mut sounds = self.custom_sounds.lock().await;
        sounds.clear();
        
        if let Some(ref path) = settings.custom_sound_start {
            if !path.is_empty() && std::path::Path::new(path).exists() {
                sounds.insert(SoundEvent::DownloadStart, PathBuf::from(path));
            }
        }
        if let Some(ref path) = settings.custom_sound_complete {
            if !path.is_empty() && std::path::Path::new(path).exists() {
                sounds.insert(SoundEvent::DownloadComplete, PathBuf::from(path));
            }
        }
        if let Some(ref path) = settings.custom_sound_error {
            if !path.is_empty() && std::path::Path::new(path).exists() {
                sounds.insert(SoundEvent::DownloadError, PathBuf::from(path));
            }
        }
    }

    /// Play a sound event asynchronously (non-blocking)
    pub async fn play(&self, event: SoundEvent) {
        if !self.is_enabled().await {
            return;
        }

        let volume = self.get_volume().await;
        let custom_path = self.custom_sounds.lock().await.get(&event).cloned();
        
        // Spawn a new thread to avoid blocking
        std::thread::spawn(move || {
            if let Err(e) = play_sound_blocking(event, volume, custom_path) {
                eprintln!("Failed to play sound: {}", e);
            }
        });
    }
}

/// Blocking sound playback (called in separate thread)
/// Compatible with rodio 0.17-0.21
fn play_sound_blocking(event: SoundEvent, volume: f32, custom_path: Option<PathBuf>) -> Result<(), String> {
    // Try to play sound - use Result for better error handling
    match rodio::OutputStream::try_default() {
        Ok((_stream, stream_handle)) => {
            match rodio::Sink::try_new(&stream_handle) {
                Ok(sink) => {
                    sink.set_volume(volume);
                    
                    // Try custom file first, fall back to embedded
                    if let Some(ref path) = custom_path {
                        if path.exists() {
                            match std::fs::File::open(path) {
                                Ok(file) => {
                                    let reader = std::io::BufReader::new(file);
                                    match rodio::Decoder::new(reader) {
                                        Ok(source) => {
                                            sink.append(source);
                                            sink.sleep_until_end();
                                            return Ok(());
                                        }
                                        Err(e) => {
                                            eprintln!("Custom sound decode failed, using embedded: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Custom sound file open failed, using embedded: {}", e);
                                }
                            }
                        }
                    }
                    
                    // Fall back to embedded sounds
                    let sound_data = match event {
                        SoundEvent::DownloadStart => START_SOUND,
                        SoundEvent::DownloadComplete => SUCCESS_SOUND,
                        SoundEvent::DownloadError => ERROR_SOUND,
                    };
                    
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

    #[tokio::test]
    async fn test_custom_sounds() {
        let player = AudioPlayer::new();
        
        // Initially empty
        let sounds = player.get_custom_sounds().await;
        assert!(sounds.is_empty());
        
        // Set a custom sound
        player.set_custom_sound(SoundEvent::DownloadComplete, PathBuf::from("test.wav")).await;
        let sounds = player.get_custom_sounds().await;
        assert_eq!(sounds.get("complete"), Some(&"test.wav".to_string()));
        
        // Clear it
        player.clear_custom_sound(SoundEvent::DownloadComplete).await;
        let sounds = player.get_custom_sounds().await;
        assert!(sounds.is_empty());
    }
}
