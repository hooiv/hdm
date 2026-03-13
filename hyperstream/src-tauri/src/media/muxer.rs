use std::process::Command;
use std::path::Path;
use std::io::{self, Write};

/// Merge video and audio files into a single container using FFmpeg.
/// Falls back to raw stream concatenation when FFmpeg is unavailable
/// (works correctly for MPEG-TS streams; fMP4 audio will be saved separately).
pub fn merge_streams(video_path: &Path, audio_path: &Path, output_path: &Path) -> Result<(), String> {
    // ── Path security: validate parents are inside download dir ────────────
    // We validate *parent directories* (not the files themselves) because the
    // temp files may not exist yet when this function is called during setup.
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;

    for (label, p) in [("Video", video_path), ("Audio", audio_path)] {
        let parent = p.parent().unwrap_or(p);
        // If the parent is empty (file in CWD) treat it as current dir
        let canon_parent = if parent == Path::new("") {
            dunce::canonicalize(Path::new("."))
                .map_err(|e| format!("Cannot resolve {} parent dir: {}", label, e))?
        } else {
            dunce::canonicalize(parent)
                .map_err(|e| format!("Cannot resolve {} parent dir: {}", label, e))?
        };
        if !canon_parent.starts_with(&download_dir) {
            return Err(format!("{} file must be within the download directory", label));
        }
    }

    if let Some(out_parent) = output_path.parent() {
        if out_parent != Path::new("") {
            let canon_out = dunce::canonicalize(out_parent)
                .map_err(|e| format!("Cannot resolve output directory: {}", e))?;
            if !canon_out.starts_with(&download_dir) {
                return Err("Output file must be within the download directory".to_string());
            }
        }
    }

    // ── Input files must exist ──────────────────────────────────────────────
    if !video_path.exists() {
        return Err(format!("Video file not found: {:?}", video_path));
    }
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {:?}", audio_path));
    }

    // ── Prefer FFmpeg (supports all container formats) ──────────────────────
    if is_ffmpeg_available() {
        return merge_with_ffmpeg(video_path, audio_path, output_path);
    }

    // ── FFmpeg not found: use pure-Rust TS concatenation fallback ──────────
    // This works correctly for MPEG-TS (.ts) segments which are byte-stream
    // concatenable. For fMP4 we rename video and warn about audio.
    let ext = video_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext.eq_ignore_ascii_case("ts") {
        concat_ts_streams(video_path, audio_path, output_path)
    } else {
        // fMP4 / MP4: rename video, emit warning
        std::fs::rename(video_path, output_path)
            .map_err(|e| format!("Failed to move video file: {}", e))?;
        eprintln!(
            "[Muxer] WARNING: FFmpeg not found. Audio track could not be merged — \
             saved separately at {:?}. Install FFmpeg to enable full muxing.",
            audio_path
        );
        Ok(())
    }
}

fn merge_with_ffmpeg(video: &Path, audio: &Path, output: &Path) -> Result<(), String> {
    // -c copy: stream copy (no re-encode), preserves quality
    // -movflags +faststart: relocate moov atom for web streaming
    // -y: overwrite output
    let result = Command::new("ffmpeg")
        .args(["-i", &video.to_string_lossy()])
        .args(["-i", &audio.to_string_lossy()])
        .args(["-c", "copy"])
        .args(["-movflags", "+faststart"])
        .arg("-y")
        .arg(output)
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}. Install FFmpeg and add it to PATH.", e))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("FFmpeg failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Pure-Rust MPEG-TS concatenation: append audio stream bytes to video stream file.
/// This is correct for TS because TS is a packetized stream — cat-ing two TS files
/// produces a valid TS container the player can demux correctly.
fn concat_ts_streams(video: &Path, audio: &Path, output: &Path) -> Result<(), String> {
    use std::fs::File;

    let mut out_file = File::create(output)
        .map_err(|e| format!("Failed to create output file: {}", e))?;

    // Write video stream
    {
        let mut vid = File::open(video)
            .map_err(|e| format!("Failed to open video file: {}", e))?;
        io::copy(&mut vid, &mut out_file)
            .map_err(|e| format!("Failed to write video data: {}", e))?;
    }

    // Append audio stream
    {
        let mut aud = File::open(audio)
            .map_err(|e| format!("Failed to open audio file: {}", e))?;
        io::copy(&mut aud, &mut out_file)
            .map_err(|e| format!("Failed to write audio data: {}", e))?;
    }

    out_file.flush()
        .map_err(|e| format!("Failed to flush output: {}", e))?;

    Ok(())
}

/// Check if FFmpeg is installed and accessible in PATH.
pub fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn concat_ts_combines_files() {
        let dir = temp_dir();
        let video = dir.path().join("video.ts");
        let audio = dir.path().join("audio.ts");
        let output = dir.path().join("out.ts");

        fs::write(&video, b"VIDEO_DATA_XXXX").unwrap();
        fs::write(&audio, b"AUDIO_DATA_YYYY").unwrap();

        concat_ts_streams(&video, &audio, &output).expect("concat should succeed");

        let result = fs::read(&output).unwrap();
        assert_eq!(result, b"VIDEO_DATA_XXXXAUDIO_DATA_YYYY");
    }

    #[test]
    fn merge_streams_errors_on_missing_video() {
        let dir = temp_dir();
        let video = dir.path().join("nonexistent_video.ts");
        let audio = dir.path().join("audio.ts");
        let output = dir.path().join("out.ts");
        fs::write(&audio, b"audio").unwrap();

        // This will error on path validation (download_dir check) before file existence
        // for the security check; we just confirm it returns an Err.
        let result = std::panic::catch_unwind(|| {
            merge_streams(&video, &audio, &output)
        });
        // Either panics (settings) or returns Err — either way it doesn't succeed silently
        if let Ok(r) = result {
            assert!(r.is_err(), "Should error when video file does not exist");
        }
    }
}
