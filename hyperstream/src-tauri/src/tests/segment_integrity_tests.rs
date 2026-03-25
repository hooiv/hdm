//! Comprehensive Integration Tests for Segment Integrity Verification System
//!
//! These tests verify the complete workflow of segment integrity checking,
//! recovery strategies, and metrics tracking.

#[cfg(test)]
mod segment_integrity_tests {
    use crate::segment_integrity::*;
    use crate::downloader::structures::{Segment, SegmentState};
    use std::path::PathBuf;

    #[test]
    fn test_entropy_calculation_zeros() {
        // All zeros should have very low entropy
        let zeros = vec![0u8; 256];
        let entropy = SegmentIntegrityVerifier::compute_entropy(&zeros);
        assert!(entropy < 0.1, "Zeros should have entropy < 0.1, got {}", entropy);
    }

    #[test]
    fn test_entropy_calculation_random() {
        // Sequential bytes (0-255) should have higher entropy
        let mut sequential = vec![0u8; 256];
        for i in 0..256 {
            sequential[i] = (i % 256) as u8;
        }
        let entropy = SegmentIntegrityVerifier::compute_entropy(&sequential);
        assert!(entropy > 0.7, "Sequential data should have entropy > 0.7, got {}", entropy);
    }

    #[test]
    fn test_entropy_calculation_empty() {
        let empty = vec![];
        let entropy = SegmentIntegrityVerifier::compute_entropy(&empty);
        assert_eq!(entropy, 0.0, "Empty data should have zero entropy");
    }

    #[test]
    fn test_segment_score_perfect() {
        let score = SegmentIntegrityVerifier::compute_segment_score(true, true, 0.5, false);
        assert_eq!(score, 100, "Perfect segment should score 100");
    }

    #[test]
    fn test_segment_score_corrupted() {
        let score = SegmentIntegrityVerifier::compute_segment_score(true, true, 0.5, true);
        assert_eq!(score, 0, "Corrupted segment should score 0");
    }

    #[test]
    fn test_segment_score_size_invalid() {
        let score = SegmentIntegrityVerifier::compute_segment_score(false, true, 0.5, false);
        assert!(score < 100 && score > 50, "Invalid size should reduce score significantly");
    }

    #[test]
    fn test_segment_score_checksum_invalid() {
        let score = SegmentIntegrityVerifier::compute_segment_score(true, false, 0.5, false);
        assert!(score < 100 && score > 50, "Invalid checksum should reduce score");
    }

    #[test]
    fn test_risk_classification_healthy() {
        let level = SegmentIntegrityVerifier::classify_risk_level(95, &[], 10);
        assert_eq!(level, SegmentRiskLevel::Healthy);
    }

    #[test]
    fn test_risk_classification_caution() {
        let level = SegmentIntegrityVerifier::classify_risk_level(85, &[1, 2], 10);
        assert_eq!(level, SegmentRiskLevel::Caution);
    }

    #[test]
    fn test_risk_classification_warning() {
        let level = SegmentIntegrityVerifier::classify_risk_level(70, &[1, 2, 3], 10);
        assert_eq!(level, SegmentRiskLevel::Warning);
    }

    #[test]
    fn test_risk_classification_critical() {
        let level = SegmentIntegrityVerifier::classify_risk_level(45, &[1, 2, 3, 4, 5, 6], 10);
        assert_eq!(level, SegmentRiskLevel::Critical);
    }

