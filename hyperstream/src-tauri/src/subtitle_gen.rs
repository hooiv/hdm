use std::path::Path;
use std::process::Command;

/// Generate SRT subtitles from a video file.
/// Uses ffmpeg to extract audio, then sends it to a local or cloud Whisper API for transcription.
/// Falls back to generating a stub SRT if Whisper is not available.
pub async fn generate_subtitles(video_path: String) -> Result<serde_json::Value, String> {
    let path = Path::new(&video_path);
    if !path.exists() {
        return Err(format!("Video not found: {}", video_path));
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !["mp4", "mkv", "avi", "mov", "webm", "flv", "wmv"].contains(&ext.as_str()) {
        return Err("Only video files are supported (.mp4, .mkv, .avi, .mov, .webm).".to_string());
    }

    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let parent = path.parent().unwrap_or(Path::new("."));
    let audio_path = parent.join(format!("{}_audio.wav", stem));
    let srt_path = parent.join(format!("{}.srt", stem));

    // Step 1: Extract audio using ffmpeg
    let ffmpeg_result = Command::new("ffmpeg")
        .args([
            "-i", &video_path,
            "-vn",                // No video
            "-acodec", "pcm_s16le", // WAV format
            "-ar", "16000",       // 16kHz for Whisper
            "-ac", "1",           // Mono
            "-y",                 // Overwrite
            &audio_path.to_string_lossy(),
        ])
        .output();

    let has_audio = match ffmpeg_result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };

    if !has_audio {
        // No ffmpeg or extraction failed — try Whisper directly on video
        // Or generate a placeholder SRT
        return generate_placeholder_srt(&srt_path, &video_path).await;
    }

    // Step 2: Try local Whisper CLI first
    let whisper_result = Command::new("whisper")
        .args([
            &audio_path.to_string_lossy(),
            "--model", "base",
            "--output_format", "srt",
            "--output_dir", &parent.to_string_lossy(),
        ])
        .output();

    if let Ok(output) = whisper_result {
        if output.status.success() {
            // Clean up audio
            let _ = std::fs::remove_file(&audio_path);

            let srt_content = std::fs::read_to_string(&srt_path)
                .unwrap_or_else(|_| "Subtitles generated.".to_string());
            let line_count = srt_content.lines().count();

            return Ok(serde_json::json!({
                "status": "generated",
                "method": "whisper_local",
                "srt_path": srt_path.to_string_lossy(),
                "subtitle_lines": line_count / 4, // Approximate cue count
                "model": "base",
            }));
        }
    }

    // Step 3: Try Whisper API (OpenAI) if available
    let api_key = std::env::var("OPENAI_API_KEY").ok();
    if let Some(key) = api_key {
        let audio_bytes = tokio::fs::read(&audio_path).await
            .map_err(|e| format!("Read audio error: {}", e))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Client error: {}", e))?;

        let form = reqwest::multipart::Form::new()
            .text("model", "whisper-1")
            .text("response_format", "srt")
            .part("file", reqwest::multipart::Part::bytes(audio_bytes)
                .file_name(format!("{}_audio.wav", stem))
                .mime_str("audio/wav")
                .map_err(|e| e.to_string())?);

        let response = client.post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Whisper API failed: {}", e))?;

        if response.status().is_success() {
            let srt_content = response.text().await.map_err(|e| e.to_string())?;
            tokio::fs::write(&srt_path, &srt_content).await
                .map_err(|e| format!("Write SRT error: {}", e))?;

            // Clean up
            let _ = std::fs::remove_file(&audio_path);

            let line_count = srt_content.lines().count();
            return Ok(serde_json::json!({
                "status": "generated",
                "method": "whisper_api",
                "srt_path": srt_path.to_string_lossy(),
                "subtitle_lines": line_count / 4,
                "model": "whisper-1",
            }));
        }
    }

    // Clean up audio
    let _ = std::fs::remove_file(&audio_path);

    // Fallback: generate placeholder
    generate_placeholder_srt(&srt_path, &video_path).await
}

async fn generate_placeholder_srt(srt_path: &Path, video_path: &str) -> Result<serde_json::Value, String> {
    // Get video duration via ffprobe if available
    let duration = get_video_duration(video_path).unwrap_or(60.0);

    let mut srt = String::new();
    let interval = 10.0f64; // 10-second segments
    let segments = (duration / interval).ceil() as usize;

    for i in 0..segments.min(100) {
        let start = i as f64 * interval;
        let end = ((i + 1) as f64 * interval).min(duration);
        srt.push_str(&format!(
            "{}\n{} --> {}\n[Subtitle segment {} - Install Whisper for auto-transcription]\n\n",
            i + 1,
            format_srt_time(start),
            format_srt_time(end),
            i + 1,
        ));
    }

    tokio::fs::write(srt_path, &srt).await
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(serde_json::json!({
        "status": "placeholder",
        "method": "stub",
        "srt_path": srt_path.to_string_lossy(),
        "subtitle_lines": segments,
        "note": "Install Whisper CLI or set OPENAI_API_KEY for real transcription",
    }))
}

fn get_video_duration(path: &str) -> Option<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output().ok()?;

    String::from_utf8_lossy(&output.stdout).trim().parse::<f64>().ok()
}

fn format_srt_time(seconds: f64) -> String {
    let h = (seconds / 3600.0) as u32;
    let m = ((seconds % 3600.0) / 60.0) as u32;
    let s = (seconds % 60.0) as u32;
    let ms = ((seconds % 1.0) * 1000.0) as u32;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}
