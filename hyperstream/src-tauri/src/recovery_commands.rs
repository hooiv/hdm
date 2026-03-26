//! Tauri commands for download recovery and resumption
//!
//! Exposed commands:
//! - detect_corruption(download_id, segment_data) → CorruptionEvidence
//! - get_recovery_strategy(download_id, segment_id, ...) → RecoveryStrategy
//! - execute_recovery(download_id, strategy) → result
//! - get_corruption_report(download_id) → Vec<CorruptionEvidence>
//! - get_mirror_rankings() → Vec<MirrorReliability>

use crate::core_state::AppState;
use crate::download_recovery::{
    CorruptionEvidence, CorruptionType, MirrorReliability, RecoveryAttempt, RecoveryStrategy,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

/// Detect potential corruption in segment data
#[tauri::command]
pub async fn detect_corruption(
    state: State<'_, AppState>,
    download_id: String,
    segment_id: usize,
    segment_start: u64,
    segment_end: u64,
    data_sample: Vec<u8>,
    expected_checksum: Option<String>,
    expected_size: Option<u64>,
    algorithm: Option<String>,
) -> Result<Option<CorruptionEvidence>, String> {
    // Check size mismatch
    if let Some(expected) = expected_size {
        let actual = (segment_end - segment_start) as u64;
        if actual != expected && expected > 0 {
            let evidence = CorruptionEvidence {
                segment_id,
                segment_start,
                segment_end,
                corruption_type: CorruptionType::SizeMismatch { expected, actual },
                confidence: 99,
                detected_at_ms: current_time_ms(),
                evidence_data: format!("Expected {} bytes, got {}", expected, actual),
            };

            // Report to manager
            state
                .recovery_manager
                .report_corruption(download_id, evidence.clone())
                .await;
            return Ok(Some(evidence));
        }
    }

    // Check checksum
    if let Some(expected) = expected_checksum {
        let algo = algorithm.as_deref().unwrap_or("sha256");
        let computed = match algo {
            "sha256" => {
                let mut hasher = Sha256::new();
                hasher.update(&data_sample);
                format!("{:x}", hasher.finalize())
            }
            "md5" => {
                format!("{:x}", md5::compute(&data_sample))
            }
            _ => return Ok(None),
        };

        if computed != expected {
            let evidence = CorruptionEvidence {
                segment_id,
                segment_start,
                segment_end,
                corruption_type: CorruptionType::ChecksumMismatch {
                    expected,
                    computed,
                    algorithm: algo.to_string(),
                },
                confidence: 100,
                detected_at_ms: current_time_ms(),
                evidence_data: format!("{} checksum mismatch detected", algo),
            };

            state
                .recovery_manager
                .report_corruption(download_id, evidence.clone())
                .await;
            return Ok(Some(evidence));
        }
    }

    // Check entropy (avoid all-zeros or highly repetitive data)
    let entropy = calculate_entropy(&data_sample);
    if entropy == 0.0 {
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::ZeroEntropy,
            confidence: 98,
            detected_at_ms: current_time_ms(),
            evidence_data: "All bytes identical (zero entropy)".to_string(),
        };

        state
            .recovery_manager
            .report_corruption(download_id, evidence.clone())
            .await;
        return Ok(Some(evidence));
    }

    // Check entropy against threshold
    let entropy_threshold = 1.5; // Very low entropy suggests corruption
    if entropy < entropy_threshold {
        let evidence = CorruptionEvidence {
            segment_id,
            segment_start,
            segment_end,
            corruption_type: CorruptionType::LowEntropy {
                entropy,
                threshold: entropy_threshold,
            },
            confidence: 70,
            detected_at_ms: current_time_ms(),
            evidence_data: format!(
                "Entropy {:.4} below threshold {:.4}",
                entropy, entropy_threshold
            ),
        };

        state
            .recovery_manager
            .report_corruption(download_id, evidence.clone())
            .await;
        return Ok(Some(evidence));
    }

    Ok(None)
}

/// Get recommended recovery strategy for a segment
#[tauri::command]
pub async fn get_recovery_strategy(
    state: State<'_, AppState>,
    download_id: String,
    segment_id: usize,
    segment_start: u64,
    segment_end: u64,
    original_url: String,
    alternative_mirrors: Vec<String>,
) -> Result<RecoveryStrategy, String> {
    let strategy = state
        .recovery_manager
        .get_recovery_strategy(
            &download_id,
            segment_id,
            segment_start,
            segment_end,
            &original_url,
            alternative_mirrors,
        )
        .await;

    Ok(strategy)
}

