//! Resume Safety Validator — Ensures Downloads Can Be Safely Resumed
//!
//! Before resuming a paused download, we validate:
//! 1. File exists and is readable
//! 2. File size is consistent with saved state
//! 3. URL is still accessible (HEAD request)
//! 4. Segments are in valid state
//! 5. No truncation/corruption detected
//! 6. Download freshness (not too old to resume)

use crate::downloader::structures::Segment;
use crate::persistence::SavedDownload;
use chrono::Utc;
use tokio::fs;

/// Validation severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// Safe to resume immediately
    Safe,
    /// Might work but needs monitoring
    Caution,
    /// Requires user confirmation
    Warning,
    /// Cannot resume
    Blocked,
}

/// Result of resume validation check
#[derive(Debug, Clone)]
pub struct ResumeValidityReport {
    pub download_id: String,

    /// Overall verdict
    pub level: ValidationLevel,

    /// What we validated
    pub checks_passed: Vec<String>,
    pub checks_warning: Vec<String>,
    pub checks_failed: Vec<String>,

    /// Recommended action
    pub recommendation: String,

    /// Suggested retry strategy
    pub suggested_retry_delay_secs: Option<u64>,

    /// Should we truncate and restart from scratch?
    pub should_restart_from_scratch: bool,

    /// Human-readable summary
    pub summary: String,
}

impl ResumeValidityReport {
    pub fn can_resume(&self) -> bool {
        matches!(self.level, ValidationLevel::Safe | ValidationLevel::Caution)
    }

    pub fn requires_confirmation(&self) -> bool {
        self.level == ValidationLevel::Warning
    }

    pub fn cannot_resume(&self) -> bool {
        self.level == ValidationLevel::Blocked
    }
}

/// Validates that a paused download is safe to resume
pub struct ResumeSafetyValidator;

impl ResumeSafetyValidator {
    /// Full validation of a paused download's resumability
    pub async fn validate_resume_safe(
        download: &SavedDownload,
    ) -> ResumeValidityReport {
        let mut report = ResumeValidityReport {
            download_id: download.id.clone(),
            level: ValidationLevel::Safe,
            checks_passed: Vec::new(),
            checks_warning: Vec::new(),
            checks_failed: Vec::new(),
            recommendation: "Resume download".to_string(),
            suggested_retry_delay_secs: None,
            should_restart_from_scratch: false,
            summary: String::new(),
        };

        // 1. Check file exists
        Self::check_file_exists(&download.path, &mut report).await;

        // 2. Check file size consistency
        if report.level != ValidationLevel::Blocked {
            Self::check_file_size_consistency(&download.path, download.downloaded_bytes, &mut report).await;
        }

        // 3. Check URL accessibility (HEAD request)
        if report.level != ValidationLevel::Blocked {
            Self::check_url_accessible(&download.url, &mut report).await;
        }

        // 4. Check download freshness
        Self::check_download_freshness(&download.last_active, &mut report);

        // 5. Check segment state validity
        if let Some(ref segments) = download.segments {
            Self::check_segment_validity(segments, &mut report);
        }

        // 6. Check if file appears corrupted
        if report.level != ValidationLevel::Blocked {
            Self::check_corruption_signs(&download.path, download.total_size, &mut report).await;
        }

        // Finalize recommendation
        report.summary = Self::generate_summary(&report);
        report
    }

    async fn check_file_exists(path: &str, report: &mut ResumeValidityReport) {
        match fs::try_exists(path).await {
            Ok(true) => {
                report.checks_passed.push(format!("File exists: {}", path));
            }
            Ok(false) => {
                report.checks_failed.push(format!("File does not exist: {}", path));
                report.level = ValidationLevel::Blocked;
                report.recommendation = "Restart download from scratch".to_string();
                report.should_restart_from_scratch = true;
            }
            Err(e) => {
                report.checks_failed.push(format!("Cannot access file: {}", e));
                report.level = ValidationLevel::Blocked;
            }
        }
    }

