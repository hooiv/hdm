//! Download History — persistent, searchable record of all completed & failed
//! downloads with rich metadata (timestamps, avg speed, source, mirror info).
//!
//! History is stored separately from `downloads.json` in `history.json` so
//! active/paused downloads aren't mixed with historical records.  The history
//! file is append-optimised (new entries go at the end) and self-pruning
//! according to the `max_history_entries` setting.
//!
//! # Commands
//! - `get_download_history` — paginated, filterable list
//! - `search_download_history` — full-text search across filename/URL
//! - `clear_download_history` — wipe all records
//! - `export_download_history` — CSV export
//! - `delete_history_entry` — remove a single record

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Serialize all history read-modify-write operations.
static HISTORY_LOCK: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

/// Maximum entries before auto-pruning (oldest removed first).
const DEFAULT_MAX_ENTRIES: usize = 10_000;

// ────────────────────────────────────────────────────────────────────────────
// Data model
// ────────────────────────────────────────────────────────────────────────────

/// One historical download record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unique download ID (same as the one used during download).
    pub id: String,
    /// Original URL.
    pub url: String,
    /// Final file path on disk.
    pub path: String,
    /// Filename (convenience duplicate of the path's basename).
    pub filename: String,
    /// Total size in bytes (0 = unknown).
    pub total_size: u64,
    /// Bytes actually downloaded (may differ from total_size on errors).
    pub downloaded_bytes: u64,
    /// Final status: "Complete", "Error", "Cancelled".
    pub status: String,
    /// ISO-8601 timestamp of when the download was started.
    pub started_at: String,
    /// ISO-8601 timestamp of when the download finished.
    pub finished_at: String,
    /// Average speed in bytes/sec over the download's lifetime.
    pub avg_speed_bps: u64,
    /// Duration in seconds.
    pub duration_secs: u64,
    /// Number of segments used.
    pub segments_used: u32,
    /// Optional error message (when status = "Error").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Optional source label (e.g. "HLS", "DASH", "Multi-Source").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
}

/// Filter for querying history.
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryFilter {
    /// Page number (0-indexed).
    #[serde(default)]
    pub page: usize,
    /// Items per page (default 50).
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    /// Optional status filter ("Complete", "Error", "Cancelled").
    #[serde(default)]
    pub status: Option<String>,
    /// Optional date-from filter (ISO-8601).
    #[serde(default)]
    pub date_from: Option<String>,
    /// Optional date-to filter (ISO-8601).
    #[serde(default)]
    pub date_to: Option<String>,
    /// Sort field: "date" (default), "size", "speed", "name".
    #[serde(default = "default_sort_field")]
    pub sort_by: String,
    /// Sort direction: true = descending (default).
    #[serde(default = "default_sort_desc")]
    pub sort_desc: bool,
}

fn default_page_size() -> usize { 50 }
fn default_sort_field() -> String { "date".to_string() }
fn default_sort_desc() -> bool { true }

/// Paginated result.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub total_count: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

// ────────────────────────────────────────────────────────────────────────────
// Storage
// ────────────────────────────────────────────────────────────────────────────

fn get_history_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("history.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".hyperstream").join("history.json")
}

