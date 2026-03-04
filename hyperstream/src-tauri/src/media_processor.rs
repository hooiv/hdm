use std::process::Command;
use std::path::Path;
use serde::{Serialize, Deserialize};

/// Validate that a path is within the download directory.
fn validate_path_in_downloads(path_str: &str) -> Result<std::path::PathBuf, String> {
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon = dunce::canonicalize(path_str)
        .map_err(|e| format!("Cannot resolve path '{}': {}", path_str, e))?;
    if !canon.starts_with(&download_dir) {
        return Err(format!("Path must be within the download directory: {}", path_str));
    }
    Ok(canon)
}

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
        // Validate paths are within download directory
        let canon_input = validate_path_in_downloads(input_path)?;
        let _ = validate_path_in_downloads(output_path).or_else(|_| {
            // Output might not exist yet — validate its parent
            let parent = Path::new(output_path).parent().ok_or("No parent dir".to_string())?;
            let settings = crate::settings::load_settings();
            let download_dir = dunce::canonicalize(&settings.download_dir).map_err(|e| e.to_string())?;
            let canon_parent = dunce::canonicalize(parent).map_err(|e| e.to_string())?;
            if !canon_parent.starts_with(&download_dir) {
                return Err("Output path must be within download directory".to_string());
            }
            Ok(canon_parent)
        })?;

        if !canon_input.exists() {
            return Err("Input file not found".to_string());
        }

        // Get duration first (stubbed for MVP: assume 20% mark is safe or just use specific time like 00:00:10 if possible, 
        // but better to probe. For MVP, let's use fixed offset 10s or 10% logic if simple).
        // Let's rely on user "Smart Preview" logic: Try -ss 00:00:10.
        
        let canon_input_str = canon_input.to_string_lossy();

        // Construct safe output path: canonical parent + original filename
        let safe_output = {
            let out_path = Path::new(output_path);
            let parent = out_path.parent().ok_or("No parent dir for output".to_string())?;
            let filename = out_path.file_name().ok_or("No filename for output".to_string())?;
            let canon_parent = dunce::canonicalize(parent).map_err(|e| format!("Cannot resolve output dir: {}", e))?;
            canon_parent.join(filename)
        };
        let safe_output_str = safe_output.to_string_lossy();

        let output = Command::new("ffmpeg")
            .args(&[
                "-y", // Overwrite
                "-ss", "10", // Start at 10s
                "-t", "3", // 3 seconds
                "-i", &canon_input_str,
                "-vf", "fps=10,scale=320:-1:flags=lanczos",
                &safe_output_str
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(safe_output_str.to_string())
        } else {
             let err = String::from_utf8_lossy(&output.stderr);
             Err(format!("FFmpeg failed: {}", err))
        }
    }

    pub fn extract_audio(input_path: &str, output_path: &str) -> Result<String, String> {
        // Validate paths are within download directory
        let canon_input = validate_path_in_downloads(input_path)?;
        let _ = validate_path_in_downloads(output_path).or_else(|_| {
            let parent = Path::new(output_path).parent().ok_or("No parent dir".to_string())?;
            let settings = crate::settings::load_settings();
            let download_dir = dunce::canonicalize(&settings.download_dir).map_err(|e| e.to_string())?;
            let canon_parent = dunce::canonicalize(parent).map_err(|e| e.to_string())?;
            if !canon_parent.starts_with(&download_dir) {
                return Err("Output path must be within download directory".to_string());
            }
            Ok(canon_parent)
        })?;

        let canon_input_str = canon_input.to_string_lossy();

        // Construct safe output path: canonical parent + original filename
        let safe_output = {
            let out_path = Path::new(output_path);
            let parent = out_path.parent().ok_or("No parent dir for output".to_string())?;
            let filename = out_path.file_name().ok_or("No filename for output".to_string())?;
            let canon_parent = dunce::canonicalize(parent).map_err(|e| format!("Cannot resolve output dir: {}", e))?;
            canon_parent.join(filename)
        };
        let safe_output_str = safe_output.to_string_lossy();

        let output = Command::new("ffmpeg")
            .args(&[
                "-y",
                "-i", &canon_input_str,
                "-vn", // No video
                "-acodec", "libmp3lame",
                "-q:a", "2",
                &safe_output_str
            ])
            .output()
            .map_err(|e| e.to_string())?;

         if output.status.success() {
            Ok(safe_output_str.to_string())
        } else {
             let err = String::from_utf8_lossy(&output.stderr);
             Err(format!("FFmpeg failed: {}", err))
        }
    }
}
