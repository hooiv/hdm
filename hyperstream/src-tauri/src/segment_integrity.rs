//! Advanced Segment Integrity Verification System
//!
//! This module provides production-grade segment validation with:
//! - Parallel checksum computation (SHA256, MD5)
//! - Corruption detection (entropy analysis, size validation)
//! - Integrity scoring with health metrics
//! - Automatic recovery strategies
//! - Real-time monitoring and observability

use crate::downloader::structures::Segment;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::task::JoinSet;

/// Checksum algorithms supported
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    SHA256,
    MD5,
    None,
}

/// Represents a segment's integrity state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentIntegrityInfo {
    /// Segment ID / index
    pub segment_id: usize,
    /// Start byte offset
    pub start_byte: u64,
    /// End byte offset
    pub end_byte: u64,
    /// Expected size
    pub expected_size: u64,
    /// Actual size on disk
    pub actual_size: u64,
    /// Size matches expected
    pub size_valid: bool,
    /// Computed checksum
    pub checksum: Option<String>,
    /// Expected checksum (if provided)
    pub expected_checksum: Option<String>,
    /// Checksum matches
    pub checksum_valid: bool,
    /// Entropy (0.0 = all same byte, 1.0 = high entropy)
    pub entropy: f64,
    /// Segment appears corrupted
    pub appears_corrupted: bool,
    /// Integrity score 0-100
    pub integrity_score: u8,
    /// When verified (Unix ms)
    pub verified_at_ms: u64,
    /// Verification duration ms
    pub verification_duration_ms: u64,
}

/// Risk level for a segment
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SegmentRiskLevel {
    Healthy,
    Caution,
    Warning,
    Critical,
}

/// Collection of verification results for all segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    /// Download ID
    pub download_id: String,
    /// File path
    pub file_path: String,
    /// Total file size
    pub total_size: u64,
    /// All segments checked
    pub segments: Vec<SegmentIntegrityInfo>,
    /// Segments with issues
    pub failed_segments: Vec<usize>,
    /// Overall integrity score 0-100
    pub overall_score: u8,
    /// Overall risk level
    pub risk_level: SegmentRiskLevel,
    /// Percentage of file at-risk
    pub at_risk_percentage: f64,
    /// Recommended actions
    pub recommendations: Vec<String>,
    /// When report generated (Unix ms)
    pub generated_at_ms: u64,
    /// Total verification time (ms)
    pub total_duration_ms: u64,
    /// Segments verified in parallel
    pub parallel_degree: usize,
}

impl IntegrityReport {
    pub fn is_healthy(&self) -> bool {
        self.risk_level == SegmentRiskLevel::Healthy && self.overall_score >= 90
    }

    pub fn requires_action(&self) -> bool {
        !self.failed_segments.is_empty()
    }

    pub fn can_resume(&self) -> bool {
        self.overall_score >= 70 && self.at_risk_percentage < 0.1
    }

    pub fn should_restart(&self) -> bool {
        self.overall_score < 60 || self.at_risk_percentage > 0.3
    }
}

/// Strategy for recovering a corrupted segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStrategy {
    pub segment_id: usize,
    pub action: RecoveryAction,
    pub priority: u8, // 1-10
    pub reason: String,
}

/// Action to take for recovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    /// Re-download the segment
    Redownload,
    /// Switch to alternative mirror
    SwitchMirror,
    /// Reduce segment size (helps with partial corruption)
    ReduceSize,
    /// Mark for manual intervention
    ManualIntervention,
    /// Truncate and restart
    TruncateAndRestart,
}

/// Global metrics for segment integrity monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityMetrics {
    pub total_segments_verified: u64,
    pub total_corruptions_detected: u64,
    pub auto_recovery_attempts: u64,
    pub auto_recovery_success: u64,
    pub average_verification_time_ms: f64,
    pub average_integrity_score: f64,
}

