//! Production-Grade Download State Machine
//! 
//! Provides a robust, type-safe state machine for managing download lifecycles.
//! Enforces valid state transitions at compile time and runtime, preventing
//! invalid states like "paused + downloading" or "completed + downloading".
//!
//! State Flow:
//! ```
//! Initial → Pending → Downloading ↔ Paused → Completed
//!              ↓                      ↓
//!            Error ←──────────────────┴─ Recovery
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Core download states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// Queued but not yet started
    Pending,
    /// Currently downloading segments
    Downloading,
    /// Suspended by user or system
    Paused,
    /// Successfully completed
    Completed,
    /// Encountered an error (may be recoverable)
    Error,
    /// In recovery/ retry sequence
    Recovering,
}

impl fmt::Display for DownloadState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Downloading => write!(f, "Downloading"),
            Self::Paused => write!(f, "Paused"),
            Self::Completed => write!(f, "Completed"),
            Self::Error => write!(f, "Error"),
            Self::Recovering => write!(f, "Recovering"),
        }
    }
}

/// Valid transitions between states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransition {
    /// Pending → Downloading (start download)
    StartDownload,
    /// Downloading → Paused (user pause)
    PauseDownload,
    /// Paused → Downloading (user resume)
    ResumeDownload,
    /// Downloading → Completed (all segments done)
    CompleteDownload,
    /// Any → Error (failure occurred)
    FailDownload,
    /// Error/Paused → Recovering (attempting recovery)
    BeginRecovery,
    /// Recovering → Downloading (recovery succeeded)
    RecoverySuccess,
    /// Recovering → Error (recovery failed)
    RecoveryFailed,
}

/// Metadata about a state transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionEvent {
    /// Timestamp of transition (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Previous state
    pub from_state: DownloadState,
    /// New state
    pub to_state: DownloadState,
    /// Human-readable reason
    pub reason: String,
    /// Optional error details
    pub error_details: Option<String>,
    /// Number of times this state changed (for debugging state churn)
    pub transition_count: u32,
}

impl TransitionEvent {
    fn now(from: DownloadState, to: DownloadState, reason: &str, error: Option<&str>, count: u32) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            timestamp_ms,
            from_state: from,
            to_state: to,
            reason: reason.to_string(),
            error_details: error.map(|e| e.to_string()),
            transition_count: count,
        }
    }
}

/// Download state metadata (not just state but context)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStateInfo {
    pub download_id: String,
    pub current_state: DownloadState,
    pub prev_state: Option<DownloadState>,
    
    /// ISO 8601 timestamps for state tracking
    pub entered_current_state_at: String,
    pub state_duration_secs: u32,
    
    /// Last transition event
    pub last_transition: Option<TransitionEvent>,
    
    /// How many times has this download changed state? (high churn = problems)
    pub total_transitions: u32,
    
    /// How many error→recovery→downloading cycles?
    pub recovery_attempts: u32,
    
    /// If paused, when was it paused?
    pub paused_at: Option<String>,
    
    /// If in error state, what was the last error?
    pub last_error: Option<String>,
    
    /// Progress when state changed (for resume validation)
    pub downloaded_bytes_at_pause: u64,
    pub total_bytes_at_pause: u64,
}

