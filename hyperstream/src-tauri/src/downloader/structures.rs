use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentState {
    Idle,
    Downloading,
    Paused,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_cursor: u64,
    pub state: SegmentState,
}

impl Segment {
    pub fn new(id: u32, start: u64, end: u64) -> Self {
        Self {
            id,
            start_byte: start,
            end_byte: end,
            downloaded_cursor: start,
            state: SegmentState::Idle,
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> u64 {
        self.end_byte - self.start_byte
    }

    #[allow(dead_code)]
    pub fn remaining(&self) -> u64 {
        self.end_byte - self.downloaded_cursor
    }
}
