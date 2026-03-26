//! Integration of corruption recovery into the download engine
//!
//! This module provides hooks that integrate the recovery system into the
//! core download lifecycle. It automatically:
//! - Detects corruption during segment completion
//! - Suggests recovery strategies
//! - Emits UI events for user notification
//! - Records mirror reliability metrics

use crate::download_recovery::{
    CorruptionEvidence, CorruptionType, RecoveryStrategy,
};
use crate::core_state::AppState;
use tauri::Emitter;
use std::sync::Arc;
use tokio::sync::Mutex;
use sha2::Digest;

/// Event emitted when corruption is detected during a download
#[derive(serde::Serialize, Clone)]
pub struct CorruptionDetectedEvent {
    pub download_id: String,
    pub segment_id: usize,
    pub evidence: CorruptionEvidence,
    pub suggested_recovery: String,
}

/// Event emitted when recovery is triggered
#[derive(serde::Serialize, Clone)]
pub struct RecoveryTriggeredEvent {
    pub download_id: String,
    pub segment_id: usize,
    pub strategy: String,
    pub reason: String,
}

/// Hook into download segment completion to check for corruption
pub async fn on_segment_completion(
    app_handle: &tauri::AppHandle,
    state: &AppState,
    download_id: &str,
    segment_id: usize,
    segment_start: u64,
    segment_end: u64,
    data: &[u8],
    expected_checksum: Option<&str>,
    mirror_url: &str,
) {
    // Check for entropy-based corruption
    let entropy = calculate_entropy(data);
    if entropy < 1.5 {
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::LowEntropy {
                entropy,
                threshold: 1.5,
            },
            confidence: 75,
            detected_at_ms: current_time_ms(),
            evidence_data: format!(
                "Entropy {:.4} below threshold 1.5 (likely compressed or corrupted)",
                entropy
            ),
        };

        state
            .recovery_manager
            .report_corruption(download_id.to_string(), evidence.clone())
            .await;

        // Emit UI event
        let _ = app_handle.emit(
            "corruption_detected",
            CorruptionDetectedEvent {
                download_id: download_id.to_string(),
                segment_id,
                evidence,
                suggested_recovery: "Switch mirror or retry from offset".to_string(),
            },
        );
    }

    // Check checksum if provided
    if let Some(expected) = expected_checksum {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        let computed = format!("{:x}", hasher.finalize());
        if computed != expected {
            let evidence = CorruptionEvidence {
                segment_id,
                segment_start,
                segment_end,
                corruption_type: CorruptionType::ChecksumMismatch {
                    expected: expected.to_string(),
                    computed,
                    algorithm: "sha256".to_string(),
                },
                confidence: 100,
                detected_at_ms: current_time_ms(),
                evidence_data: "SHA256 checksum validation failed".to_string(),
            };

            state
                .recovery_manager
                .report_corruption(download_id.to_string(), evidence.clone())
                .await;

            let _ = app_handle.emit(
                "corruption_detected",
                CorruptionDetectedEvent {
                    download_id: download_id.to_string(),
                    segment_id,
                    evidence,
                    suggested_recovery: "Re-download from mirror".to_string(),
                },
            );
        }
    }

    // Update mirror reliability (success case)
    let avg_speed = if segment_end > segment_start {
        ((segment_end - segment_start) as u64 * 1000) / 1000 // placeholder
    } else {
        0
    };

    state
        .recovery_manager
        .update_mirror_reliability(mirror_url.to_string(), true, false, avg_speed)
        .await;
}

/// Hook into failed segment to attempt recovery
pub async fn on_segment_failure(
    app_handle: &tauri::AppHandle,
    state: &AppState,
    download_id: &str,
    segment_id: usize,
    segment_start: u64,
    segment_end: u64,
    original_url: &str,
    alternative_mirrors: Vec<String>,
    mirror_url: &str,
) {
    // Get recommended strategy
    let strategy = state
        .recovery_manager
        .get_recovery_strategy(
            download_id,
            segment_id,
            segment_start,
            segment_end,
            original_url,
            alternative_mirrors,
        )
        .await;

    let strategy_str = match &strategy {
        RecoveryStrategy::RetryOriginal { attempt, .. } => format!("Retry (attempt {})", attempt),
        RecoveryStrategy::SwitchMirror { fallback_mirror_url, .. } => {
            format!("Switch to mirror: {}", fallback_mirror_url)
        }
        RecoveryStrategy::ResumeFromOffset { byte_offset, .. } => format!("Resume at {}", byte_offset),
        RecoveryStrategy::SkipSegmentResumeAfter { .. } => "Skip segment".to_string(),
        RecoveryStrategy::PauseForUserInput { reason, .. } => format!("Paused: {}", reason),
    };

    // Emit UI event
    let _ = app_handle.emit(
        "recovery_triggered",
        RecoveryTriggeredEvent {
            download_id: download_id.to_string(),
            segment_id,
            strategy: strategy_str,
            reason: "Segment download failed".to_string(),
        },
    );

    // Update mirror reliability (failure case)
    state
        .recovery_manager
        .update_mirror_reliability(mirror_url.to_string(), false, true, 0)
        .await;
}

/// Periodic cleanup of old recovery data
pub async fn periodic_recovery_cleanup(state: Arc<Mutex<AppState>>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await; // Every hour
        let state = state.lock().await;
        state.recovery_manager.cleanup_old_data().await;
    }
}

// ============= Helper Functions =============

fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut freq = [0u32; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in &freq {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_calculation_uniform() {
        let mut data = vec![];
        for i in 0..256 {
            data.push(i as u8);
        }
        let e = calculate_entropy(&data);
        assert!(e > 7.9); // Nearly max entropy
    }

    #[test]
    fn test_entropy_calculation_zeros() {
        let data = vec![0u8; 256];
        let e = calculate_entropy(&data);
        assert_eq!(e, 0.0);
    }
}