fn load_all() -> Vec<HistoryEntry> {
    let path = get_history_path();
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_all(entries: &[HistoryEntry]) -> Result<(), String> {
    let path = get_history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
    }
    // Atomic write: temp file → rename
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("serialize: {}", e))?;
    fs::write(&tmp, &json).map_err(|e| format!("write: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Public API
// ────────────────────────────────────────────────────────────────────────────

/// Record a completed (or failed/cancelled) download in history.
/// Called internally by the download engine on completion.
pub fn record(entry: HistoryEntry) {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = load_all();

    // De-duplicate: if an entry with the same ID exists, update it
    if let Some(pos) = entries.iter().position(|e| e.id == entry.id) {
        entries[pos] = entry;
    } else {
        entries.push(entry);
    }

    // Auto-prune: keep at most DEFAULT_MAX_ENTRIES, dropping oldest first
    if entries.len() > DEFAULT_MAX_ENTRIES {
        // Sort by finished_at ascending so oldest are first
        entries.sort_by(|a, b| a.finished_at.cmp(&b.finished_at));
        let excess = entries.len() - DEFAULT_MAX_ENTRIES;
        entries.drain(..excess);
    }

    let _ = save_all(&entries);
}

/// Get a filtered, paginated history page.
pub fn query(filter: &HistoryFilter) -> HistoryPage {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = load_all();

    // Apply filters
    if let Some(ref status) = filter.status {
        let s = status.to_lowercase();
        entries.retain(|e| e.status.to_lowercase() == s);
    }
    if let Some(ref from) = filter.date_from {
        entries.retain(|e| e.finished_at.as_str() >= from.as_str());
    }
    if let Some(ref to) = filter.date_to {
        entries.retain(|e| e.finished_at.as_str() <= to.as_str());
    }

    // Sort
    match filter.sort_by.as_str() {
        "size" => entries.sort_by_key(|e| e.total_size),
        "speed" => entries.sort_by_key(|e| e.avg_speed_bps),
        "name" => entries.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase())),
        _ => entries.sort_by(|a, b| a.finished_at.cmp(&b.finished_at)), // "date"
    }
    if filter.sort_desc {
        entries.reverse();
    }

    let total_count = entries.len();
    let page_size = filter.page_size.max(1).min(500);
    let total_pages = if total_count == 0 { 1 } else { (total_count + page_size - 1) / page_size };
    let page = filter.page.min(total_pages.saturating_sub(1));
    let start = page * page_size;
    let end = (start + page_size).min(total_count);
    let page_entries = if start < total_count {
        entries[start..end].to_vec()
    } else {
        Vec::new()
    };

    HistoryPage {
        entries: page_entries,
        total_count,
        page,
        page_size,
        total_pages,
    }
}

/// Full-text search across filename and URL (case-insensitive substring match).
pub fn search(query_str: &str, limit: usize) -> Vec<HistoryEntry> {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let entries = load_all();
    let q = query_str.to_lowercase();
    entries
        .into_iter()
        .filter(|e| {
            e.filename.to_lowercase().contains(&q)
                || e.url.to_lowercase().contains(&q)
                || e.path.to_lowercase().contains(&q)
        })
        .take(limit.max(1).min(1000))
        .collect()
}

/// Clear all history entries.
pub fn clear() -> Result<(), String> {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    save_all(&[])
}

/// Delete a single history entry by ID.
pub fn delete_entry(id: &str) -> Result<(), String> {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = load_all();
    let before = entries.len();
    entries.retain(|e| e.id != id);
    if entries.len() == before {
        return Err(format!("No history entry with ID '{}'", id));
    }
    save_all(&entries)
}

/// Export history as CSV bytes.
pub fn export_csv() -> Result<String, String> {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let entries = load_all();

    let mut csv = String::from("ID,Filename,URL,Path,Size,Downloaded,Status,Started,Finished,AvgSpeed,Duration,Segments,Error,Source\n");
    for e in &entries {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&e.id),
            csv_escape(&e.filename),
            csv_escape(&e.url),
            csv_escape(&e.path),
            e.total_size,
            e.downloaded_bytes,
            csv_escape(&e.status),
            csv_escape(&e.started_at),
            csv_escape(&e.finished_at),
            e.avg_speed_bps,
            e.duration_secs,
            e.segments_used,
            csv_escape(e.error_message.as_deref().unwrap_or("")),
            csv_escape(e.source_type.as_deref().unwrap_or("")),
        ));
    }
    Ok(csv)
}

/// Helper: CSV-escape a field (quote if it contains comma, newline, or quote).
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('\n') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Summary statistics across all history.
#[derive(Debug, Clone, Serialize)]
pub struct HistorySummary {
    pub total_downloads: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub total_bytes: u64,
    pub avg_speed_bps: u64,
}

pub fn summary() -> HistorySummary {
    let _lock = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let entries = load_all();
    let total = entries.len();
    let completed = entries.iter().filter(|e| e.status == "Complete").count();
    let failed = entries.iter().filter(|e| e.status == "Error").count();
    let cancelled = entries.iter().filter(|e| e.status == "Cancelled").count();
    let total_bytes: u64 = entries.iter().map(|e| e.downloaded_bytes).sum();
    let total_speed: u64 = entries.iter().map(|e| e.avg_speed_bps).sum();
    let avg_speed = if total > 0 { total_speed / total as u64 } else { 0 };

    HistorySummary {
        total_downloads: total,
        completed,
        failed,
        cancelled,
        total_bytes,
        avg_speed_bps: avg_speed,
    }
}