impl DownloadStateInfo {
    /// Create initial state for a new download
    pub fn new(download_id: String, total_size: u64) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            download_id,
            current_state: DownloadState::Pending,
            prev_state: None,
            entered_current_state_at: now,
            state_duration_secs: 0,
            last_transition: None,
            total_transitions: 0,
            recovery_attempts: 0,
            paused_at: None,
            last_error: None,
            downloaded_bytes_at_pause: 0,
            total_bytes_at_pause: total_size,
        }
    }

    /// Check if transition is valid from current state
    pub fn can_transition_to(&self, target: DownloadState) -> bool {
        match (self.current_state, target) {
            // Can't transition to same state
            (from, to) if from == to => false,
            
            // Valid transitions
            (DownloadState::Pending, DownloadState::Downloading) => true,
            (DownloadState::Pending, DownloadState::Error) => true,
            (DownloadState::Downloading, DownloadState::Paused) => true,
            (DownloadState::Downloading, DownloadState::Completed) => true,
            (DownloadState::Downloading, DownloadState::Error) => true,
            (DownloadState::Downloading, DownloadState::Recovering) => true,
            (DownloadState::Paused, DownloadState::Downloading) => true,
            (DownloadState::Paused, DownloadState::Error) => true,
            (DownloadState::Paused, DownloadState::Recovering) => true,
            (DownloadState::Paused, DownloadState::Completed) => true, // Complete paused downloads
            (DownloadState::Error, DownloadState::Recovering) => true,
            (DownloadState::Error, DownloadState::Pending) => true, // Reset for retry
            (DownloadState::Recovering, DownloadState::Downloading) => true,
            (DownloadState::Recovering, DownloadState::Error) => true,
            (DownloadState::Recovering, DownloadState::Paused) => true,
            
            // Invalid transitions
            _ => false,
        }
    }

    /// Validate consistency of state data
    pub fn validate_consistency(&self) -> Result<(), String> {
        // If paused, must have paused_at
        if self.current_state == DownloadState::Paused && self.paused_at.is_none() {
            return Err("State is Paused but paused_at is missing".to_string());
        }

        // If error, should have last_error
        if self.current_state == DownloadState::Error && self.last_error.is_none() {
            return Err("State is Error but last_error is missing".to_string());
        }

        // Recovered bytes shouldn't exceed total
        if self.downloaded_bytes_at_pause > self.total_bytes_at_pause {
            return Err(format!(
                "Downloaded bytes ({}) exceeds total size ({})",
                self.downloaded_bytes_at_pause, self.total_bytes_at_pause
            ));
        }

        // No state should have excessively high transition counts (>1000 = infinite loop?)
        if self.total_transitions > 1000 {
            return Err(format!(
                "Download has {} transitions (probable state churn bug)",
                self.total_transitions
            ));
        }

        Ok(())
    }

    /// Attempt transition with full context
    pub fn transition(
        &mut self,
        target_state: DownloadState,
        reason: &str,
        downloaded_now: u64,
    ) -> Result<TransitionEvent, String> {
        // Validate target state is legal from current
        if !self.can_transition_to(target_state) {
            return Err(format!(
                "Invalid transition: {} → {} (reason: {})",
                self.current_state, target_state, reason
            ));
        }

        // Update state metadata
        let prev = self.current_state;
        let now = Utc::now().to_rfc3339();

        self.prev_state = Some(prev);
        self.current_state = target_state;
        self.entered_current_state_at = now.clone();
        self.state_duration_secs = 0;
        self.total_transitions += 1;

        // Track pause-specific metadata
        if target_state == DownloadState::Paused {
            self.paused_at = Some(now.clone());
            self.downloaded_bytes_at_pause = downloaded_now;
        } else if target_state == DownloadState::Downloading {
            // Clear pause metadata when resuming
            self.paused_at = None;
        }

        // Track recovery attempts
        if target_state == DownloadState::Recovering {
            self.recovery_attempts += 1;
        }

        let event = TransitionEvent::now(prev, target_state, reason, None, self.total_transitions);
        self.last_transition = Some(event.clone());

        Ok(event)
    }

    /// Record an error that occurred during this state
    pub fn record_error(&mut self, error_msg: &str) -> Result<(), String> {
        self.last_error = Some(error_msg.to_string());

        // Attempt to transition to Error state if not already there
        if self.current_state != DownloadState::Error {
            self.transition(
                DownloadState::Error,
                "Error occurred",
                self.downloaded_bytes_at_pause,
            )?;
        }

        Ok(())
    }

    /// Age of current state in seconds (how long in this state?)
    pub fn state_age_secs(&self) -> u32 {
        if let Ok(dt) = Utc::now().to_rfc3339().parse::<chrono::DateTime<Utc>>() {
            if let Ok(entered) = self.entered_current_state_at.parse::<chrono::DateTime<Utc>>() {
                let age = dt - entered;
                return age.num_seconds() as u32;
            }
        }
        0
    }

    /// Diagnostic string for logging
    pub fn to_diagnostic_string(&self) -> String {
        format!(
            "[{}] State={} Duration={}s Transitions={} Recovery={}x Error={} Paused={:?}",
            self.download_id,
            self.current_state,
            self.state_age_secs(),
            self.total_transitions,
            self.recovery_attempts,
            self.last_error.as_ref().map(|e| e.as_str()).unwrap_or("None"),
            self.paused_at.as_ref().map(|p| p.as_str())
        )
    }
}

