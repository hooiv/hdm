/// Tauri IPC commands for download history analytics
///
/// Exposes the analytics engine to the frontend through Tauri commands

use crate::download_history_analytics::{
    get_history_analytics, DownloadHistoryStat, DownloadAnalyticsSnapshot,
};

#[tauri::command]
pub async fn record_download_stat(
    url: String,
    filename: String,
    file_size_bytes: u64,
    success: bool,
    duration_seconds: u64,
    average_speed_mbps: f64,
    mirror_used: String,
    failure_reason: Option<String>,
    retries_needed: u32,
) -> Result<String, String> {
    let analytics = get_history_analytics();

    let stat = DownloadHistoryStat {
        url,
        filename,
        file_size_bytes,
        success,
        duration_seconds,
        average_speed_mbps,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        mirror_used,
        failure_reason,
        retries_needed,
    };

    analytics.record_download(stat)?;
    Ok("Download recorded".to_string())
}

#[tauri::command]
pub async fn get_download_analytics() -> Result<DownloadAnalyticsSnapshot, String> {
    let analytics = get_history_analytics();
    analytics.get_analytics()
}

#[tauri::command]
pub async fn get_analytics_summary() -> Result<String, String> {
    let analytics = get_history_analytics();
    let snapshot = analytics.get_analytics()?;

    let summary = format!(
        "Download Analytics Summary\n\
         =========================\n\
         Total Downloads: {}\n\
         Successful: {} ({:.1}%)\n\
         \n\
         Performance:\n\
         - Average Speed: {:.2} Mbps\n\
         - Average Duration: {:.0} seconds\n\
         - Total Data: {} bytes\n\
         \n\
         Best Time: Hour {}\n\
         Best Mirror: {}\n\
         Worst Mirror: {}\n\
         \n\
         Recommendations: {}\n\
         Failure Patterns: {}",
        snapshot.total_downloads,
        snapshot.successful_downloads,
        snapshot.overall_success_rate * 100.0,
        snapshot.avg_speed_mbps,
        snapshot.avg_duration_seconds,
        snapshot.total_bytes_downloaded,
        snapshot.best_time_to_download,
        snapshot.best_mirror.as_deref().unwrap_or("None"),
        snapshot.worst_mirror.as_deref().unwrap_or("None"),
        snapshot.recommendations.len(),
        snapshot.failure_patterns.len(),
    );

    Ok(summary)
}

#[tauri::command]
pub async fn get_file_type_insights() -> Result<Vec<String>, String> {
    let analytics = get_history_analytics();
    let snapshot = analytics.get_analytics()?;

    let insights = snapshot
        .file_type_insights
        .iter()
        .map(|ft| {
            format!(
                ".{}: {} downloads, {:.1}% success, {:.2} Mbps avg",
                ft.file_type, ft.total_downloads, ft.success_rate * 100.0, ft.avg_speed_mbps
            )
        })
        .collect();

    Ok(insights)
}

#[tauri::command]
pub async fn get_mirror_performance() -> Result<Vec<String>, String> {
    let analytics = get_history_analytics();
    let snapshot = analytics.get_analytics()?;

    let performance = snapshot
        .mirror_analytics
        .iter()
        .map(|m| {
            format!(
                "{}: {:.1}% success, {:.2} Mbps, {} failures",
                m.mirror_host, m.success_rate * 100.0, m.avg_speed_mbps, m.failure_count
            )
        })
        .collect();

    Ok(performance)
}

#[tauri::command]
pub async fn get_recommendations() -> Result<Vec<String>, String> {
    let analytics = get_history_analytics();
    let snapshot = analytics.get_analytics()?;

    let recs = snapshot
        .recommendations
        .iter()
        .map(|r| {
            format!(
                "[{}] {} - {} (Confidence: {:.0}%)",
                r.category, r.title, r.action, r.confidence * 100.0
            )
        })
        .collect();

    Ok(recs)
}