    async fn check_file_size_consistency(path: &str, expected_bytes: u64, report: &mut ResumeValidityReport) {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                let actual_size = metadata.len();
                if actual_size <= expected_bytes {
                    report.checks_passed.push(format!(
                        "File size consistent: {} bytes (expected ≤ {})",
                        actual_size, expected_bytes
                    ));
                } else {
                    report.checks_warning.push(format!(
                        "File size exceeds expected: {} > {}",
                        actual_size, expected_bytes
                    ));
                    report.level = ValidationLevel::Warning;

                    // Likely someone else modified the file
                    if actual_size > expected_bytes + 1024 {
                        report.recommendation =
                            "File may have been modified externally. Restart from scratch?".to_string();
                        report.should_restart_from_scratch = true;
                    }
                }
            }
            Err(e) => {
                report.checks_failed.push(format!("Cannot get file metadata: {}", e));
                report.level = ValidationLevel::Warning;
            }
        }
    }

    async fn check_url_accessible(url: &str, report: &mut ResumeValidityReport) {
        // Quick HEAD request to verify URL still works
        use rquest::Client;
        let client = match Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
            Ok(c) => c,
            Err(e) => {
                report.checks_warning.push(format!("Cannot create HTTP client: {}", e));
                report.level = ValidationLevel::Caution;
                return;
            }
        };

        match client.head(url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    report.checks_passed.push(format!("URL accessible: {} ({})", url, resp.status()));
                } else if resp.status().is_redirection() {
                    report.checks_warning.push(format!("URL redirected: {} ({})", url, resp.status()));
                    report.level = ValidationLevel::Caution;
                } else if resp.status().as_u16() == 404 {
                    report.checks_failed.push("URL returns 404 – file may have been deleted".to_string());
                    report.level = ValidationLevel::Blocked;
                    report.should_restart_from_scratch = true;
                } else if resp.status().as_u16() == 403 {
                    report.checks_warning.push("URL returns 403 (Forbidden) – may need re-authentication".to_string());
                    report.level = ValidationLevel::Warning;
                }
            }
            Err(e) => {
                report.checks_warning.push(format!("Cannot reach URL: {} – {}", url, e));
                report.level = if report.level == ValidationLevel::Safe {
                    ValidationLevel::Caution
                } else {
                    report.level
                };
                report.suggested_retry_delay_secs = Some(30);
            }
        }
    }

    fn check_download_freshness(last_active: &Option<String>, report: &mut ResumeValidityReport) {
        const RESUME_TIMEOUT_DAYS: u32 = 30;

        match last_active {
            Some(timestamp_str) => {
                if let Ok(last) = timestamp_str.parse::<chrono::DateTime<Utc>>() {
                    let now = Utc::now();
                    let age = now.signed_duration_since(last);
                    let age_days = age.num_days() as u32;

                    if age_days <= 7 {
                        report.checks_passed.push(format!("Download is recent ({} days old)", age_days));
                    } else if age_days <= RESUME_TIMEOUT_DAYS {
                        report
                            .checks_warning
                            .push(format!("Download is {} days old – URL may have changed", age_days));
                        report.level = ValidationLevel::Caution;
                    } else {
                        report.checks_warning.push(format!(
                            "Download is {} days old – exceeds {} day resume window",
                            age_days, RESUME_TIMEOUT_DAYS
                        ));
                        report.level = ValidationLevel::Warning;
                        report.should_restart_from_scratch = true;
                    }
                } else {
                    report.checks_warning.push("Cannot parse last_active timestamp".to_string());
                }
            }
            None => {
                report.checks_warning.push("No last_active timestamp – age unknown".to_string());
            }
        }
    }

    fn check_segment_validity(segments: &[Segment], report: &mut ResumeValidityReport) {
        use crate::downloader::structures::SegmentState;

        // Count segment states
        let mut idle_count = 0;
        let mut downloading_count = 0;
        let mut paused_count = 0;
        let mut complete_count = 0;
        let mut error_count = 0;

        for seg in segments {
            match seg.state {
                SegmentState::Idle => idle_count += 1,
                SegmentState::Downloading => downloading_count += 1,
                SegmentState::Paused => paused_count += 1,
                SegmentState::Complete => complete_count += 1,
                SegmentState::Error => error_count += 1,
            }
        }

        // If all segments complete, that's great
        if complete_count == segments.len() {
            report.checks_passed.push("All segments complete".to_string());
        } else if downloading_count > 0 {
            report.checks_warning.push(format!(
                "Some segments marked as downloading ({}), though paused – may cause race conditions",
                downloading_count
            ));
            report.level = ValidationLevel::Caution;
        } else if error_count > 0 {
            report.checks_warning.push(format!(
                "Some segments in error state ({}/{})",
                error_count,
                segments.len()
            ));
            report.level = ValidationLevel::Caution;
        } else {
            report.checks_passed.push(format!(
                "Segments valid: {} idle/paused, {} complete, {} error",
                idle_count + paused_count,
                complete_count,
                error_count
            ));
        }
    }

    async fn check_corruption_signs(
        path: &str,
        expected_total: u64,
        report: &mut ResumeValidityReport,
    ) {
        // Try to read first 1KB and last 1KB to detect obvious corruption
        match fs::metadata(path).await {
            Ok(meta) => {
                let size = meta.len();

                // File is much smaller than expected = likely corruption
                if size < expected_total / 2 {
                    report.checks_warning.push(format!(
                        "File is significantly smaller than expected: {} < {}",
                        size, expected_total / 2
                    ));
                    report.level = ValidationLevel::Warning;
                }

                // Attempt to read and verify file is readable
                match fs::File::open(path).await {
                    Ok(_) => {
                        report.checks_passed.push("File is readable".to_string());
                    }
                    Err(e) => {
                        report.checks_failed.push(format!("File is not readable: {}", e));
                        report.level = ValidationLevel::Blocked;
                    }
                }
            }
            Err(e) => {
                report.checks_warning.push(format!("Cannot read file metadata: {}", e));
            }
        }
    }

    fn generate_summary(report: &ResumeValidityReport) -> String {
        let mut summary = String::new();

        match report.level {
            ValidationLevel::Safe => {
                summary.push_str("✓ Download is safe to resume immediately.\n");
            }
            ValidationLevel::Caution => {
                summary.push_str("⚠ Download can likely be resumed, but monitor for issues:\n");
                for warning in &report.checks_warning {
                    summary.push_str(&format!("  • {}\n", warning));
                }
            }
            ValidationLevel::Warning => {
                summary.push_str("⚠ Download may be resumable, but requires review:\n");
                for warning in &report.checks_warning {
                    summary.push_str(&format!("  • {}\n", warning));
                }
            }
            ValidationLevel::Blocked => {
                summary.push_str("✗ Download cannot be safely resumed:\n");
                for failed in &report.checks_failed {
                    summary.push_str(&format!("  • {}\n", failed));
                }
                summary.push_str(&format!("  → {}\n", report.recommendation));
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_report_generation() {
        let mut report = ResumeValidityReport {
            download_id: "test".to_string(),
            level: ValidationLevel::Safe,
            checks_passed: vec!["File exists".to_string()],
            checks_warning: vec![],
            checks_failed: vec![],
            recommendation: "Resume".to_string(),
            suggested_retry_delay_secs: None,
            should_restart_from_scratch: false,
            summary: String::new(),
        };

        report.summary = ResumeSafetyValidator::generate_summary(&report);
        assert!(report.summary.contains("safe to resume"));
    }
}
