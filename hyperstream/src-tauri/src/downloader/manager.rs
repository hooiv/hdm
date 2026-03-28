use crate::downloader::structures::{Segment, SegmentState, StolenWork, WorkStealConfig};
use std::sync::{Arc, RwLock};

/// Thread-safe Download Manager with Work Stealing support
pub struct DownloadManager {
    pub file_size: u64,
    pub segments: Arc<RwLock<Vec<Segment>>>,
    pub config: WorkStealConfig,
    next_segment_id: Arc<RwLock<u32>>,
}

#[allow(dead_code)]
impl DownloadManager {
    pub fn new(file_size: u64, parts: u32) -> Self {
        Self::with_config(file_size, parts, WorkStealConfig::default())
    }

    pub fn with_config(file_size: u64, parts: u32, config: WorkStealConfig) -> Self {
        let parts = if parts == 0 { 1 } else { parts };
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
            file_size,
            segments: Arc::new(RwLock::new(segments)),
            config,
            next_segment_id: Arc::new(RwLock::new(parts)),
        }
    }

    pub fn new_with_segments(file_size: u64, segments: Vec<Segment>) -> Self {
        // Find max ID to initialize next_segment_id correctly
        let max_id = segments.iter().map(|s| s.id).max().unwrap_or(0);
        
        Self {
            file_size,
            segments: Arc::new(RwLock::new(segments)),
            config: WorkStealConfig::default(),
            next_segment_id: Arc::new(RwLock::new(max_id + 1)),
        }
    }

    /// Get the next idle segment to download
    pub fn get_next_segment(&self) -> Option<Segment> {
        let segments = self.segments.read().ok()?;
        segments.iter()
            .find(|s| s.state == SegmentState::Idle)
            .cloned()
    }

    /// Mark a segment as downloading
    pub fn start_segment(&self, segment_id: u32) -> bool {
        if let Ok(mut segments) = self.segments.write() {
            if let Some(seg) = segments.iter_mut().find(|s| s.id == segment_id) {
                seg.state = SegmentState::Downloading;
                return true;
            }
        }
        false
    }

    /// Update segment progress
    pub fn update_progress(&self, segment_id: u32, cursor: u64, speed_bps: u64) {
        if let Ok(mut segments) = self.segments.write() {
            if let Some(seg) = segments.iter_mut().find(|s| s.id == segment_id) {
                seg.downloaded_cursor = cursor;
                seg.speed_bps = speed_bps;
                seg.last_update = Some(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0));
                
                // Auto-complete if reached end
                if seg.downloaded_cursor >= seg.end_byte {
                    seg.state = SegmentState::Complete;
                }
            }
        }
    }

    /// Mark segment as complete
    pub fn complete_segment(&self, segment_id: u32) {
        if let Ok(mut segments) = self.segments.write() {
            if let Some(seg) = segments.iter_mut().find(|s| s.id == segment_id) {
                seg.state = SegmentState::Complete;
                seg.downloaded_cursor = seg.end_byte;
            }
        }
    }

    /// Mark segment as paused
    pub fn pause_segment(&self, segment_id: u32) {
        if let Ok(mut segments) = self.segments.write() {
            if let Some(seg) = segments.iter_mut().find(|s| s.id == segment_id) {
                seg.state = SegmentState::Paused;
                seg.speed_bps = 0;
            }
        }
    }

    /// Mark segment as errored
    pub fn error_segment(&self, segment_id: u32) {
        if let Ok(mut segments) = self.segments.write() {
            if let Some(seg) = segments.iter_mut().find(|s| s.id == segment_id) {
                seg.state = SegmentState::Error;
                seg.speed_bps = 0;
            }
        }
    }

    /// Get total download progress (0.0 - 100.0)
    pub fn total_progress(&self) -> f64 {
        if self.file_size == 0 {
            return 0.0;
        }
        if let Ok(segments) = self.segments.read() {
            let total_downloaded: u64 = segments.iter()
                .map(|s| s.downloaded_cursor.saturating_sub(s.start_byte))
                .sum();
            ((total_downloaded as f64 / self.file_size as f64) * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    /// Get total downloaded bytes
    pub fn total_downloaded(&self) -> u64 {
        if let Ok(segments) = self.segments.read() {
            segments.iter()
                .map(|s| s.downloaded_cursor.saturating_sub(s.start_byte))
                .sum()
        } else {
            0
        }
    }

    /// Get aggregate speed in bytes per second
    pub fn total_speed(&self) -> u64 {
        if let Ok(segments) = self.segments.read() {
            segments.iter()
                .filter(|s| s.state == SegmentState::Downloading)
                .map(|s| s.speed_bps)
                .sum()
        } else {
            0
        }
    }

    /// Check if all segments are complete
    pub fn is_complete(&self) -> bool {
        if let Ok(segments) = self.segments.read() {
            segments.iter().all(|s| s.state == SegmentState::Complete)
        } else {
            false
        }
    }

    /// **THE CORE WORK STEALING ALGORITHM**
    /// Called when a segment completes. Returns work stolen from a slower segment.
    pub fn on_segment_complete(&self, completed_segment_id: u32) -> Option<StolenWork> {
        let mut segments = self.segments.write().ok()?;
        
        // Mark the completed segment
        if let Some(seg) = segments.iter_mut().find(|s| s.id == completed_segment_id) {
            seg.state = SegmentState::Complete;
            seg.speed_bps = 0;
        }

        // Find the segment with the most remaining work that is currently downloading
        let target_idx = segments.iter()
            .enumerate()
            .filter(|(_, s)| s.state == SegmentState::Downloading)
            .filter(|(_, s)| s.remaining() >= self.config.min_split_size * 2)
            .max_by_key(|(_, s)| s.remaining())
            .map(|(i, _)| i);

        let target_idx = target_idx?;
        let target = &segments[target_idx];
        
        // Check if there's enough work to steal
        let remaining = target.remaining();
        if remaining < self.config.min_split_size * 2 {
            return None;
        }

        // Calculate split point - steal the second half
        let steal_bytes = (remaining as f64 * self.config.steal_ratio) as u64;
        let split_point = target.end_byte - steal_bytes;

        // Ensure split point is aligned and valid
        if split_point <= target.downloaded_cursor {
            return None;
        }

        // Generate new segment ID
        let new_id = {
            let mut id_lock = self.next_segment_id.write().ok()?;
            let id = *id_lock;
            *id_lock += 1;
            id
        };

        // Create the stolen segment
        let mut new_segment = Segment::new(new_id, split_point, target.end_byte);
        new_segment.state = SegmentState::Downloading;
        
        // Shrink the target's responsibility
        let target = &mut segments[target_idx];
        let original_end = target.end_byte;
        target.end_byte = split_point;

        println!("[WorkSteal] Segment {} stole {} bytes from segment {} (new range: {}-{})",
            new_id, steal_bytes, target.id, split_point, original_end);

        let stolen = StolenWork {
            original_segment_id: target.id,
            new_segment: new_segment.clone(),
        };

        // Register stolen segment in the manager for progress tracking
        segments.push(new_segment);

        Some(stolen)
    }

    /// Try to steal work without completing a segment (proactive stealing)
    /// Used when a thread is idle and wants work
    pub fn steal_work(&self) -> Option<StolenWork> {
        let mut segments = self.segments.write().ok()?;
        
        // Find the slowest active segment with enough work to split
        let target_idx = segments.iter()
            .enumerate()
            .filter(|(_, s)| s.state == SegmentState::Downloading)
            .filter(|(_, s)| s.remaining() >= self.config.min_split_size * 2)
            .min_by_key(|(_, s)| s.speed_bps) // Find slowest
            .map(|(i, _)| i);

        let target_idx = target_idx?;
        let target = &segments[target_idx];
        
        let remaining = target.remaining();
        if remaining < self.config.min_split_size * 2 {
            return None;
        }

        let steal_bytes = (remaining as f64 * self.config.steal_ratio) as u64;
        let split_point = target.end_byte - steal_bytes;

        if split_point <= target.downloaded_cursor {
            return None;
        }

        let new_id = {
            let mut id_lock = self.next_segment_id.write().ok()?;
            let id = *id_lock;
            *id_lock += 1;
            id
        };

        let mut new_segment = Segment::new(new_id, split_point, target.end_byte);
        new_segment.state = SegmentState::Downloading;
        
        let target = &mut segments[target_idx];
        target.end_byte = split_point;

        let stolen = StolenWork {
            original_segment_id: target.id,
            new_segment: new_segment.clone(),
        };

        // Register stolen segment in the manager for progress tracking
        segments.push(new_segment);

        Some(stolen)
    }

    /// Number of segments currently in `Downloading` state.
    pub fn active_segment_count(&self) -> usize {
        self.segments
            .read()
            .map(|segs| segs.iter().filter(|s| s.state == SegmentState::Downloading).count())
            .unwrap_or(0)
    }

    /// Average speed (bytes/sec) across all active (Downloading) segments.
    /// Returns 0 if no segments are active.
    pub fn average_active_speed(&self) -> u64 {
        if let Ok(segs) = self.segments.read() {
            let active: Vec<_> = segs
                .iter()
                .filter(|s| s.state == SegmentState::Downloading)
                .collect();
            if active.is_empty() {
                return 0;
            }
            let total_speed: u64 = active.iter().map(|s| s.speed_bps).sum();
            total_speed / active.len() as u64
        } else {
            0
        }
    }

    /// **PROACTIVE DYNAMIC SEGMENT SPLITTING**
    ///
    /// Analyzes all active segments and splits those running significantly
    /// slower than the average. This is the IDM-style feature that dynamically
    /// adds connections mid-download to maximize throughput.
    ///
    /// Only splits when:
    /// - Current active count < `max_active`
    /// - The segment's speed is < 50% of average active speed
    /// - The segment has enough remaining bytes (> `min_split_size * 2`)
    ///
    /// Returns a vec of `StolenWork` — one per split performed. The caller
    /// should spawn a new worker for each.
    pub fn find_splittable_segments(&self, max_active: u32) -> Vec<StolenWork> {
        let mut segments = match self.segments.write() {
            Ok(s) => s,
            Err(e) => e.into_inner(),
        };

        let active_count = segments
            .iter()
            .filter(|s| s.state == SegmentState::Downloading)
            .count() as u32;

        if active_count >= max_active {
            return Vec::new();
        }

        // Calculate average speed across active segments
        let active_speeds: Vec<(usize, u64, u64)> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.state == SegmentState::Downloading)
            .filter(|(_, s)| s.remaining() >= self.config.min_split_size * 2)
            .map(|(i, s)| (i, s.speed_bps, s.remaining()))
            .collect();

        if active_speeds.is_empty() {
            return Vec::new();
        }

        let avg_speed: u64 = {
            let total: u64 = active_speeds.iter().map(|(_, spd, _)| spd).sum();
            total / active_speeds.len() as u64
        };

        // Don't split if average speed is 0 (download just started, no data yet)
        if avg_speed == 0 {
            return Vec::new();
        }

        // Find segments running below 50% of average speed, sorted by most remaining bytes
        let slow_threshold = avg_speed / 2;
        let mut candidates: Vec<usize> = active_speeds
            .iter()
            .filter(|(_, spd, _)| *spd < slow_threshold)
            .map(|(idx, _, _)| *idx)
            .collect();

        // Sort by most remaining bytes descending (split the largest slow segments first)
        candidates.sort_by(|a, b| {
            segments[*b].remaining().cmp(&segments[*a].remaining())
        });

        let mut splits_available = (max_active - active_count) as usize;
        let mut results = Vec::new();

        for target_idx in candidates {
            if splits_available == 0 {
                break;
            }

            let remaining = segments[target_idx].remaining();
            if remaining < self.config.min_split_size * 2 {
                continue;
            }

            let steal_bytes = (remaining as f64 * self.config.steal_ratio) as u64;
            let split_point = segments[target_idx].end_byte - steal_bytes;

            if split_point <= segments[target_idx].downloaded_cursor {
                continue;
            }

            // Generate new segment ID
            let new_id = {
                let mut id_lock = match self.next_segment_id.write() {
                    Ok(l) => l,
                    Err(e) => e.into_inner(),
                };
                let id = *id_lock;
                *id_lock += 1;
                id
            };

            let original_end = segments[target_idx].end_byte;
            let original_id = segments[target_idx].id;

            // Create the new segment for the upper half
            let mut new_segment = Segment::new(new_id, split_point, original_end);
            new_segment.state = SegmentState::Downloading;

            // Shrink the original segment
            segments[target_idx].end_byte = split_point;

            println!(
                "[DynamicSplit] Split segment {} at byte {} → new segment {} ({}-{}), remaining: {} bytes",
                original_id, split_point, new_id, split_point, original_end,
                original_end - split_point
            );

            let stolen = StolenWork {
                original_segment_id: original_id,
                new_segment: new_segment.clone(),
            };

            segments.push(new_segment);
            results.push(stolen);
            splits_available -= 1;
        }

        results
    }

    /// FORCED PROACTIVE SPLIT: Specifically for straggler/stalled handling.
    /// Splits the target segment immediately and returns the new segment.
    /// The new segment is initialized to `Idle` state so a new mirror can pick it up.
    pub fn trigger_proactive_split(&self, target_segment_id: u32) -> Option<Segment> {
        let mut segments = match self.segments.write() {
            Ok(s) => s,
            Err(e) => e.into_inner(),
        };

        // Find target segment by ID
        let target_idx = segments.iter().position(|s| s.id == target_segment_id)?;

        // Validate state and size
        if segments[target_idx].state != SegmentState::Downloading || 
           segments[target_idx].remaining() < self.config.min_split_size * 2 {
            return None;
        }

        // Calculate split point (steal the 2nd half)
        let remaining = segments[target_idx].remaining();
        let mid = segments[target_idx].downloaded_cursor + (remaining / 2);

        // Generate new ID
        let new_id = {
            let mut id_lock = match self.next_segment_id.write() {
                Ok(l) => l,
                Err(e) => e.into_inner(),
            };
            let id = *id_lock;
            *id_lock += 1;
            id
        };

        let original_end = segments[target_idx].end_byte;
        let original_id = segments[target_idx].id;

        // Create the new segment for the upper half (Starts as Idle)
        let mut new_segment = Segment::new(new_id, mid, original_end);
        new_segment.state = SegmentState::Idle; 

        // Shrink the original segment
        segments[target_idx].end_byte = mid;

        println!(
            "[ProactiveSplit] Forced split of straggler {} at byte {} → new idle segment {} ({}-{})",
            original_id, mid, new_id, mid, original_end
        );

        let result = new_segment.clone();
        segments.push(new_segment);
        
        Some(result)
    }

    /// Get a snapshot of all segments for UI display
    pub fn get_segments_snapshot(&self) -> Vec<Segment> {
        self.segments.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Get stats summary
    pub fn get_stats(&self) -> DownloadStats {
        if let Ok(segments) = self.segments.read() {
            let active = segments.iter().filter(|s| s.state == SegmentState::Downloading).count();
            let complete = segments.iter().filter(|s| s.state == SegmentState::Complete).count();
            let total_speed: u64 = segments.iter()
                .filter(|s| s.state == SegmentState::Downloading)
                .map(|s| s.speed_bps)
                .sum();
            let downloaded: u64 = segments.iter()
                .map(|s| s.downloaded_cursor.saturating_sub(s.start_byte))
                .sum();
            
            DownloadStats {
                total_segments: segments.len(),
                active_segments: active,
                complete_segments: complete,
                total_speed_bps: total_speed,
                downloaded_bytes: downloaded,
                total_bytes: self.file_size,
                progress_percent: if self.file_size > 0 {
                    ((downloaded as f64 / self.file_size as f64) * 100.0).min(100.0)
                } else {
                    0.0
                },
            }
        } else {
            DownloadStats::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DownloadStats {
    pub total_segments: usize,
    pub active_segments: usize,
    pub complete_segments: usize,
    pub total_speed_bps: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress_percent: f64,
}

impl Clone for DownloadManager {
    fn clone(&self) -> Self {
        Self {
            file_size: self.file_size,
            segments: Arc::clone(&self.segments),
            config: self.config.clone(),
            next_segment_id: Arc::clone(&self.next_segment_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_stealing() {
        let manager = DownloadManager::new(100_000_000, 4); // 100MB, 4 parts
        
        // Start all segments
        for i in 0..4 {
            manager.start_segment(i);
        }

        // Simulate segment 0 completing fast
        manager.update_progress(0, 25_000_000, 10_000_000);
        manager.complete_segment(0);

        // Try to steal work
        let stolen = manager.on_segment_complete(0);
        assert!(stolen.is_some(), "Should have stolen work");

        let stolen = stolen.unwrap();
        println!("Stolen segment: {:?}", stolen.new_segment);
        assert!(stolen.new_segment.len() > 0);
    }

    #[test]
    fn test_no_steal_when_small() {
        let config = WorkStealConfig {
            min_split_size: 10_000_000, // 10MB minimum
            ..Default::default()
        };
        let manager = DownloadManager::with_config(5_000_000, 2, config); // 5MB total
        
        manager.start_segment(0);
        manager.start_segment(1);
        manager.complete_segment(0);

        let stolen = manager.on_segment_complete(0);
        assert!(stolen.is_none(), "Should not steal when segment is too small");
    }

    #[test]
    fn test_find_splittable_segments() {
        let manager = DownloadManager::new(100_000_000, 4); // 100MB, 4 segments

        // Start all segments
        for i in 0..4 {
            manager.start_segment(i);
        }

        // Simulate one segment being very slow (1 KB/s) while others are fast (1 MB/s)
        manager.update_progress(0, 25_000_100, 1_000); // slow
        manager.update_progress(1, 50_000_100, 1_000_000); // fast
        manager.update_progress(2, 75_000_100, 1_000_000); // fast
        manager.update_progress(3, 99_000_000, 1_000_000); // fast, almost done

        // Allow up to 8 active connections
        let splits = manager.find_splittable_segments(8);
        assert!(!splits.is_empty(), "Should have split the slow segment");

        // The slow segment (0) should have been split
        let split_from_0 = splits.iter().any(|s| s.original_segment_id == 0);
        assert!(split_from_0, "Segment 0 (the slow one) should have been split");

        // Verify new segment is valid
        let new_seg = &splits[0].new_segment;
        assert!(new_seg.len() > 0, "New segment should have positive length");
        assert_eq!(new_seg.state, SegmentState::Downloading, "New segment should start as Downloading");
    }

    #[test]
    fn test_split_respects_max_active() {
        let manager = DownloadManager::new(100_000_000, 4);

        for i in 0..4 {
            manager.start_segment(i);
        }

        // All segments slow
        for i in 0..4 {
            let cursor = (i as u64 + 1) * 25_000_100;
            manager.update_progress(i, cursor, 1_000);
        }

        // max_active = 4, already have 4 active → should return nothing
        let splits = manager.find_splittable_segments(4);
        assert!(splits.is_empty(), "Should not split when already at max active");
    }

    #[test]
    fn test_active_segment_count() {
        let manager = DownloadManager::new(100_000_000, 4);

        assert_eq!(manager.active_segment_count(), 0, "No active segments initially");

        manager.start_segment(0);
        manager.start_segment(1);
        assert_eq!(manager.active_segment_count(), 2, "Two segments downloading");

        manager.complete_segment(0);
        assert_eq!(manager.active_segment_count(), 1, "One segment after completing one");
    }
}
