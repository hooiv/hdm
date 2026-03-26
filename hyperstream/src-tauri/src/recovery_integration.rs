//! Integration of corruption recovery into the download engine
//!
//! This module provides hooks that integrate the recovery system into the
//! core download lifecycle. It automatically:
//! - Detects corruption during segment completion
//! - Suggests recovery strategies
//! - Emits UI events for user notification
//! - Records mirror reliability metrics

use crate::core_state::AppState;
use crate::download_recovery::{CorruptionEvidence, CorruptionType, RecoveryStrategy};
use sha2::Digest;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

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
    // Quick exit for empty data
    if data.is_empty() {
        return;
    }

    let mut detected_corruption = false;

    // 1. Check for size consistency
    let expected_size = segment_end - segment_start;
    if data.len() as u64 != expected_size {
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::SizeMismatch {
                expected: expected_size,
                actual: data.len() as u64,
            },
            confidence: 95,
            detected_at_ms: current_time_ms(),
            evidence_data: format!("Expected {} bytes, got {} bytes", expected_size, data.len()),
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
                suggested_recovery: "Resume from offset".to_string(),
            },
        );
        detected_corruption = true;
    }

    // 2. Check checksum if provided
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
            detected_corruption = true;
        }
    }

    // 3. Check for GZIP-compressed data (low entropy is expected for valid GZIP)
    let entropy = calculate_entropy(data);
    let is_gzip = is_gzip_data(data);

    if entropy == 0.0 {
        // All bytes identical - definitely corruption
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::ZeroEntropy,
            confidence: 98,
            detected_at_ms: current_time_ms(),
            evidence_data: "All bytes are identical - definite corruption".to_string(),
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
                suggested_recovery: "Retry or switch mirror".to_string(),
            },
        );
        detected_corruption = true;
    } else if entropy < 1.5 && !is_gzip {
        // Low entropy but NOT gzip - suspect
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::LowEntropy {
                entropy,
                threshold: 1.5,
            },
            confidence: 65,
            detected_at_ms: current_time_ms(),
            evidence_data: format!(
                "Entropy {:.4} below threshold 1.5 (not GZIP, possibly corrupted)",
                entropy
            ),
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
                suggested_recovery: "Switch mirror or retry from offset".to_string(),
            },
        );
        detected_corruption = true;
    }

    // 4. Update mirror reliability
    let segment_size = segment_end - segment_start;
    state
        .recovery_manager
        .update_mirror_reliability(
            mirror_url.to_string(),
            !detected_corruption, // success if no corruption detected
            false,
            segment_size, // use segment size as proxy for speed metric
        )
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
    // Report failure to mirror reliability
    state
        .recovery_manager
        .update_mirror_reliability(mirror_url.to_string(), false, true, 0)
        .await;

    // Get recommended strategy based on attempt history
    let strategy = state
        .recovery_manager
        .get_recovery_strategy(
            download_id,
            segment_id,
            segment_start,
            segment_end,
            original_url,
            alternative_mirrors.clone(),
        )
        .await;

    let strategy_str = match &strategy {
        RecoveryStrategy::RetryOriginal {
            attempt,
            max_attempts,
        } => {
            format!("Retry original (attempt {}/{})", attempt, max_attempts)
        }
        RecoveryStrategy::SwitchMirror {
            fallback_mirror_url,
            ..
        } => {
            format!("Switch to mirror: {}", fallback_mirror_url)
        }
        RecoveryStrategy::ResumeFromOffset { byte_offset, .. } => {
            format!("Resume at offset {}", byte_offset)
        }
        RecoveryStrategy::SkipSegmentResumeAfter { .. } => "Skip segment".to_string(),
        RecoveryStrategy::PauseForUserInput { reason, .. } => format!("Paused: {}", reason),
    };

    // Emit UI event for recovery attempt
    let _ = app_handle.emit(
        "recovery_triggered",
        RecoveryTriggeredEvent {
            download_id: download_id.to_string(),
            segment_id,
            strategy: strategy_str.clone(),
            reason: "Segment download failed".to_string(),
        },
    );

    eprintln!(
        "[Recovery] Download {} segment {} failed, attempting: {}",
        download_id, segment_id, strategy_str
    );
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

/// Detect if data is GZIP compressed
/// GZIP files start with magic bytes: 0x1f 0x8b
fn is_gzip_data(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

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
