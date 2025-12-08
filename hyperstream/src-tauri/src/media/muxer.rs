use std::process::Command;
use std::path::Path;

/// Merge video and audio files into egg single container using FFmpeg
pub fn merge_streams(video_path: &Path, audio_path: &Path, output_path: &Path) -> Result<(), String> {
    // Check if input files exist
    if !video_path.exists() {
        return Err(format!("Video file not found: {:?}", video_path));
    }
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {:?}", audio_path));
    }

    // Command: ffmpeg -i video -i audio -c copy -y output
    // -c copy: Copy streams without re-encoding (fast)
    // -y: Overwrite output file if exists
    let output = Command::new("ffmpeg")
        .arg("-i").arg(video_path)
        .arg("-i").arg(audio_path)
        .arg("-c").arg("copy")
        .arg("-y")
        .arg(output_path)
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}. Is FFmpeg installed and in PATH?", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg failed: {}", stderr));
    }

    Ok(())
}

/// Check if FFmpeg is installed and accessible in PATH
pub fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
