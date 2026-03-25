//! Tauri commands for segment integrity verification and monitoring

use crate::segment_integrity::*;
use crate::core_state::AppState;
use crate::downloader::structures::Segment;
use crate::persistence::SavedDownload;
use tauri::State;
use serde_json::json;

/// Verify integrity of all segments in a download
#[tauri::command]
pub async fn verify_download_integrity(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<IntegrityReport, String> {
    // Get download from state
    let downloads = state.downloads.lock().map_err(|e| format!("Lock error: {}", e))?;
    let download = downloads
        .get(&download_id)
        .cloned()
        .ok_or_else(|| format!("Download not found: {}", download_id))?;
    drop(downloads);

    let verifier = SegmentIntegrityVerifier::new();

    // Convert segment structs
    let segments = if let Some(ref segs) = download.segments {
        segs.clone()
    } else {
        return Err("Download has no segments".to_string());
    };

    verifier
        .verify_download(
            &download_id,
            &download.path,
            &segments,
            ChecksumAlgorithm::SHA256,
        )
        .await
}

/// Verify specific segments by index
#[tauri::command]
pub async fn verify_segments(
    state: State<'_, AppState>,
    download_id: String,
    segment_indices: Vec<usize>,
) -> Result<Vec<SegmentIntegrityInfo>, String> {
    let downloads = state.downloads.lock().map_err(|e| format!("Lock error: {}", e))?;
    let download = downloads
        .get(&download_id)
        .cloned()
        .ok_or_else(|| format!("Download not found: {}", download_id))?;
    drop(downloads);

    let verifier = SegmentIntegrityVerifier::new();

    let segments = if let Some(ref segs) = download.segments {
        segs.clone()
    } else {
        return Err("Download has no segments".to_string());
    };

    // Filter to requested segments
    let filtered: Vec<_> = segments
        .iter()
        .enumerate()
        .filter(|(i, _)| segment_indices.contains(i))
        .map(|(_, seg)| seg.clone())
        .collect();

    if filtered.is_empty() {
        return Err("No valid segments to verify".to_string());
    }

    // Verify each segment
    let mut results = Vec::new();
    for (idx, segment) in filtered.iter().enumerate() {
        let segment_id = segment_indices[idx];
        let info = SegmentIntegrityVerifier::verify_segment(
            std::path::Path::new(&download.path),
            segment,
            segment_id,
            ChecksumAlgorithm::SHA256,
        )
        .await?;

        results.push(info);
    }

    Ok(results)
}

/// Get cached integrity report
#[tauri::command]
pub fn get_cached_integrity_report(download_id: String) -> Result<Option<IntegrityReport>, String> {
    Ok(get_integrity_report(&download_id))
}

/// Get global integrity metrics
#[tauri::command]
pub fn get_integrity_monitoring_metrics() -> Result<IntegrityMetrics, String> {
    get_integrity_metrics()
}

/// Generate recovery strategies for a failed download
#[tauri::command]
pub fn generate_recovery_strategies(
    download_id: String,
) -> Result<Vec<RecoveryStrategy>, String> {
    let report = get_integrity_report(&download_id)
        .ok_or_else(|| format!("No integrity report found for {}", download_id))?;

    let verifier = SegmentIntegrityVerifier::new();
    let strategies = verifier.generate_recovery_strategies(&report);

    Ok(strategies)
}

/// Batch verify multiple downloads
#[tauri::command]
pub async fn batch_verify_downloads(
    state: State<'_, AppState>,
    download_ids: Vec<String>,
) -> Result<Vec<(String, IntegrityReport)>, String> {
    let verifier = SegmentIntegrityVerifier::new();
    let mut results = Vec::new();

    for id in download_ids {
        let downloads = state.downloads.lock().map_err(|e| format!("Lock error: {}", e))?;
        if let Some(download) = downloads.get(&id).cloned() {
            drop(downloads);

            if let Some(ref segments) = download.segments {
                match verifier
                    .verify_download(&id, &download.path, segments, ChecksumAlgorithm::SHA256)
                    .await
                {
                    Ok(report) => results.push((id.clone(), report)),
                    Err(e) => log::warn!("Failed to verify {}: {}", id, e),
                }
            }
        }
    }

    Ok(results)
}

/// Get integrity summary for UI dashboard
#[tauri::command]
pub fn get_integrity_summary(
    download_id: String,
) -> Result<serde_json::Value, String> {
    let report = get_integrity_report(&download_id)
        .ok_or_else(|| format!("No report found for {}", download_id))?;

    let metrics = get_integrity_metrics().unwrap_or_else(|_| IntegrityMetrics {
        total_segments_verified: 0,
        total_corruptions_detected: 0,
        auto_recovery_attempts: 0,
        auto_recovery_success: 0,
        average_verification_time_ms: 0.0,
        average_integrity_score: 100.0,
    });

    Ok(json!({
        "download_id": report.download_id,
        "overall_score": report.overall_score,
        "risk_level": report.risk_level,
        "at_risk_percentage": report.at_risk_percentage,
        "failed_segments_count": report.failed_segments.len(),
        "total_segments": report.segments.len(),
        "is_healthy": report.is_healthy(),
        "can_resume": report.can_resume(),
        "should_restart": report.should_restart(),
        "verification_time_ms": report.total_duration_ms,
        "global_metrics": {
            "segments_verified": metrics.total_segments_verified,
            "corruptions_detected": metrics.total_corruptions_detected,
            "average_score": metrics.average_integrity_score,
        },
        "recommendations": report.recommendations,
    }))
}

/// Export integrity report as JSON
#[tauri::command]
pub fn export_integrity_report(download_id: String, export_path: String) -> Result<String, String> {
    let report = get_integrity_report(&download_id)
        .ok_or_else(|| format!("No report found for {}", download_id))?;

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("JSON serialization failed: {}", e))?;

    std::fs::write(&export_path, json)
        .map_err(|e| format!("Cannot write report: {}", e))?;

    Ok(format!("Report exported to {}", export_path))
}
