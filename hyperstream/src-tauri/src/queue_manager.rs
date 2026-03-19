use serde::{Deserialize, Serialize};
use std::collections::{VecDeque, HashMap};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

lazy_static::lazy_static! {
    pub static ref DOWNLOAD_QUEUE: Mutex<DownloadQueue> = Mutex::new(DownloadQueue::new());
    /// Retry metadata for downloads started via the queue. The session monitor
    /// reads this to decide whether a failed download should be re-queued.
    pub static ref RETRY_METADATA: Mutex<HashMap<String, RetryMetadata>> = Mutex::new(HashMap::new());
    /// Per-download settings overrides set by the queue before starting a download.
    /// The engine reads and removes these on startup.
    pub static ref DOWNLOAD_OVERRIDES: Mutex<HashMap<String, DownloadOverrides>> = Mutex::new(HashMap::new());
}

/// Per-download settings that override globals for a specific download.
#[derive(Debug, Clone, Default)]
pub struct DownloadOverrides {
    /// Override the global segment count.
    pub custom_segments: Option<u32>,
    /// Group label.
    pub group: Option<String>,
}

/// Metadata stored per-download so the session monitor can re-queue on failure.
#[derive(Debug, Clone)]
pub struct RetryMetadata {
    pub url: String,
    pub path: String,
    pub priority: DownloadPriority,
    pub custom_headers: Option<HashMap<String, String>>,
    pub expected_checksum: Option<String>,
    pub fresh_restart: bool,
    pub retry_count: u32,
    pub max_retries: u32,
}

/// Priority levels for downloads — higher priority downloads are started first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DownloadPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

impl Default for DownloadPriority {
    fn default() -> Self {
        DownloadPriority::Normal
    }
}

fn default_max_retries() -> u32 { 3 }

impl std::fmt::Display for DownloadPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadPriority::Low => write!(f, "low"),
            DownloadPriority::Normal => write!(f, "normal"),
            DownloadPriority::High => write!(f, "high"),
        }
    }
}

impl DownloadPriority {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "high" => DownloadPriority::High,
            "low" => DownloadPriority::Low,
            _ => DownloadPriority::Normal,
        }
    }
}

/// Represents a download waiting in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedDownload {
    pub id: String,
    pub url: String,
    pub path: String,
    pub priority: DownloadPriority,
    pub added_at: i64, // Unix timestamp ms
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    /// Expected checksum for post-download verification (e.g. "sha256:abc123...")
    #[serde(default)]
    pub expected_checksum: Option<String>,
    /// Force the next queued attempt to restart from a clean file instead of resuming.
    #[serde(default)]
    pub fresh_restart: bool,
    /// Number of times this download has been retried after failure.
    #[serde(default)]
    pub retry_count: u32,
    /// Maximum automatic retries before giving up. 0 = no auto-retry.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Delay in ms before retrying (doubles each retry).
    #[serde(default)]
    pub retry_delay_ms: u64,
    /// Download IDs that must complete before this download can start.
    /// The download will remain queued until all dependencies are "Done".
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Override the global segment count for this specific download.
    /// None = use global settings.segments.
    #[serde(default)]
    pub custom_segments: Option<u32>,
    /// Group tag for organizing related downloads (e.g. "project-assets").
    #[serde(default)]
    pub group: Option<String>,
}

/// Snapshot of queue state for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub max_concurrent: u32,
    pub active_count: u32,
    pub queued_count: usize,
    pub queued_items: Vec<QueuedDownload>,
    pub active_ids: Vec<String>,
    pub paused: bool,
    /// IDs of downloads whose dependencies are not yet satisfied.
    pub blocked_ids: Vec<String>,
}

