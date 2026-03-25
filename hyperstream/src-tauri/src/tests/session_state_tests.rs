#[cfg(test)]
mod download_state_machine_tests {
    use crate::session_state::*;
    use crate::session_recovery::*;

    #[test]
    fn test_state_machine_pending_to_downloading() {
        let mut state = DownloadStateInfo::new("test_dl_1".to_string(), 10_000_000);
        
        // Initial state should be Pending
        assert_eq!(state.current_state, DownloadState::Pending);
        
        // Transition to Downloading
        let transition = state.transition(DownloadState::Downloading, "User initiated download", 0);
        assert!(transition.is_ok());
        assert_eq!(state.current_state, DownloadState::Downloading);
        assert_eq!(state.total_transitions, 1);
    }

    #[test]
    fn test_state_machine_downloading_to_paused() {
        let mut state = DownloadStateInfo::new("test_dl_2".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Transition to Paused
        let transition = state.transition(DownloadState::Paused, "User paused", 5_000_000);
        assert!(transition.is_ok());
        assert_eq!(state.current_state, DownloadState::Paused);
        assert_eq!(state.downloaded_bytes_at_pause, 5_000_000);
        assert!(state.paused_at.is_some());
    }

    #[test]
    fn test_state_machine_paused_to_downloading() {
        let mut state = DownloadStateInfo::new("test_dl_3".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        state.transition(DownloadState::Paused, "pause", 3_000_000).unwrap();
        
        // Resume from paused
        let transition = state.transition(DownloadState::Downloading, "User resumed", 3_000_000);
        assert!(transition.is_ok());
        assert_eq!(state.current_state, DownloadState::Downloading);
        assert!(state.paused_at.is_none()); // Clear on resume
    }

    #[test]
    fn test_state_machine_downloading_to_completed() {
        let total_size = 10_000_000u64;
        let mut state = DownloadStateInfo::new("test_dl_4".to_string(), total_size);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Complete
        let transition = state.transition(DownloadState::Completed, "All segments downloaded", total_size);
        assert!(transition.is_ok());
        assert_eq!(state.current_state, DownloadState::Completed);
    }

    #[test]
    fn test_state_machine_error_recovery() {
        let mut state = DownloadStateInfo::new("test_dl_5".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Hit an error
        state.record_error("Network timeout").unwrap();
        assert_eq!(state.current_state, DownloadState::Error);
        assert_eq!(state.last_error, Some("Network timeout".to_string()));
        
        // Begin recovery
        state.transition(DownloadState::Recovering, "Auto-recovery initiated", 1_000_000).unwrap();
        assert_eq!(state.recovery_attempts, 1);
        
        // Recovery succeeded - resume
        state.transition(DownloadState::Downloading, "Recovery succeeded", 1_000_000).unwrap();
        assert_eq!(state.current_state, DownloadState::Downloading);
    }

    #[test]
    fn test_invalid_transition_same_state() {
        let state = DownloadStateInfo::new("test_dl_6".to_string(), 10_000_000);
        assert!(!state.can_transition_to(DownloadState::Pending));
    }

    #[test]
    fn test_invalid_transition_pending_to_paused() {
        let state = DownloadStateInfo::new("test_dl_7".to_string(), 10_000_000);
        assert!(!state.can_transition_to(DownloadState::Paused));
    }

    #[test]
    fn test_invalid_transition_pending_to_completed() {
        let state = DownloadStateInfo::new("test_dl_8".to_string(), 10_000_000);
        assert!(!state.can_transition_to(DownloadState::Completed));
    }

    #[test]
    fn test_state_consistency_validation_paused_without_timestamp() {
        let mut state = DownloadStateInfo::new("test_dl_9".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        state.transition(DownloadState::Paused, "pause", 5_000_000).unwrap();
        
        // State should validate successfully
        assert!(state.validate_consistency().is_ok());
        
        // Break it
        state.paused_at = None;
        assert!(state.validate_consistency().is_err());
    }

    #[test]
    fn test_state_consistency_validation_error_without_message() {
        let mut state = DownloadStateInfo::new("test_dl_10".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Force Error state without last_error
        state.current_state = DownloadState::Error;
        
        // Should catch the issue
        assert!(state.validate_consistency().is_err());
    }

    #[test]
    fn test_state_consistency_downloaded_exceeds_total() {
        let mut state = DownloadStateInfo::new("test_dl_11".to_string(), 10_000_000);
        state.downloaded_bytes_at_pause = 15_000_000; // More than total!
        
        // Should fail
        assert!(state.validate_consistency().is_err());
    }

    #[test]
    fn test_excessive_state_churn_detection() {
        let mut state = DownloadStateInfo::new("test_dl_12".to_string(), 10_000_000);
        
        // Manually set very high transition count
        state.total_transitions = 2000;
        
        // Should be caught
        assert!(state.validate_consistency().is_err());
    }

    #[test]
    fn test_state_age_calculation() {
        let state = DownloadStateInfo::new("test_dl_13".to_string(), 10_000_000);
        let age = state.state_age_secs();
        
        // Should be very close to 0 (a few milliseconds old)
        assert!(age < 5);
    }

    #[test]
    fn test_diagnostic_string_generation() {
        let mut state = DownloadStateInfo::new("test_dl_14".to_string(), 10_000_000);
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        let diag = state.to_diagnostic_string();
        assert!(diag.contains("test_dl_14"));
        assert!(diag.contains("Downloading"));
        assert!(diag.contains("Transitions=1"));
    }

    #[test]
    fn test_transition_guard_committed() {
        let mut guard = StateTransitionGuard::new("test_dl_15".to_string(), DownloadState::Pending);
        assert!(!guard.is_committed());
        
        guard.commit();
        assert!(guard.is_committed());
    }

    #[test]
    fn test_transition_guard_uncommitted_warning() {
        // This would print a warning on drop
        let _guard = StateTransitionGuard::new("test_dl_16".to_string(), DownloadState::Downloading);
        // When this drops, should emit warning (test output)
    }

    #[test]
    fn test_complex_state_machine_flow() {
        // Simulate a realistic download lifecycle
        let mut state = DownloadStateInfo::new("complex_dl".to_string(), 100_000_000);
        
        // Start
        state.transition(DownloadState::Downloading, "Begin download", 0).unwrap();
        assert_eq!(state.current_state, DownloadState::Downloading);
        assert_eq!(state.total_transitions, 1);
        
        // Pause
        state.transition(DownloadState::Paused, "User pause #1", 25_000_000).unwrap();
        assert_eq!(state.total_transitions, 2);
        
        // Resume
        state.transition(DownloadState::Downloading, "User resume #1", 25_000_000).unwrap();
        assert_eq!(state.total_transitions, 3);
        
        // Error
        state.record_error("Network connection dead").unwrap();
        assert_eq!(state.current_state, DownloadState::Error);
        assert_eq!(state.total_transitions, 4);
        
        // Recovery
        state.transition(DownloadState::Recovering, "Auto-retry", 25_000_000).unwrap();
        assert_eq!(state.recovery_attempts, 1);
        
        // Recovery succeeded
        state.transition(DownloadState::Downloading, "Recovery OK", 25_000_000).unwrap();
        
        // Complete
        state.transition(DownloadState::Completed, "Finished", 100_000_000).unwrap();
        
        // Verify final state
        assert_eq!(state.current_state, DownloadState::Completed);
        assert!(state.validate_consistency().is_ok());
        assert!(state.total_transitions >= 6);
    }

    #[test]
    fn test_resume_validity_report_safe() {
        let report = ResumeValidityReport {
            download_id: "test".to_string(),
            level: crate::session_recovery::ValidationLevel::Safe,
            checks_passed: vec!["File exists".to_string()],
            checks_warning: vec![],
            checks_failed: vec![],
            recommendation: "Resume".to_string(),
            suggested_retry_delay_secs: None,
            should_restart_from_scratch: false,
            summary: "Safe to resume".to_string(),
        };
        
        assert!(report.can_resume());
        assert!(!report.requires_confirmation());
        assert!(!report.cannot_resume());
    }

    #[test]
    fn test_resume_validity_report_caution() {
        let report = ResumeValidityReport {
            download_id: "test".to_string(),
            level: crate::session_recovery::ValidationLevel::Caution,
            checks_passed: vec!["File exists".to_string()],
            checks_warning: vec!["File is old".to_string()],
            checks_failed: vec![],
            recommendation: "May resume with monitoring".to_string(),
            suggested_retry_delay_secs: Some(30),
            should_restart_from_scratch: false,
            summary: "Caution advised".to_string(),
        };
        
        assert!(report.can_resume());
        assert!(!report.requires_confirmation());
        assert!(!report.cannot_resume());
    }

    #[test]
    fn test_resume_validity_report_warning() {
        let report = ResumeValidityReport {
            download_id: "test".to_string(),
            level: crate::session_recovery::ValidationLevel::Warning,
            checks_passed: vec![],
            checks_warning: vec!["URL returns 403".to_string()],
            checks_failed: vec![],
            recommendation: "Confirm before resuming".to_string(),
            suggested_retry_delay_secs: None,
            should_restart_from_scratch: false,
            summary: "Requires confirmation".to_string(),
        };
        
        assert!(!report.can_resume());
        assert!(report.requires_confirmation());
        assert!(!report.cannot_resume());
    }

    #[test]
    fn test_resume_validity_report_blocked() {
        let report = ResumeValidityReport {
            download_id: "test".to_string(),
            level: crate::session_recovery::ValidationLevel::Blocked,
            checks_passed: vec![],
            checks_warning: vec![],
            checks_failed: vec!["File not found".to_string()],
            recommendation: "Restart from scratch".to_string(),
            suggested_retry_delay_secs: None,
            should_restart_from_scratch: true,
            summary: "Cannot resume".to_string(),
        };
        
        assert!(!report.can_resume());
        assert!(!report.requires_confirmation());
        assert!(report.cannot_resume());
    }

    #[test]
    fn test_all_valid_state_transitions() {
        // Test matrix of all valid transitions
        let valid_transitions = vec![
            (DownloadState::Pending, DownloadState::Downloading),
            (DownloadState::Pending, DownloadState::Error),
            (DownloadState::Downloading, DownloadState::Paused),
            (DownloadState::Downloading, DownloadState::Completed),
            (DownloadState::Downloading, DownloadState::Error),
            (DownloadState::Downloading, DownloadState::Recovering),
            (DownloadState::Paused, DownloadState::Downloading),
            (DownloadState::Paused, DownloadState::Error),
            (DownloadState::Paused, DownloadState::Recovering),
            (DownloadState::Paused, DownloadState::Completed),
            (DownloadState::Error, DownloadState::Recovering),
            (DownloadState::Error, DownloadState::Pending),
            (DownloadState::Recovering, DownloadState::Downloading),
            (DownloadState::Recovering, DownloadState::Error),
            (DownloadState::Recovering, DownloadState::Paused),
        ];
        
        for (from, to) in valid_transitions {
            let mut state = DownloadStateInfo::new("test".to_string(), 1000);
            
            // Navigate to `from` state
            match from {
                DownloadState::Pending => {}, // Already pending
                DownloadState::Downloading => {
                    state.transition(DownloadState::Downloading, "test", 0).unwrap();
                },
                DownloadState::Paused => {
                    state.transition(DownloadState::Downloading, "test", 0).unwrap();
                    state.transition(DownloadState::Paused, "test", 500).unwrap();
                },
                DownloadState::Error => {
                    state.transition(DownloadState::Downloading, "test", 0).unwrap();
                    state.current_state = DownloadState::Error;
                    state.last_error = Some("test".to_string());
                },
                DownloadState::Recovering => {
                    state.transition(DownloadState::Downloading, "test", 0).unwrap();
                    state.current_state = DownloadState::Error;
                    state.last_error = Some("test".to_string());
                    state.transition(DownloadState::Recovering, "test", 0).unwrap();
                },
                DownloadState::Completed => {},
            }
            
            // Now test transition to `to`
            assert!(state.can_transition_to(to), "Cannot transition from {} to {}", from, to);
        }
    }

    #[test]
    fn test_production_scenario_interrupted_download() {
        // Simulates: Download interrupted by system crash, needs recovery
        let mut state = DownloadStateInfo::new("crashed_dl".to_string(), 1_000_000_000);
        
        // Was downloading
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Got paused (by crash recovery)
        state.transition(DownloadState::Paused, "crash recovery", 250_000_000).unwrap();
        
        // Now being resumed by user
        let result = state.transition(DownloadState::Downloading, "user resume after crash", 250_000_000);
        
        assert!(result.is_ok());
        assert_eq!(state.current_state, DownloadState::Downloading);
        assert_eq!(state.downloaded_bytes_at_pause, 250_000_000);
    }

    #[test]
    fn test_production_scenario_permanent_failure() {
        let mut state = DownloadStateInfo::new("failed_dl".to_string(), 1_000_000_000);
        
        state.transition(DownloadState::Downloading, "start", 0).unwrap();
        
        // Attempt 1: Error
        state.record_error("Server connection refused").unwrap();
        assert_eq!(state.current_state, DownloadState::Error);
        
        // Attempt 2: Recovery
        state.transition(DownloadState::Recovering, "retry attempt 1", 0).unwrap();
        assert_eq!(state.recovery_attempts, 1);
        
        // Recovery failed again
        state.record_error("Server still refusing").unwrap();
        assert_eq!(state.recovery_attempts, 1); // Still 1, didn't increment on record_error
        
        // Attempt 3: Another recovery?  
        state.transition(DownloadState::Recovering, "retry attempt 2", 0).unwrap();
        assert_eq!(state.recovery_attempts, 2);
        
        // Give up
        let final_err = state.transition(DownloadState::Error, "Final failure - giving up", 0);
        assert!(final_err.is_ok());
        
        // Verify state is locked in Error
        assert_eq!(state.current_state, DownloadState::Error);
        assert_eq!(state.recovery_attempts, 2);
    }
}