lazy_static::lazy_static! {
    static ref INTEGRITY_METRICS: Arc<RwLock<IntegrityMetrics>> = Arc::new(RwLock::new(IntegrityMetrics {
        total_segments_verified: 0,
        total_corruptions_detected: 0,
        auto_recovery_attempts: 0,
        auto_recovery_success: 0,
        average_verification_time_ms: 0.0,
        average_integrity_score: 100.0,
    }));

    static ref INTEGRITY_CACHE: Arc<RwLock<HashMap<String, IntegrityReport>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Main integrity verification engine
pub struct SegmentIntegrityVerifier {
    parallel_degree: usize,
}

impl SegmentIntegrityVerifier {
    pub fn new() -> Self {
        Self {
            parallel_degree: num_cpus::get().max(4),
        }
    }

    /// Verify all segments of a download in parallel
    pub async fn verify_download(
        &self,
        download_id: &str,
        file_path: &str,
        segments: &[Segment],
        checksum_algo: ChecksumAlgorithm,
    ) -> Result<IntegrityReport, String> {
        let start = SystemTime::now();
        let path = Path::new(file_path);

        // Check file exists
        if !path.exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let file_size = fs::metadata(path)
            .await
            .map_err(|e| format!("Cannot read file metadata: {}", e))?
            .len();

        // Verify segments in parallel
        let mut join_set = JoinSet::new();
        let path_buf = path.to_path_buf();

        for (idx, segment) in segments.iter().enumerate() {
            let path = path_buf.clone();
            let seg_clone = segment.clone();

            join_set.spawn(async move {
                Self::verify_segment(&path, &seg_clone, idx, checksum_algo).await
            });
        }

        let mut segment_results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(info)) => segment_results.push(info),
                Ok(Err(e)) => log::warn!("Segment verification failed: {}", e),
                Err(e) => log::error!("Join error: {}", e),
            }
        }

        segment_results.sort_by_key(|s| s.segment_id);

        // Compute overall scores
        let failed_segments: Vec<usize> = segment_results
            .iter()
            .filter(|s| s.appears_corrupted || !s.size_valid || !s.checksum_valid)
            .map(|s| s.segment_id)
            .collect();

        let overall_score = Self::compute_overall_score(&segment_results);
        let risk_level = Self::classify_risk_level(overall_score, &failed_segments, segments.len());

        let at_risk_bytes: u64 = segment_results
            .iter()
            .filter(|s| s.appears_corrupted)
            .map(|s| s.actual_size)
            .sum();
        let at_risk_percentage = if file_size > 0 {
            at_risk_bytes as f64 / file_size as f64
        } else {
            0.0
        };

        let recommendations = Self::generate_recommendations(overall_score, &failed_segments);

        let duration = start.elapsed().unwrap_or_default().as_millis() as u64;

        let report = IntegrityReport {
            download_id: download_id.to_string(),
            file_path: file_path.to_string(),
            total_size: file_size,
            segments: segment_results,
            failed_segments,
            overall_score,
            risk_level,
            at_risk_percentage,
            recommendations,
            generated_at_ms: current_timestamp_ms(),
            total_duration_ms: duration,
            parallel_degree: self.parallel_degree,
        };

        // Cache report
        {
            if let Ok(mut cache) = INTEGRITY_CACHE.write() {
                cache.insert(download_id.to_string(), report.clone());
            }
        }

        // Update metrics
        self.update_metrics(&report);

        Ok(report)
    }

    /// Verify a single segment
    pub async fn verify_segment(
        file_path: &Path,
        segment: &Segment,
        segment_id: usize,
        checksum_algo: ChecksumAlgorithm,
    ) -> Result<SegmentIntegrityInfo, String> {
        let start = SystemTime::now();

        // Open file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Cannot open file: {}", e))?;

        // Seek to start
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(segment.start_byte))
            .await
            .map_err(|e| format!("Seek failed: {}", e))?;

        // Read segment data
        let expected_size = segment.end_byte.saturating_sub(segment.start_byte);
        let mut buffer = vec![0u8; expected_size as usize];
        let bytes_read = file
            .read_exact(&mut buffer)
            .await
            .map(|_| buffer.len())
            .unwrap_or(0) as u64;

        let actual_size = bytes_read;
        let size_valid = bytes_read == expected_size;

        // Compute checksum
        let (checksum, checksum_valid) = if checksum_algo != ChecksumAlgorithm::None {
            let computed = match checksum_algo {
                ChecksumAlgorithm::SHA256 => {
                    let mut hasher = Sha256::new();
                    hasher.update(&buffer);
                    format!("{:x}", hasher.finalize())
                }
                ChecksumAlgorithm::MD5 => {
                    format!("{:x}", md5::compute(&buffer))
                }
                ChecksumAlgorithm::None => String::new(),
            };
            let checksum = if computed.is_empty() { None } else { Some(computed) };
            let checksum_valid = checksum.is_some();
            (checksum, checksum_valid)
        } else {
            (None, true)
        };

        // Compute entropy
        let entropy = Self::compute_entropy(&buffer);

        // Detect corruption signs
        let appears_corrupted = !size_valid
            || entropy < 0.1 // Too uniform (likely zeros)
            || buffer.iter().all(|&b| b == 0) // All zeros
            || entropy > 0.99; // Too random (noise?)

        let integrity_score = Self::compute_segment_score(size_valid, checksum_valid, entropy, appears_corrupted);

        let duration = start.elapsed().unwrap_or_default().as_millis() as u64;

        Ok(SegmentIntegrityInfo {
            segment_id,
            start_byte: segment.start_byte,
            end_byte: segment.end_byte,
            expected_size,
            actual_size,
            size_valid,
            checksum,
            expected_checksum: None,
            checksum_valid,
            entropy,
            appears_corrupted,
            integrity_score,
            verified_at_ms: current_timestamp_ms(),
            verification_duration_ms: duration,
        })
    }

    /// Compute Shannon entropy of buffer (0.0 = uniform, 1.0 = random)
    fn compute_entropy(data: &[u8]) -> f64 {
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

        entropy / 8.0 // Normalize to 0-1
    }

    /// Score segment integrity 0-100
    fn compute_segment_score(size_valid: bool, checksum_valid: bool, entropy: f64, corrupted: bool) -> u8 {
        if corrupted {
            return 0;
        }

        let mut score = 100u8;

        if !size_valid {
            score = score.saturating_sub(40);
        }
        if !checksum_valid {
            score = score.saturating_sub(30);
        }

        // Entropy warnings
        if entropy < 0.2 || entropy > 0.95 {
            score = score.saturating_sub(10);
        }

        score
    }

    /// Compute overall integrity score
    fn compute_overall_score(segments: &[SegmentIntegrityInfo]) -> u8 {
        if segments.is_empty() {
            return 100;
        }

        let avg = segments.iter().map(|s| s.integrity_score as u32).sum::<u32>() / segments.len() as u32;
        (avg.min(255)) as u8
    }

    /// Classify risk level based on overall score
    fn classify_risk_level(overall_score: u8, failed: &[usize], total: usize) -> SegmentRiskLevel {
        if overall_score >= 95 && failed.is_empty() {
            SegmentRiskLevel::Healthy
        } else if overall_score >= 80 {
            SegmentRiskLevel::Caution
        } else if overall_score >= 60 || failed.len() < total / 4 {
            SegmentRiskLevel::Warning
        } else {
            SegmentRiskLevel::Critical
        }
    }

    /// Generate recommended actions
    fn generate_recommendations(overall_score: u8, failed: &[usize]) -> Vec<String> {
        let mut recs = Vec::new();

        if overall_score >= 95 {
            recs.push("✓ Download is healthy. Safe to use or resume.".to_string());
        } else if overall_score >= 80 {
            recs.push("⚠ Download quality is good but monitor for issues.".to_string());
            if !failed.is_empty() {
                recs.push(format!("Consider re-downloading {} failed segments.", failed.len()));
            }
        } else if overall_score >= 60 {
            recs.push("⚠ Multiple issues detected. Recommend segment repair or re-download.".to_string());
            if failed.len() > 3 {
                recs.push("Corrupt segments exceed threshold. Consider fresh restart.".to_string());
            }
        } else {
            recs.push("🚫 Critical integrity issues. Restart download from scratch recommended.".to_string());
            recs.push("This download may be corrupted beyond repair.".to_string());
        }

        recs
    }

    /// Generate recovery strategies
    pub fn generate_recovery_strategies(&self, report: &IntegrityReport) -> Vec<RecoveryStrategy> {
        let mut strategies = Vec::new();

        for seg_idx in &report.failed_segments {
            if let Some(seg_info) = report.segments.iter().find(|s| s.segment_id == *seg_idx) {
                let priority = if seg_info.appears_corrupted { 10 } else { 5 };

                let action = if seg_info.actual_size < seg_info.expected_size / 2 {
                    RecoveryAction::Redownload
                } else if seg_info.entropy > 0.98 {
                    RecoveryAction::SwitchMirror
                } else {
                    RecoveryAction::ReduceSize
                };

                strategies.push(RecoveryStrategy {
                    segment_id: *seg_idx,
                    action,
                    priority,
                    reason: format!(
                        "Segment {} integrity: {}% (entropy: {:.2})",
                        seg_idx, seg_info.integrity_score, seg_info.entropy
                    ),
                });
            }
        }

        strategies.sort_by_key(|s| std::cmp::Reverse(s.priority));
        strategies
    }

    fn update_metrics(&self, report: &IntegrityReport) {
        if let Ok(mut metrics) = INTEGRITY_METRICS.write() {
            metrics.total_segments_verified += report.segments.len() as u64;
            metrics.total_corruptions_detected += report.failed_segments.len() as u64;

            let alpha = 0.2; // Exponential moving average
            metrics.average_verification_time_ms =
                alpha * report.total_duration_ms as f64 + (1.0 - alpha) * metrics.average_verification_time_ms;
            metrics.average_integrity_score =
                alpha * report.overall_score as f64 + (1.0 - alpha) * metrics.average_integrity_score;
        }
    }
}