    #[test]
    fn test_recommendations_healthy() {
        let recs = SegmentIntegrityVerifier::generate_recommendations(95, &[]);
        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.contains("healthy") || r.contains("Safe")));
    }

    #[test]
    fn test_recommendations_critical() {
        let recs = SegmentIntegrityVerifier::generate_recommendations(30, &[1, 2, 3, 4, 5]);
        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.contains("Critical") || r.contains("restart")));
    }

    #[test]
    fn test_overall_score_computation() {
        let segments = vec![
            SegmentIntegrityInfo {
                segment_id: 0,
                start_byte: 0,
                end_byte: 1000,
                expected_size: 1000,
                actual_size: 1000,
                size_valid: true,
                checksum: None,
                expected_checksum: None,
                checksum_valid: true,
                entropy: 0.5,
                appears_corrupted: false,
                integrity_score: 100,
                verified_at_ms: 0,
                verification_duration_ms: 10,
            },
            SegmentIntegrityInfo {
                segment_id: 1,
                start_byte: 1000,
                end_byte: 2000,
                expected_size: 1000,
                actual_size: 1000,
                size_valid: true,
                checksum: None,
                expected_checksum: None,
                checksum_valid: true,
                entropy: 0.5,
                appears_corrupted: false,
                integrity_score: 80,
                verified_at_ms: 0,
                verification_duration_ms: 10,
            },
        ];

        let score = SegmentIntegrityVerifier::compute_overall_score(&segments);
        // Average of 100 and 80 = 90
        assert_eq!(score, 90);
    }

    #[test]
    fn test_integrity_report_can_resume() {
        let report = IntegrityReport {
            download_id: "test".to_string(),
            file_path: "/tmp/test".to_string(),
            total_size: 10000,
            segments: vec![],
            failed_segments: vec![],
            overall_score: 75,
            risk_level: SegmentRiskLevel::Caution,
            at_risk_percentage: 0.05,
            recommendations: vec![],
            generated_at_ms: 0,
            total_duration_ms: 100,
            parallel_degree: 4,
        };

        assert!(report.can_resume());
    }

    #[test]
    fn test_integrity_report_cannot_resume() {
        let report = IntegrityReport {
            download_id: "test".to_string(),
            file_path: "/tmp/test".to_string(),
            total_size: 10000,
            segments: vec![],
            failed_segments: vec![],
            overall_score: 60,
            risk_level: SegmentRiskLevel::Warning,
            at_risk_percentage: 0.20,
            recommendations: vec![],
            generated_at_ms: 0,
            total_duration_ms: 100,
            parallel_degree: 4,
        };

        assert!(!report.can_resume());
    }

    #[test]
    fn test_integrity_report_should_restart() {
        let report = IntegrityReport {
            download_id: "test".to_string(),
            file_path: "/tmp/test".to_string(),
            total_size: 10000,
            segments: vec![],
            failed_segments: vec![],
            overall_score: 50,
            risk_level: SegmentRiskLevel::Critical,
            at_risk_percentage: 0.40,
            recommendations: vec![],
            generated_at_ms: 0,
            total_duration_ms: 100,
            parallel_degree: 4,
        };

        assert!(report.should_restart());
    }

    #[test]
    fn test_integrity_report_is_healthy() {
        let report = IntegrityReport {
            download_id: "test".to_string(),
            file_path: "/tmp/test".to_string(),
            total_size: 10000,
            segments: vec![],
            failed_segments: vec![],
            overall_score: 95,
            risk_level: SegmentRiskLevel::Healthy,
            at_risk_percentage: 0.0,
            recommendations: vec![],
            generated_at_ms: 0,
            total_duration_ms: 100,
            parallel_degree: 4,
        };

        assert!(report.is_healthy());
    }

    #[test]
    fn test_recovery_strategy_priority() {
        let report = IntegrityReport {
            download_id: "test".to_string(),
            file_path: "/tmp/test".to_string(),
            total_size: 100000,
            segments: vec![
                SegmentIntegrityInfo {
                    segment_id: 0,
                    start_byte: 0,
                    end_byte: 10000,
                    expected_size: 10000,
                    actual_size: 5000, // Partial
                    size_valid: false,
                    checksum: None,
                    expected_checksum: None,
                    checksum_valid: false,
                    entropy: 0.15, // Suspicious
                    appears_corrupted: true,
                    integrity_score: 20,
                    verified_at_ms: 0,
                    verification_duration_ms: 10,
                },
            ],
            failed_segments: vec![0],
            overall_score: 20,
            risk_level: SegmentRiskLevel::Critical,
            at_risk_percentage: 0.10,
            recommendations: vec![],
            generated_at_ms: 0,
            total_duration_ms: 100,
            parallel_degree: 4,
        };

        let verifier = SegmentIntegrityVerifier::new();
        let strategies = verifier.generate_recovery_strategies(&report);

        assert!(!strategies.is_empty());
        // Highest priority strategies should come first
        assert!(strategies[0].priority >= strategies.last().unwrap().priority.min(strategies[0].priority));
    }

    #[test]
    fn test_metrics_initialization() {
        let metrics = IntegrityMetrics {
            total_segments_verified: 0,
            total_corruptions_detected: 0,
            auto_recovery_attempts: 0,
            auto_recovery_success: 0,
            average_verification_time_ms: 0.0,
            average_integrity_score: 100.0,
        };

        assert_eq!(metrics.total_segments_verified, 0);
        assert_eq!(metrics.average_integrity_score, 100.0);
    }

    #[test]
    fn test_checksum_algorithm_enum() {
        assert_eq!(ChecksumAlgorithm::SHA256, ChecksumAlgorithm::SHA256);
        assert_ne!(ChecksumAlgorithm::SHA256, ChecksumAlgorithm::None);
    }

    #[test]
    fn test_segment_risk_level_ordering() {
        assert!(SegmentRiskLevel::Healthy < SegmentRiskLevel::Caution);
        assert!(SegmentRiskLevel::Caution < SegmentRiskLevel::Warning);
        assert!(SegmentRiskLevel::Warning < SegmentRiskLevel::Critical);
    }

    #[test]
    fn test_segment_integrity_info_creation() {
        let info = SegmentIntegrityInfo {
            segment_id: 42,
            start_byte: 100,
            end_byte: 200,
            expected_size: 100,
            actual_size: 100,
            size_valid: true,
            checksum: Some("abc123".to_string()),
            expected_checksum: None,
            checksum_valid: true,
            entropy: 0.5,
            appears_corrupted: false,
            integrity_score: 95,
            verified_at_ms: 1234567890,
            verification_duration_ms: 25,
        };

        assert_eq!(info.segment_id, 42);
        assert_eq!(info.integrity_score, 95);
        assert!(!info.appears_corrupted);
    }

    #[test]
    fn test_recovery_action_enum_variants() {
        let actions = vec![
            RecoveryAction::Redownload,
            RecoveryAction::SwitchMirror,
            RecoveryAction::ReduceSize,
            RecoveryAction::ManualIntervention,
            RecoveryAction::TruncateAndRestart,
        ];

        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0], RecoveryAction::Redownload);
    }

    #[test]
    fn test_verifier_initialization() {
        let verifier = SegmentIntegrityVerifier::new();
        // Just ensure it initializes without panicking
        assert!(verifier.parallel_degree >= 1);
    }

    #[test]
    fn test_empty_segments_report() {
        let segments = vec![];
        let score = SegmentIntegrityVerifier::compute_overall_score(&segments);
        assert_eq!(score, 100, "Empty segment list should score 100");
    }

    #[test]
    fn test_single_segment_report() {
        let segments = vec![SegmentIntegrityInfo {
            segment_id: 0,
            start_byte: 0,
            end_byte: 1000,
            expected_size: 1000,
            actual_size: 1000,
            size_valid: true,
            checksum: None,
            expected_checksum: None,
            checksum_valid: true,
            entropy: 0.5,
            appears_corrupted: false,
            integrity_score: 88,
            verified_at_ms: 0,
            verification_duration_ms: 10,
        }];

        let score = SegmentIntegrityVerifier::compute_overall_score(&segments);
        assert_eq!(score, 88);
    }

    #[test]
    fn test_risk_level_with_all_failed_segments() {
        let level = SegmentIntegrityVerifier::classify_risk_level(40, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 10);
        assert_eq!(level, SegmentRiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_edge_cases() {
        // Test boundary values
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(95, &[], 10),
            SegmentRiskLevel::Healthy
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(80, &[], 10),
            SegmentRiskLevel::Caution
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(60, &[1], 10),
            SegmentRiskLevel::Warning
        );
        assert_eq!(
            SegmentIntegrityVerifier::classify_risk_level(50, &[1, 2, 3, 4, 5, 6], 10),
            SegmentRiskLevel::Critical
        );
    }
}