/// The download queue manages concurrency limits and priority ordering.
///
/// Downloads are stored in three priority lanes (high, normal, low).
/// When a slot opens, the highest-priority non-empty lane is dequeued first.
/// Within a lane, downloads are served FIFO (first-in, first-out).
pub struct DownloadQueue {
    high: VecDeque<QueuedDownload>,
    normal: VecDeque<QueuedDownload>,
    low: VecDeque<QueuedDownload>,
    /// IDs of currently active (downloading) items.
    active: Vec<String>,
    /// Normalized URL reservations for active items keyed by download ID.
    active_urls: HashMap<String, String>,
    /// Maximum concurrent downloads allowed.
    max_concurrent: u32,
    /// Channel sender to notify the queue processor that a slot opened.
    notify_tx: Option<mpsc::UnboundedSender<QueueEvent>>,
    /// When true, the queue processor will not start any new downloads.
    paused: bool,
    /// IDs of downloads that have completed (for dependency resolution).
    /// Cleared periodically to avoid unbounded growth.
    completed_ids: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum QueueEvent {
    SlotAvailable,
    /// A download was enqueued — processor should check if it can start immediately.
    Enqueued,
}

impl DownloadQueue {
    pub fn new() -> Self {
        Self {
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
            active: Vec::new(),
            active_urls: HashMap::new(),
            max_concurrent: 5, // sensible default; overridden by settings
            notify_tx: None,
            paused: false,
            completed_ids: std::collections::HashSet::new(),
        }
    }

    pub fn set_max_concurrent(&mut self, max: u32) {
        self.max_concurrent = max.max(1); // at least 1
    }

    pub fn set_notify_channel(&mut self, tx: mpsc::UnboundedSender<QueueEvent>) {
        self.notify_tx = Some(tx);
    }

    /// Enqueue a download, placing it in the correct priority lane.
    pub fn enqueue(&mut self, item: QueuedDownload) -> bool {
        if self.contains(&item.id) || self.contains_normalized_url(&crate::normalize_download_url(&item.url)) {
            return false;
        }
        let lane = self.lane_mut(item.priority);
        lane.push_back(item);
        if let Some(tx) = &self.notify_tx {
            let _ = tx.send(QueueEvent::Enqueued);
        }
        true
    }

    /// Try to dequeue the next download if a slot is available.
    /// Returns `None` if the queue is paused, the concurrency limit is reached,
    /// the queue is empty, or all queued items have unsatisfied dependencies.
    pub fn try_dequeue(&mut self) -> Option<QueuedDownload> {
        if self.paused {
            return None;
        }
        if self.active.len() as u32 >= self.max_concurrent {
            return None;
        }
        // Try each priority lane. Within each lane, find the first item whose
        // dependencies are all satisfied (i.e. all in completed_ids).
        let item = self.dequeue_ready_from(&mut self.high.clone(), &self.completed_ids)
            .map(|idx| self.high.remove(idx).unwrap())
            .or_else(|| {
                self.dequeue_ready_from(&self.normal.clone(), &self.completed_ids)
                    .map(|idx| self.normal.remove(idx).unwrap())
            })
            .or_else(|| {
                self.dequeue_ready_from(&self.low.clone(), &self.completed_ids)
                    .map(|idx| self.low.remove(idx).unwrap())
            });

        if let Some(ref dl) = item {
            self.mark_active(&dl.id, &dl.url);
        }
        item
    }

    /// Find the index of the first item in a lane whose dependencies are satisfied.
    fn dequeue_ready_from(
        &self,
        lane: &VecDeque<QueuedDownload>,
        completed: &std::collections::HashSet<String>,
    ) -> Option<usize> {
        lane.iter().position(|dl| self.dependencies_satisfied(dl, completed))
    }

    /// Check if all dependencies of a download are satisfied.
    fn dependencies_satisfied(
        &self,
        dl: &QueuedDownload,
        completed: &std::collections::HashSet<String>,
    ) -> bool {
        dl.depends_on.is_empty() || dl.depends_on.iter().all(|dep| completed.contains(dep))
    }

    /// Record a download as completed for dependency resolution.
    pub fn mark_completed(&mut self, id: &str) {
        self.completed_ids.insert(id.to_string());
    }

    /// Pause the queue — no new downloads will be started.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the queue — allow starting new downloads again.
    pub fn resume(&mut self) {
        self.paused = false;
        if let Some(tx) = &self.notify_tx {
            let _ = tx.send(QueueEvent::SlotAvailable);
        }
    }

