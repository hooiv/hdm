use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentState {
    Idle,
    Downloading,
    Paused,
    Complete,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_cursor: u64,
    pub state: SegmentState,
    #[serde(skip)]
    pub speed_bps: u64,
    #[serde(skip)]
    #[allow(dead_code)]
    pub last_update: Option<u64>, // milliseconds since epoch
}

#[allow(dead_code)]
impl Segment {
    pub fn new(id: u32, start: u64, end: u64) -> Self {
        Self {
            id,
            start_byte: start,
            end_byte: end,
            downloaded_cursor: start,
            state: SegmentState::Idle,
            speed_bps: 0,
            last_update: None,
        }
    }

    pub fn len(&self) -> u64 {
        self.end_byte - self.start_byte
    }

    pub fn remaining(&self) -> u64 {
        if self.downloaded_cursor >= self.end_byte {
            0
        } else {
            self.end_byte - self.downloaded_cursor
        }
    }

    pub fn progress(&self) -> f64 {
        let len = self.len();
        if len == 0 {
            return 100.0;
        }
        let downloaded = self.downloaded_cursor - self.start_byte;
        (downloaded as f64 / len as f64) * 100.0
    }

    pub fn is_complete(&self) -> bool {
        self.downloaded_cursor >= self.end_byte
    }

    /// Estimate time to completion in seconds
    pub fn eta_seconds(&self) -> Option<u64> {
        if self.speed_bps == 0 {
            return None;
        }
        let remaining = self.remaining();
        Some(remaining / self.speed_bps)
    }
}

/// Represents a work steal request - a new segment split from an existing one
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StolenWork {
    pub original_segment_id: u32,
    pub new_segment: Segment,
}

/// Configuration for work stealing behavior
#[derive(Debug, Clone)]
pub struct WorkStealConfig {
    /// Minimum bytes remaining before a segment can be split
    pub min_split_size: u64,
    /// How much of the remaining work to steal (0.0-1.0)
    pub steal_ratio: f64,
    /// Minimum speed difference ratio before stealing (e.g., 0.5 = steal if target is 50% slower)
    pub speed_threshold_ratio: f64,
}

impl Default for WorkStealConfig {
    fn default() -> Self {
        Self {
            min_split_size: 1024 * 1024, // 1MB minimum
            steal_ratio: 0.5, // Steal half of remaining work
            speed_threshold_ratio: 0.3, // Target must be 30% or less of stealer's speed
        }
    }
}