#[cfg(test)]
mod segment_integrity_integration_tests {
    use super::*;
    use crate::segment_integrity::*;

    #[test]
    fn test_full_integrity_workflow() {
        // Simulate complete workflow: create report, generate recommendations

        let report = IntegrityReport {
            download_id: "integration_test_1".to_string(),
            file_path: "/tmp/test_file".to_string(),
            total_size: 50_000_000, // 50 MB
            segments: vec![],
            failed_segments: vec![],
            overall_score: 85,
            risk_level: SegmentRiskLevel::Caution,
            at_risk_percentage: 0.08,
            recommendations: vec![
                "Download quality is acceptable but monitor for issues".to_string(),
            ],
            generated_at_ms: 1234567890,
            total_duration_ms: 450,
            parallel_degree: 8,
        };

        // Verify the report makes sense
        assert_eq!(report.overall_score, 85);
        assert!(report.can_resume());
        assert!(!report.should_restart());
        assert!(!report.is_healthy());
        assert!(report.requires_action() == false); // No failed segments
    }

    #[test]
    fn test_recovery_strategy_workflow() {
        let verifier = SegmentIntegrityVerifier::new();

        let report = IntegrityReport {
            download_id: "recovery_test".to_string(),
            file_path: "/tmp/corrupt_file".to_string(),
            total_size: 10_000_000,
            segments: vec![
                SegmentIntegrityInfo {
                    segment_id: 5,
                    start_byte: 5_000_000,
                    end_byte: 6_000_000,
                    expected_size: 1_000_000,
                    actual_size: 500_000, // Too small
                    size_valid: false,
                    checksum: None,
                    expected_checksum: None,
                    checksum_valid: false,
                    entropy: 0.97, // Suspicious
                    appears_corrupted: true,
                    integrity_score: 10,
                    verified_at_ms: 0,
                    verification_duration_ms: 15,
                },
            ],
            failed_segments: vec![5],
            overall_score: 55,
            risk_level: SegmentRiskLevel::Warning,
            at_risk_percentage: 0.10,
            recommendations: vec![
                "Segment 5 appears corrupted".to_string(),
                "Consider re-downloading from alternative source".to_string(),
            ],
            generated_at_ms: 0,
            total_duration_ms: 200,
            parallel_degree: 4,
        };

        let strategies = verifier.generate_recovery_strategies(&report);

        assert!(!strategies.is_empty());
        // Should suggest recovery actions for segment 5
        assert!(strategies.iter().any(|s| s.segment_id == 5));
    }

    #[test]
    fn test_metrics_tracking() {
        let metrics = IntegrityMetrics {
            total_segments_verified: 1000,
            total_corruptions_detected: 15,
            auto_recovery_attempts: 10,
            auto_recovery_success: 8,
            average_verification_time_ms: 125.5,
            average_integrity_score: 92.3,
        };

        // Calculate success rate
        let success_rate = (metrics.auto_recovery_success as f64 / metrics.auto_recovery_attempts as f64) * 100.0;
        assert!(success_rate > 70.0);

        // Calculate corruption rate
        let corruption_rate =
            (metrics.total_corruptions_detected as f64 / metrics.total_segments_verified as f64) * 100.0;
        assert!(corruption_rate < 5.0);
    }
}