    /// Whether the queue is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Get distinct groups across all queued items.
    pub fn groups(&self) -> Vec<String> {
        let mut groups = std::collections::HashSet::new();
        for dl in self.high.iter().chain(self.normal.iter()).chain(self.low.iter()) {
            if let Some(ref g) = dl.group {
                groups.insert(g.clone());
            }
        }
        let mut sorted: Vec<String> = groups.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Get IDs of downloads with unsatisfied dependencies.
    pub fn blocked_ids(&self) -> Vec<String> {
        let mut blocked = Vec::new();
        for dl in self.high.iter().chain(self.normal.iter()).chain(self.low.iter()) {
            if !self.dependencies_satisfied(dl, &self.completed_ids) {
                blocked.push(dl.id.clone());
            }
        }
        blocked
    }

    /// Add a dependency: `download_id` must wait for `depends_on_id` to complete.
    pub fn add_dependency(&mut self, download_id: &str, depends_on_id: &str) -> bool {
        for lane in [&mut self.high, &mut self.normal, &mut self.low] {
            if let Some(dl) = lane.iter_mut().find(|d| d.id == download_id) {
                if !dl.depends_on.contains(&depends_on_id.to_string()) {
                    dl.depends_on.push(depends_on_id.to_string());
                }
                return true;
            }
        }
        false
    }

    /// Remove a dependency from a queued download.
    pub fn remove_dependency(&mut self, download_id: &str, depends_on_id: &str) -> bool {
        for lane in [&mut self.high, &mut self.normal, &mut self.low] {
            if let Some(dl) = lane.iter_mut().find(|d| d.id == download_id) {
                dl.depends_on.retain(|d| d != depends_on_id);
                return true;
            }
        }
        false
    }

    /// Mark a download as no longer active (completed, errored, or paused).
    /// This opens a slot for the next queued download.
    pub fn mark_finished(&mut self, id: &str) {
        self.mark_finished_with_notify(id, true);
    }

    /// Mark a download as no longer active without notifying the queue processor.
    /// Useful when bulk-pausing downloads during shutdown/protective flows where
    /// starting new queued work would be undesirable.
    pub fn mark_finished_silent(&mut self, id: &str) {
        self.mark_finished_with_notify(id, false);
    }

    fn mark_finished_with_notify(&mut self, id: &str, notify: bool) {
        self.active.retain(|a| a != id);
        self.active_urls.remove(id);
        if notify {
            if let Some(tx) = &self.notify_tx {
                let _ = tx.send(QueueEvent::SlotAvailable);
            }
        }
    }

    /// Mark a download as successfully completed — records it so dependent
    /// downloads can proceed. Call this instead of `mark_finished` when
    /// a download reaches "Done" / "Complete" status.
    pub fn mark_finished_success(&mut self, id: &str) {
        self.completed_ids.insert(id.to_string());
        self.mark_finished_with_notify(id, true);
    }

    /// Mark a download as actively running (called when starting directly, not via queue).
    pub fn mark_active(&mut self, id: &str, url: &str) {
        if !self.active.contains(&id.to_string()) {
            self.active.push(id.to_string());
        }
        self.active_urls
            .insert(id.to_string(), crate::normalize_download_url(url));
    }

    /// Check if there's room to start a download immediately.
    pub fn has_slot(&self) -> bool {
        (self.active.len() as u32) < self.max_concurrent
    }

    /// Number of items currently active.
    pub fn active_count(&self) -> u32 {
        self.active.len() as u32
    }

    /// Total queued items across all priority lanes.
    pub fn queued_count(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len()
    }

    /// Get a snapshot of the queue state for the frontend.
    pub fn status(&self) -> QueueStatus {
        let mut queued_items: Vec<QueuedDownload> = Vec::new();
        // High priority first, then normal, then low
        queued_items.extend(self.high.iter().cloned());
        queued_items.extend(self.normal.iter().cloned());
        queued_items.extend(self.low.iter().cloned());

        QueueStatus {
            max_concurrent: self.max_concurrent,
            active_count: self.active.len() as u32,
            queued_count: self.queued_count(),
            queued_items,
            active_ids: self.active.clone(),
            paused: self.paused,
            blocked_ids: self.blocked_ids(),
        }
    }

    /// Remove a specific download from the queue (not from active).
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.queued_count();
        self.high.retain(|d| d.id != id);
        self.normal.retain(|d| d.id != id);
        self.low.retain(|d| d.id != id);
        self.queued_count() < before
    }

    /// Move a queued item to a different priority lane.
    pub fn set_priority(&mut self, id: &str, priority: DownloadPriority) -> bool {
        // Find and remove from current lane
        let item = self.remove_from_lanes(id);
        if let Some(mut dl) = item {
            dl.priority = priority;
            self.lane_mut(priority).push_back(dl);
            true
        } else {
            false
        }
    }