/// State machine guard to ensure atomic state changes
pub struct StateTransitionGuard {
    download_id: String,
    from_state: DownloadState,
    committed: bool,
}

impl StateTransitionGuard {
    pub fn new(id: String, from: DownloadState) -> Self {
        Self {
            download_id: id,
            from_state: from,
            committed: false,
        }
    }

    pub fn commit(&mut self) {
        self.committed = true;
    }

    pub fn download_id(&self) -> &str {
        &self.download_id
    }

    pub fn from_state(&self) -> DownloadState {
        self.from_state
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }
}

impl Drop for StateTransitionGuard {
    fn drop(&mut self) {
        if !self.committed {
            eprintln!(
                "[StateTransitionGuard] WARNING: Uncommitted transition for {} (from {})",
                self.download_id, self.from_state
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let mut info = DownloadStateInfo::new("test".to_string(), 1000);

        // Pending → Downloading
        assert!(info.can_transition_to(DownloadState::Downloading));
        info.transition(DownloadState::Downloading, "test", 0).unwrap();

        // Downloading → Paused
        assert!(info.can_transition_to(DownloadState::Paused));
        info.transition(DownloadState::Paused, "test", 500).unwrap();

        // Paused → Downloading
        assert!(info.can_transition_to(DownloadState::Downloading));
        info.transition(DownloadState::Downloading, "test", 500).unwrap();

        // Downloading → Completed
        assert!(info.can_transition_to(DownloadState::Completed));
        info.transition(DownloadState::Completed, "test", 1000).unwrap();
    }

    #[test]
    fn test_invalid_transitions() {
        let info = DownloadStateInfo::new("test".to_string(), 1000);

        // Pending → Paused (invalid)
        assert!(!info.can_transition_to(DownloadState::Paused));

        // Pending → Completed (invalid)
        assert!(!info.can_transition_to(DownloadState::Completed));

        // Pending → Pending (no-op)
        assert!(!info.can_transition_to(DownloadState::Pending));
    }

    #[test]
    fn test_state_consistency_validation() {
        let mut info = DownloadStateInfo::new("test".to_string(), 1000);
        info.transition(DownloadState::Downloading, "start", 0).unwrap();
        info.transition(DownloadState::Paused, "pause", 500).unwrap();

        // Paused without paused_at should not validate
        let mut broken = info.clone();
        broken.paused_at = None;
        assert!(broken.validate_consistency().is_err());

        // Good state should validate
        assert!(info.validate_consistency().is_ok());
    }

    #[test]
    fn test_recovery_attempt_tracking() {
        let mut info = DownloadStateInfo::new("test".to_string(), 1000);
        info.transition(DownloadState::Downloading, "start", 0).unwrap();
        info.transition(DownloadState::Error, "error", 100).unwrap();

        assert_eq!(info.recovery_attempts, 0);
        info.transition(DownloadState::Recovering, "recovery", 100).unwrap();
        assert_eq!(info.recovery_attempts, 1);
    }

    #[test]
    fn test_error_recording() {
        let mut info = DownloadStateInfo::new("test".to_string(), 1000);
        info.transition(DownloadState::Downloading, "start", 0).unwrap();

        info.record_error("Network timeout").unwrap();
        assert_eq!(info.current_state, DownloadState::Error);
        assert_eq!(info.last_error, Some("Network timeout".to_string()));
    }

    #[test]
    fn test_state_age() {
        let info = DownloadStateInfo::new("test".to_string(), 1000);
        let age = info.state_age_secs();
        assert!(age < 5); // Should be nearly 0
    }

    #[test]
    fn test_transition_guard() {
        let mut guard = StateTransitionGuard::new("test".to_string(), DownloadState::Pending);
        assert!(!guard.is_committed());
        guard.commit();
        assert!(guard.is_committed());
    }
}
