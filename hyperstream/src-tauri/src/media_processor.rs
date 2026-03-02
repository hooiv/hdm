use std::process::Command;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

pub struct MediaProcessor;

impl MediaProcessor {
    pub fn check_ffmpeg() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn generate_preview(input_path: &str, output_path: &str) -> Result<String, String> {
        // Generate a 3-second GIF/WebP from 20% mark
        // ffmpeg -ss <duration*0.2> -t 3 -i input -vf "fps=10,scale=320:-1:flags=lanczos" output.webp
        
        let path = Path::new(input_path);
        if !path.exists() {
            return Err("Input file not found".to_string());
        }

        // Get duration first (stubbed for MVP: assume 20% mark is safe or just use specific time like 00:00:10 if possible, 
        // but better to probe. For MVP, let's use fixed offset 10s or 10% logic if simple).
        // Let's rely on user "Smart Preview" logic: Try -ss 00:00:10.
        
        let output = Command::new("ffmpeg")
            .args(&[
                "-y", // Overwrite
                "-ss", "10", // Start at 10s
                "-t", "3", // 3 seconds
                "-i", input_path,
                "-vf", "fps=10,scale=320:-1:flags=lanczos",
                output_path
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(output_path.to_string())
        } else {
             let err = String::from_utf8_lossy(&output.stderr);
             Err(format!("FFmpeg failed: {}", err))
        }
    }

    pub fn extract_audio(input_path: &str, output_path: &str) -> Result<String, String> {
        let output = Command::new("ffmpeg")
            .args(&[
                "-y",
                "-i", input_path,
                "-vn", // No video
                "-acodec", "libmp3lame",
                "-q:a", "2",
                output_path
            ])
            .output()
            .map_err(|e| e.to_string())?;

         if output.status.success() {
            Ok(output_path.to_string())
        } else {
             let err = String::from_utf8_lossy(&output.stderr);
             Err(format!("FFmpeg failed: {}", err))
        }
    }
}