    /// Move a queued item to the front of its priority lane.
    pub fn move_to_front(&mut self, id: &str) -> bool {
        let item = self.remove_from_lanes(id);
        if let Some(dl) = item {
            self.lane_mut(dl.priority).push_front(dl);
            true
        } else {
            false
        }
    }

    /// Check if a download (by ID) is in the queue or active.
    pub fn contains(&self, id: &str) -> bool {
        self.active.iter().any(|a| a == id)
            || self.high.iter().any(|d| d.id == id)
            || self.normal.iter().any(|d| d.id == id)
            || self.low.iter().any(|d| d.id == id)
    }

    pub fn contains_url(&self, url: &str) -> bool {
        let normalized = crate::normalize_download_url(url);
        !normalized.is_empty() && self.contains_normalized_url(&normalized)
    }

    /// Clear all queued items (does NOT touch active downloads).
    pub fn clear_queue(&mut self) {
        self.high.clear();
        self.normal.clear();
        self.low.clear();
    }

    /// Re-queue a failed download for automatic retry.
    /// Returns `true` if the download was re-queued, `false` if max retries reached.
    /// Uses settings for base/max delay (exponential backoff).
    pub fn requeue_failed(&mut self, mut item: QueuedDownload) -> bool {
        if item.retry_count >= item.max_retries {
            return false;
        }
        item.retry_count += 1;
        let s = crate::settings::load_settings();
        let base_ms = s.queue_retry_base_delay_secs as u64 * 1000;
        let max_ms = s.queue_retry_max_delay_secs as u64 * 1000;
        // Exponential backoff: base * 2^retry_count, capped at max
        item.retry_delay_ms = (base_ms.saturating_mul(1u64 << item.retry_count.min(12))).min(max_ms);
        // Demote to low priority on retry so fresh downloads get priority
        if item.retry_count >= 2 {
            item.priority = DownloadPriority::Low;
        }
        // Requeues commonly happen while the failed attempt is still marked active.
        // Remove it first so duplicate suppression doesn't silently drop the retry.
        self.active.retain(|a| a != &item.id);
        self.active_urls.remove(&item.id);
        let queued_before = self.queued_count();
        self.enqueue(item);
        self.queued_count() > queued_before
    }

    /// Get the queue position of a specific download (1-based, across all lanes).
    /// Active downloads return position 0. Not found returns None.
    pub fn position(&self, id: &str) -> Option<u32> {
        if self.active.iter().any(|a| a == id) {
            return Some(0);
        }
        let mut pos = 1u32;
        for lane in [&self.high, &self.normal, &self.low] {
            for dl in lane.iter() {
                if dl.id == id { return Some(pos); }
                pos += 1;
            }
        }
        None
    }

    // ── Private helpers ──────────────────────────────────────────────

    fn lane_mut(&mut self, priority: DownloadPriority) -> &mut VecDeque<QueuedDownload> {
        match priority {
            DownloadPriority::High => &mut self.high,
            DownloadPriority::Normal => &mut self.normal,
            DownloadPriority::Low => &mut self.low,
        }
    }

    fn remove_from_lanes(&mut self, id: &str) -> Option<QueuedDownload> {
        for lane in [&mut self.high, &mut self.normal, &mut self.low] {
            if let Some(pos) = lane.iter().position(|d| d.id == id) {
                return lane.remove(pos);
            }
        }
        None
    }

    fn contains_normalized_url(&self, normalized_url: &str) -> bool {
        self.active_urls.values().any(|url| url == normalized_url)
            || self.high.iter().any(|d| crate::normalize_download_url(&d.url) == normalized_url)
            || self.normal.iter().any(|d| crate::normalize_download_url(&d.url) == normalized_url)
            || self.low.iter().any(|d| crate::normalize_download_url(&d.url) == normalized_url)
    }
}

// ── Persistent queue state ─────────────────────────────────────────────

fn get_queue_store_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("hyperstream")
        .join("queue.json")
}