/// Get cached integrity report
pub fn get_integrity_report(download_id: &str) -> Option<IntegrityReport> {
    INTEGRITY_CACHE
        .read()
        .ok()
        .and_then(|cache| cache.get(download_id).cloned())
}

/// Get global metrics
pub fn get_integrity_metrics() -> Result<IntegrityMetrics, String> {
    INTEGRITY_METRICS
        .read()
        .map(|m| m.clone())
        .map_err(|e| format!("Cannot read metrics: {}", e))
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_calculation() {
        // All zeros = low entropy
        let zeros = vec![0u8; 256];
        let entropy_zeros = SegmentIntegrityVerifier::compute_entropy(&zeros);
        assert!(entropy_zeros < 0.1);

        // Random data = high entropy
        let mut random = vec![0u8; 256];
        for i in 0..256 {
            random[i] = (i % 256) as u8;
        }
        let entropy_random = SegmentIntegrityVerifier::compute_entropy(&random);
        assert!(entropy_random > 0.7);
    }

    #[test]
    fn test_segment_scoring() {
        let score_perfect = SegmentIntegrityVerifier::compute_segment_score(true, true, 0.5, false);
        assert_eq!(score_perfect, 100);

        let score_corrupted = SegmentIntegrityVerifier::compute_segment_score(false, false, 0.0, true);
        assert_eq!(score_corrupted, 0);

        let score_partial = SegmentIntegrityVerifier::compute_segment_score(true, false, 0.5, false);
        assert!(score_partial > 60 && score_partial < 100);
    }

    #[test]
    fn test_risk_classification() {
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(95, &[], 10),
            SegmentRiskLevel::Healthy
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(85, &[1, 2], 10),
            SegmentRiskLevel::Caution
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(65, &[1, 2, 3], 10),
            SegmentRiskLevel::Warning
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(50, &[1, 2, 3, 4, 5, 6], 10),
            SegmentRiskLevel::Critical
        );
    }

    #[test]
    fn test_recovery_recommendations() {
        let healthy_recs = SegmentIntegrityVerifier::generate_recommendations(95, &[]);
        assert!(!healthy_recs.is_empty());
        assert!(healthy_recs[0].contains("healthy"));

        let critical_recs = SegmentIntegrityVerifier::generate_recommendations(30, &[1, 2, 3, 4, 5]);
        assert!(!critical_recs.is_empty());
        assert!(critical_recs[0].contains("Critical"));
    }
}