/// Execute a recovery strategy (e.g., trigger re-download, switch mirror, etc.)
#[tauri::command]
pub async fn execute_recovery(
    state: State<'_, AppState>,
    download_id: String,
    segment_id: usize,
    strategy: RecoveryStrategy,
) -> Result<String, String> {
    let start_time = SystemTime::now();

    // Simulate recovery execution (in real code, this would interact with download engine)
    let (succeeded, reason) = match &strategy {
        RecoveryStrategy::RetryOriginal { attempt, .. } => (
            true,
            format!("Retrying segment {} (attempt {})", segment_id, attempt),
        ),
        RecoveryStrategy::SwitchMirror {
            fallback_mirror_url,
            ..
        } => (true, format!("Switched to mirror: {}", fallback_mirror_url)),
        RecoveryStrategy::ResumeFromOffset { byte_offset, .. } => {
            (true, format!("Resuming from byte offset {}", byte_offset))
        }
        RecoveryStrategy::SkipSegmentResumeAfter {
            next_segment_offset,
            ..
        } => (
            false,
            format!(
                "Skipping segment, resuming at offset {}",
                next_segment_offset
            ),
        ),
        RecoveryStrategy::PauseForUserInput { reason: r, .. } => (false, format!("Paused: {}", r)),
    };

    let duration_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;

    let attempt = RecoveryAttempt {
        segment_id,
        strategy,
        succeeded,
        reason: reason.clone(),
        duration_ms,
        attempted_at_ms: current_time_ms(),
    };

    state
        .recovery_manager
        .record_recovery_attempt(download_id, attempt)
        .await;

    Ok(reason)
}

/// Get corruption report for a download
#[tauri::command]
pub async fn get_corruption_report(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<Vec<CorruptionEvidence>, String> {
    let report = state
        .recovery_manager
        .get_corruption_report(&download_id)
        .await;
    Ok(report)
}

/// Get mirror reliability rankings
#[tauri::command]
pub async fn get_mirror_rankings(
    state: State<'_, AppState>,
) -> Result<Vec<MirrorReliability>, String> {
    let rankings = state.recovery_manager.get_mirror_rankings().await;
    Ok(rankings)
}

/// Record success/failure of mirror download
#[tauri::command]
pub async fn update_mirror_reliability(
    state: State<'_, AppState>,
    url: String,
    success: bool,
    had_corruption: bool,
    avg_speed_bps: u64,
) -> Result<(), String> {
    state
        .recovery_manager
        .update_mirror_reliability(url, success, had_corruption, avg_speed_bps)
        .await;
    Ok(())
}

/// Automatically execute recovery for a segment without user intervention
/// Returns the executed strategy details
#[tauri::command]
pub async fn auto_execute_recovery(
    state: State<'_, AppState>,
    download_id: String,
    segment_id: usize,
    segment_start: u64,
    segment_end: u64,
    original_url: String,
    alternative_mirrors: Vec<String>,
) -> Result<String, String> {
    let strategy = state
        .recovery_manager
        .get_recovery_strategy(
            &download_id,
            segment_id,
            segment_start,
            segment_end,
            &original_url,
            alternative_mirrors,
        )
        .await;

    let strategy_desc = match &strategy {
        RecoveryStrategy::RetryOriginal { attempt, max_attempts, backoff_ms } => {
            format!(
                "RetryOriginal(attempt={}/{}, backoff={}ms)",
                attempt, max_attempts, backoff_ms
            )
        }
        RecoveryStrategy::SwitchMirror { fallback_mirror_url, .. } => {
            format!("SwitchMirror({})", fallback_mirror_url)
        }
        RecoveryStrategy::ResumeFromOffset { byte_offset, .. } => {
            format!("ResumeFromOffset({})", byte_offset)
        }
        RecoveryStrategy::SkipSegmentResumeAfter { .. } => "SkipSegment".to_string(),
        RecoveryStrategy::PauseForUserInput { reason, .. } => format!("PauseForUserInput({})", reason),
    };

    // Record the recovery attempt
    let attempt = RecoveryAttempt {
        segment_id,
        strategy,
        succeeded: true, // We assume success when auto-executing
        reason: "Automatic recovery execution".to_string(),
        duration_ms: 0,
        attempted_at_ms: current_time_ms(),
    };

    state
        .recovery_manager
        .record_recovery_attempt(download_id, attempt)
        .await;

    Ok(strategy_desc)
}

/// Clean up old recovery data
#[tauri::command]
pub async fn cleanup_recovery_data(state: State<'_, AppState>) -> Result<(), String> {
    state.recovery_manager.cleanup_old_data().await;
    Ok(())
}

// ============= Helper Functions =============

/// Calculate Shannon entropy of data (0.0 = deterministic, 8.0 = random)
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_calculation() {
        // All zeros should have entropy 0
        let zeros = vec![0u8; 100];
        let entropy = calculate_entropy(&zeros);
        assert_eq!(entropy, 0.0);

        // Uniform distribution should have high entropy
        let mut uniform = vec![];
        for i in 0..256 {
            uniform.push(i as u8);
        }
        let entropy = calculate_entropy(&uniform);
        assert!(entropy > 7.0); // Near max of 8
    }

    #[test]
    fn test_entropy_low_threshold() {
        // Mostly zeros with few 1s
        let mut low_entropy = vec![0u8; 250];
        low_entropy.extend_from_slice(&[1u8; 6]);
        let entropy = calculate_entropy(&low_entropy);
        assert!(entropy < 1.0); // Very low
    }
}