/// Save the current queue to disk so it survives restarts.
pub fn persist_queue() {
    let queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let items: Vec<&QueuedDownload> = queue.high.iter()
        .chain(queue.normal.iter())
        .chain(queue.low.iter())
        .collect();

    if let Ok(json) = serde_json::to_string_pretty(&items) {
        let path = get_queue_store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            if std::fs::rename(&tmp, &path).is_err() {
                let _ = std::fs::write(&path, &json);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

/// Restore the queue from disk on startup. Skips invalid items (missing id/url) to avoid poisoning the queue.
pub fn restore_queue() {
    let path = get_queue_store_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(items) = serde_json::from_str::<Vec<QueuedDownload>>(&data) {
            let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
            for item in items {
                if !item.id.is_empty() && !item.url.is_empty() {
                    queue.enqueue(item);
                }
            }
        }
    }
}

/// Background task that listens for queue events and starts downloads when
/// slots become available.  This must be spawned once during app setup.
pub async fn queue_processor(app: tauri::AppHandle, mut rx: mpsc::UnboundedReceiver<QueueEvent>) {
    use crate::core_state::AppState;

    loop {
        // Wait for an event (slot available or new enqueue)
        match rx.recv().await {
            Some(_event) => {
                // Drain all startable downloads
                loop {
                    let next = {
                        let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                        queue.try_dequeue()
                    };

                    match next {
                        Some(dl) => {
                            let app_clone = app.clone();
                            let dl_id = dl.id.clone();
                            let dl_url = dl.url.clone();
                            let dl_path = dl.path.clone();
                            let dl_headers = dl.custom_headers.clone();
                            let _checksum = dl.expected_checksum.clone();
                            let dl_retry_count = dl.retry_count;
                            let dl_max_retries = dl.max_retries;
                            let dl_priority_retry = dl.priority;
                            let dl_url_retry = dl.url.clone();
                            let dl_path_retry = dl.path.clone();
                            let dl_headers_retry = dl.custom_headers.clone();
                            let checksum_retry = dl.expected_checksum.clone();
                            let retry_delay = dl.retry_delay_ms;
                            let custom_segments = dl.custom_segments;
                            let group = dl.group.clone();

                            // Spawn the download in its own task.
                            // NOTE: start_download_impl spawns background tasks and returns
                            // immediately. The download monitor in session.rs calls
                            // mark_finished() when the download truly completes or errors.
                            // We must NOT call mark_finished() here — doing so would open
                            // a concurrency slot before the download finishes, defeating
                            // the max_concurrent limit entirely.
                            tokio::spawn(async move {
                                // If this is a retry, wait for the backoff delay first
                                if retry_delay > 0 {
                                    eprintln!("[Queue] Waiting {}ms before retry of {}", retry_delay, dl_id);
                                    tokio::time::sleep(std::time::Duration::from_millis(retry_delay)).await;
                                }

                                // Store retry metadata so the monitor can re-queue on failure
                                {
                                    let mut meta = RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
                                    meta.insert(dl_id.clone(), RetryMetadata {
                                        url: dl_url_retry.clone(),
                                        path: dl_path_retry.clone(),
                                        priority: dl_priority_retry,
                                        custom_headers: dl_headers_retry.clone(),
                                        expected_checksum: checksum_retry.clone(),
                                        fresh_restart: dl.fresh_restart,
                                        retry_count: dl_retry_count,
                                        max_retries: dl_max_retries,
                                    });
                                }

                                // Store per-download overrides for the engine to pick up
                                if custom_segments.is_some() || group.is_some() {
                                    let mut overrides = DOWNLOAD_OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
                                    overrides.insert(dl_id.clone(), DownloadOverrides {
                                        custom_segments,
                                        group: group.clone(),
                                    });
                                }

                                let state: tauri::State<AppState> = app_clone.state();

                                let result = crate::engine::start_download_routed(
                                    &app_clone,
                                    &state,
                                    dl_id.clone(),
                                    dl_url,
                                    dl_path.clone(),
                                    dl_headers,
                                    false,
                                ).await;

                                // If the routed start itself returns Err, the engine never took
                                // ownership of cleanup. Handle retry and slot release here.
                                if let Err(e) = result {
                                    eprintln!("[Queue] Download {} failed to start: {}", dl_id, e);
                                    {
                                        let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                                        queue.mark_finished(&dl_id);
                                    }

                                    // Try auto-retry
                                    let requeued = {
                                        let retry_item = QueuedDownload {
                                            id: dl_id.clone(),
                                            url: dl_url_retry,
                                            path: dl_path_retry,
                                            priority: dl_priority_retry,
                                            added_at: chrono::Utc::now().timestamp_millis(),
                                            custom_headers: dl_headers_retry,
                                            expected_checksum: checksum_retry,
                                            fresh_restart: dl.fresh_restart,
                                            retry_count: dl_retry_count,
                                            max_retries: dl_max_retries,
                                            retry_delay_ms: 0,
                                            depends_on: Vec::new(),
                                            custom_segments,
                                            group,
                                        };
                                        let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                                        queue.requeue_failed(retry_item)
                                    };
                                    if requeued {
                                        eprintln!("[Queue] Re-queued {} for retry (attempt {})", dl_id, dl_retry_count + 1);
                                        let _ = app_clone.emit("download_retry", serde_json::json!({
                                            "id": dl_id,
                                            "attempt": dl_retry_count + 1,
                                            "max_retries": dl_max_retries,
                                        }));
                                    }

                                    // Clean up retry metadata
                                    let mut meta = RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
                                    meta.remove(&dl_id);

                                    persist_queue();
                                }
                                // If result is Ok, start_download_impl has spawned the monitor
                                // which will handle mark_finished, retry, and integrity checks.
                            });
                        }
                        None => break, // No more slots or queue empty
                    }
                }
                persist_queue();
            }
            None => {
                // Channel closed, processor should exit
                break;
            }
        }
    }
}

/// Initialize the queue system: restore persisted state, set concurrency from
/// settings, and spawn the background processor.
pub fn init_queue(app: &tauri::AppHandle) {
    let settings = crate::settings::load_settings();

    let (tx, rx) = mpsc::unbounded_channel();

    {
        let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.set_max_concurrent(settings.max_concurrent_downloads);
        queue.set_notify_channel(tx.clone());
    }

    restore_queue();

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        queue_processor(app_clone, rx).await;
    });

    // Trigger processing of any restored queue items
    let _ = tx.send(QueueEvent::Enqueued);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued_download(id: &str) -> QueuedDownload {
        queued_download_with_url(id, "https://example.com/file.bin")
    }

    fn queued_download_with_url(id: &str, url: &str) -> QueuedDownload {
        QueuedDownload {
            id: id.to_string(),
            url: url.to_string(),
            path: "/tmp/file.bin".to_string(),
            priority: DownloadPriority::Normal,
            added_at: 0,
            custom_headers: None,
            expected_checksum: None,
            fresh_restart: false,
            retry_count: 0,
            max_retries: 3,
            retry_delay_ms: 0,
            depends_on: Vec::new(),
            custom_segments: None,
            group: None,
        }
    }

    #[test]
    fn requeue_failed_reenqueues_even_if_id_is_still_active() {
        let mut queue = DownloadQueue::new();
        queue.mark_active("download-1", "https://example.com/file.bin");

        let requeued = queue.requeue_failed(queued_download("download-1"));

        assert!(requeued);
        assert_eq!(queue.active_count(), 0);
        assert_eq!(queue.queued_count(), 1);
        assert_eq!(queue.position("download-1"), Some(1));
    }

    #[test]
    fn queued_download_deserializes_missing_fresh_restart_as_false() {
        let item: QueuedDownload = serde_json::from_str(r#"{
            "id": "download-1",
            "url": "https://example.com/file.bin",
            "path": "/tmp/file.bin",
            "priority": "Normal",
            "added_at": 0,
            "retry_count": 0,
            "max_retries": 3,
            "retry_delay_ms": 0
        }"#).unwrap();

        assert!(!item.fresh_restart);
    }

    #[test]
    fn mark_finished_silent_removes_active_without_notifying_processor() {
        let mut queue = DownloadQueue::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        queue.set_notify_channel(tx);
        queue.mark_active("download-1", "https://example.com/file.bin");

        queue.mark_finished_silent("download-1");

        assert_eq!(queue.active_count(), 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn enqueue_rejects_duplicate_normalized_urls() {
        let mut queue = DownloadQueue::new();

        assert!(queue.enqueue(queued_download_with_url(
            "download-1",
            "HTTPS://Example.com:443/file.bin#frag",
        )));
        assert!(!queue.enqueue(queued_download_with_url(
            "download-2",
            "https://example.com/file.bin",
        )));
        assert_eq!(queue.queued_count(), 1);
    }

    #[test]
    fn mark_finished_removes_active_url_reservation() {
        let mut queue = DownloadQueue::new();
        queue.mark_active("download-1", "https://example.com/file.bin#frag");

        assert!(queue.contains_url("https://example.com/file.bin"));

        queue.mark_finished_silent("download-1");

        assert!(!queue.contains_url("https://example.com/file.bin"));
    }
}
