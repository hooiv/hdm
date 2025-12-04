use crate::downloader::structures::{Segment, SegmentState};

pub struct DownloadManager {
    pub _file_size: u64,
    pub segments: Vec<Segment>,
}

impl DownloadManager {
    pub fn new(file_size: u64, parts: u32) -> Self {
        let mut segments = Vec::new();
        let part_size = file_size / parts as u64;

        for i in 0..parts {
            let start = i as u64 * part_size;
            let end = if i == parts - 1 {
                file_size
            } else {
                (i + 1) as u64 * part_size
            };

            segments.push(Segment::new(i, start, end));
        }

        Self {
            _file_size: file_size,
            segments,
        }
    }

    #[allow(dead_code)]
    pub fn get_next_segment(&mut self) -> Option<&mut Segment> {
        self.segments.iter_mut().find(|s| s.state == SegmentState::Idle)
    }
}
