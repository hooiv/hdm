pub mod core_state;
pub use core_state::*;
pub mod engine;
use engine::session::*;
use tauri::{Emitter, State, Manager};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use crate::downloader::disk::{DiskWriter, WriteRequest};
use std::sync::mpsc;
use std::thread;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

mod downloader;
mod persistence;
mod http_server;
mod commands;
use crate::http_server::StreamingSource;
pub mod settings;
pub mod settings_cache;
mod settings_utils;
mod speed_limiter;
mod speed_profiles;
mod download_history;
mod site_rules;
mod file_categorizer;
mod clipboard;
mod network;
mod scheduler;
mod media;
mod plugin_vm;
pub mod mqtt_client;
mod doi_resolver;
mod spider;

mod zip_preview;
mod proxy;
mod adaptive_threads;

mod virus_scanner;
mod import_export;
mod lan_api;
mod system_monitor;
mod feeds;
mod search;
pub mod resilience;
pub mod network_diagnostics;
pub mod auto_recovery;
pub mod resilience_integration;
pub mod resilience_analytics;
pub mod mirror_scoring;
pub mod download_recovery;
pub mod recovery_commands;
pub mod recovery_integration;
pub mod parallel_mirror_retry;
pub mod parallel_mirror_commands;
pub mod mirror_analytics;
pub mod mirror_analytics_commands;
pub mod speed_acceleration;
pub mod speed_acceleration_commands;
pub mod failure_prediction;
pub mod failure_prediction_commands;
pub mod download_groups;
pub mod group_scheduler;
pub mod group_persistence;
pub mod group_engine;
pub mod group_dag_solver;
pub mod group_batch_detector;
pub mod group_atomic_ops;
pub mod group_metrics;
pub mod group_smart_queue;
pub mod group_error_handler;
pub mod group_commands;

// mod virtual_drive;
mod cloud_bridge;
mod media_processor;
mod ai;
mod audio_events;
mod webhooks;
mod archive_manager;
mod metadata_scrubber;
mod ephemeral_server;
mod wayback;
mod docker_pull;
mod power_manager;
pub mod cas_manager;
mod warc_archiver;
mod git_lfs;
mod sandbox;
mod notarize;
mod mirror_hunter;
mod usb_flasher;
mod api_replay;
mod c2pa_validator;
mod bandwidth_arb;
mod bandwidth_allocator;
mod crash_recovery;
mod stego_vault;
mod tui_dashboard;
mod auto_extract;
mod ipfs_gateway;
mod sql_query;
mod dlna_cast;
mod qos_manager;
mod mod_optimizer;
mod rclone_bridge;
mod subtitle_gen;
mod virtual_drive;
mod geofence;
mod queue_manager;
mod integrity;
mod network_monitor;
mod event_bus;
mod event_sourcing;
mod video_detector;
mod bandwidth_history;
pub mod session_state;
pub mod session_recovery;
pub mod segment_integrity;

use persistence::SavedDownload;

/// Resolve a file path that may be relative (just a filename) to an absolute path.
/// Tries: 1) already absolute 2) download_dir/path 3) desktop/path
pub fn resolve_download_path(path: &str, download_dir: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);
    let full_path = if p.is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(download_dir).join(path)
    };
    
    if full_path.exists() {
        return Ok(full_path);
    }

    // Fallback: try desktop
    if let Some(desktop) = dirs::desktop_dir() {
        let desktop_path = desktop.join(path);
        if desktop_path.exists() {
            return Ok(desktop_path);
        }
    }

    // Return the download_dir-based path even if it doesn't exist yet
    Ok(full_path)
}

pub(crate) fn normalize_download_url(raw_url: &str) -> String {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    match url::Url::parse(trimmed) {
        Ok(mut parsed) => {
            parsed.set_fragment(None);

            let is_default_port = matches!(
                (parsed.scheme(), parsed.port()),
                ("http", Some(80)) | ("https", Some(443))
            );
            if is_default_port {
                let _ = parsed.set_port(None);
            }

            parsed.to_string()
        }
        Err(_) => trimmed.to_string(),
    }
}

fn duplicate_download_id_error() -> String {
    "A download with this ID is already active or queued".to_string()
}

fn duplicate_download_url_error() -> String {
    "A download for this URL is already active or queued".to_string()
}

// (id, start, end, cursor, state, speed)

// ─── Torrent commands ────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct TorrentBulkActionResult {
    attempted: usize,
    succeeded: usize,
    failed: usize,
    failed_ids: Vec<usize>,
}

#[derive(Clone, serde::Serialize)]
struct AddTorrentResult {
    id: usize,
    warnings: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
struct TorrentActionFailedEvent {
    timestamp_ms: u64,
    severity: String,
    category: String,
    action: String,
    id: Option<usize>,
    error: String,
}

#[derive(Clone, serde::Serialize)]
struct TorrentDiagnostics {
    generated_at_ms: u64,
    auto_manage_queue: bool,
    max_active_downloads: u32,
    auto_stop_seeding: bool,
    seed_ratio_limit: f64,
    seed_time_limit_mins: u32,
    total_torrents: usize,
    live_torrents: usize,
    paused_torrents: usize,
    error_torrents: usize,
    initializing_torrents: usize,
    completed_torrents: usize,
    pinned_torrents: usize,
    queue_auto_paused: usize,
    seeding_policy_auto_paused: usize,
    recent_error_count: usize,
    recent_warning_count: usize,
    recent_errors: Vec<TorrentActionFailedEvent>,
    recent_warnings: Vec<TorrentActionFailedEvent>,
    recent_failures: Vec<TorrentActionFailedEvent>,
    torrents: Vec<network::bittorrent::manager::TorrentStatus>,
}

#[derive(Clone, serde::Serialize)]
struct RecentIssueClearResult {
    removed_count: usize,
    clear_token: Option<u64>,
}

#[derive(Clone)]
struct ClearedTorrentIssuesBatch {
    token: u64,
    entries: Vec<TorrentActionFailedEvent>,
}

const MAX_RECENT_TORRENT_ERRORS: usize = 128;
const MAX_TORRENT_ERROR_MESSAGE_CHARS: usize = 512;

fn recent_torrent_errors() -> &'static Mutex<Vec<TorrentActionFailedEvent>> {
    static STORE: OnceLock<Mutex<Vec<TorrentActionFailedEvent>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn last_cleared_torrent_issues() -> &'static Mutex<Option<ClearedTorrentIssuesBatch>> {
    static STORE: OnceLock<Mutex<Option<ClearedTorrentIssuesBatch>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

fn next_torrent_issue_clear_token() -> u64 {
    static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);
    TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn should_restore_cleared_batch(expected_token: Option<u64>, batch_token: u64) -> bool {
    match expected_token {
        Some(expected) => expected == batch_token,
        None => true,
    }
}

fn torrent_action_category(action: &str) -> &'static str {
    match action {
        "add_magnet" | "add_torrent_file" => "ingest",
        "add_magnet_config" | "add_torrent_file_config" => "config",
        "pause" | "resume" | "remove" | "update_files" | "set_priority" | "set_pinned" => "action",
        "pause_policy"
        | "resume_policy"
        | "remove_policy"
        | "set_priority_policy"
        | "set_pinned_policy"
        | "add_magnet_policy"
        | "add_torrent_file_policy"
        | "pause_all_policy"
        | "resume_all_policy"
        | "settings_policy" => "policy",
        _ => "unknown",
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_torrent_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.chars().count() <= MAX_TORRENT_ERROR_MESSAGE_CHARS {
        return trimmed.to_string();
    }
    let mut out = trimmed
        .chars()
        .take(MAX_TORRENT_ERROR_MESSAGE_CHARS)
        .collect::<String>();
    out.push_str("...");
    out
}

async fn enforce_torrent_policies(
    tm: &network::bittorrent::manager::TorrentManager,
    settings: &settings::Settings,
) -> Result<(), String> {
    let queue_limit = if settings.torrent_auto_manage_queue {
        settings.torrent_max_active_downloads as usize
    } else {
        0
    };

    tm.enforce_queue_limits(queue_limit)
        .await
        .map_err(|e| e.to_string())?;
    tm.enforce_seeding_policy(
        settings.torrent_auto_stop_seeding,
        settings.torrent_seed_ratio_limit,
        settings.torrent_seed_time_limit_mins,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn apply_initial_torrent_config(
    tm: &network::bittorrent::manager::TorrentManager,
    id: usize,
    initial_priority: Option<String>,
    pinned: Option<bool>,
) -> Result<settings::Settings, String> {
    let normalized_priority = match initial_priority {
        Some(priority) => Some(
            settings::normalize_torrent_priority_label(&priority)
                .ok_or_else(|| "Priority must be one of: high, normal, low".to_string())?
                .to_string(),
        ),
        None => None,
    };

    if normalized_priority.is_none() && pinned.is_none() {
        return Ok(settings::load_settings());
    }

    let info_hash = tm
        .get_torrents()
        .into_iter()
        .find(|torrent| torrent.id == id)
        .map(|torrent| torrent.info_hash)
        .ok_or_else(|| format!("Torrent {} not found", id))?;

    let mut current_settings = settings::load_settings();
    apply_torrent_preferences_to_settings(
        &mut current_settings,
        &info_hash,
        normalized_priority.as_deref(),
        pinned,
    );

    settings::save_settings(&current_settings)?;
    Ok(current_settings)
}

fn apply_torrent_preferences_to_settings(
    current_settings: &mut settings::Settings,
    info_hash: &str,
    normalized_priority: Option<&str>,
    pinned: Option<bool>,
) {
    let normalized_hash = info_hash.to_ascii_lowercase();

    if let Some(priority) = normalized_priority {
        if priority == "normal" {
            current_settings
                .torrent_priority_overrides
                .remove(&normalized_hash);
        } else {
            current_settings
                .torrent_priority_overrides
                .insert(normalized_hash.clone(), priority.to_string());
        }
    }

    if let Some(should_pin) = pinned {
        if should_pin {
            current_settings.torrent_pinned_hashes.insert(normalized_hash);
        } else {
            current_settings.torrent_pinned_hashes.remove(&normalized_hash);
        }
    }
}

fn emit_torrent_refresh(app: &tauri::AppHandle) {
    let _ = app.emit("torrents_refresh", ());
}

fn emit_torrent_action_event(
    app: &tauri::AppHandle,
    action: &'static str,
    id: Option<usize>,
    severity: &'static str,
    error: &str,
) {
    let normalized_severity = match severity {
        "warning" => "warning",
        _ => "error",
    };

    let event = TorrentActionFailedEvent {
        timestamp_ms: now_unix_ms(),
        severity: normalized_severity.to_string(),
        category: torrent_action_category(action).to_string(),
        action: action.to_string(),
        id,
        error: normalize_torrent_error_message(error),
    };

    {
        let mut recent = recent_torrent_errors()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        recent.push(event.clone());
        if recent.len() > MAX_RECENT_TORRENT_ERRORS {
            let remove = recent.len() - MAX_RECENT_TORRENT_ERRORS;
            recent.drain(0..remove);
        }
    }

    let _ = app.emit(
        "torrent_action_failed",
        event,
    );
}

fn emit_torrent_action_failed(
    app: &tauri::AppHandle,
    action: &'static str,
    id: Option<usize>,
    error: &str,
) {
    emit_torrent_action_event(app, action, id, "error", error);
}

fn emit_torrent_action_warning(
    app: &tauri::AppHandle,
    action: &'static str,
    id: Option<usize>,
    warning: &str,
) {
    emit_torrent_action_event(app, action, id, "warning", warning);
}

#[tauri::command]
async fn add_magnet_link(
    magnet: String,
    save_path: Option<String>,
    paused: Option<bool>,
    initial_priority: Option<String>,
    pinned: Option<bool>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AddTorrentResult, String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    let add_outcome = match tm
        .add_magnet(&magnet, save_path, paused.unwrap_or(false))
        .await
    {
        Ok(outcome) => outcome,
        Err(e) => {
            let msg = e.to_string();
            emit_torrent_action_failed(&app, "add_magnet", None, &msg);
            return Err(msg);
        }
    };
    let id = add_outcome.id;
    let mut warnings = Vec::new();
    if add_outcome.already_managed {
        warnings.push("Torrent already exists in session; reusing existing torrent".to_string());
    }
    let current_settings = match apply_initial_torrent_config(
        tm.as_ref(),
        id,
        initial_priority,
        pinned,
    ) {
        Ok(settings) => settings,
        Err(e) => {
            emit_torrent_action_failed(&app, "add_magnet_config", Some(id), &e);
            return Err(e);
        }
    };
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        warnings.push(format!("Policy enforcement warning: {}", e));
        emit_torrent_action_warning(&app, "add_magnet_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);
    Ok(AddTorrentResult { id, warnings })
}

/// Add a torrent from a base64-encoded `.torrent` file sent by the frontend.
#[tauri::command]
async fn add_torrent_file(
    base64_data: String,
    save_path: Option<String>,
    paused: Option<bool>,
    only_files: Option<Vec<usize>>,
    initial_priority: Option<String>,
    pinned: Option<bool>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AddTorrentResult, String> {
    const MAX_TORRENT_METADATA_BYTES: usize = 8 * 1024 * 1024;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let estimated_decoded_len = (base64_data.len() / 4) * 3;
    if estimated_decoded_len > MAX_TORRENT_METADATA_BYTES {
        let msg = format!(
            "Torrent file is too large (max {} MiB)",
            MAX_TORRENT_METADATA_BYTES / (1024 * 1024)
        );
        emit_torrent_action_failed(&app, "add_torrent_file", None, &msg);
        return Err(msg);
    }

    let raw = STANDARD.decode(&base64_data)
        .map_err(|e| {
            let msg = format!("Invalid base64: {}", e);
            emit_torrent_action_failed(&app, "add_torrent_file", None, &msg);
            msg
        })?;
    if raw.len() > MAX_TORRENT_METADATA_BYTES {
        let msg = format!(
            "Torrent file is too large (max {} MiB)",
            MAX_TORRENT_METADATA_BYTES / (1024 * 1024)
        );
        emit_torrent_action_failed(&app, "add_torrent_file", None, &msg);
        return Err(msg);
    }

    let bytes = bytes::Bytes::from(raw);
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    let add_outcome = match tm
        .add_torrent_bytes(bytes, save_path, paused.unwrap_or(false), only_files)
        .await
    {
        Ok(outcome) => outcome,
        Err(e) => {
            let msg = e.to_string();
            emit_torrent_action_failed(&app, "add_torrent_file", None, &msg);
            return Err(msg);
        }
    };
    let id = add_outcome.id;
    let mut warnings = Vec::new();
    if add_outcome.already_managed {
        warnings.push("Torrent already exists in session; reusing existing torrent".to_string());
    }
    let current_settings = match apply_initial_torrent_config(
        tm.as_ref(),
        id,
        initial_priority,
        pinned,
    ) {
        Ok(settings) => settings,
        Err(e) => {
            emit_torrent_action_failed(&app, "add_torrent_file_config", Some(id), &e);
            return Err(e);
        }
    };
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        warnings.push(format!("Policy enforcement warning: {}", e));
        emit_torrent_action_warning(&app, "add_torrent_file_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);
    Ok(AddTorrentResult { id, warnings })
}

#[tauri::command]
async fn play_torrent(
    id: usize,
    file_id: Option<usize>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    let fid = match file_id {
        Some(f) => f,
        None => tm.get_largest_file_id(id)
            .ok_or_else(|| "Could not determine main file ID".to_string())?,
    };
    {
        let mut map = state.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.to_string(), StreamingSource::Torrent { torrent_id: id, file_id: fid });
    }
    Ok(format!("http://localhost:14733/p2p/{}", id))
}

#[tauri::command]
async fn get_torrents(
    state: State<'_, AppState>,
) -> Result<Vec<network::bittorrent::manager::TorrentStatus>, String> {
    match state.torrent_manager.as_ref() {
        Some(tm) => Ok(tm.get_torrents()),
        None => Ok(Vec::new()),
    }
}

/// Get the per-file breakdown for a single torrent.
#[tauri::command]
async fn get_torrent_files(
    id: usize,
    state: State<'_, AppState>,
) -> Result<Vec<network::bittorrent::manager::TorrentFileInfo>, String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    tm.get_torrent_files(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn pause_torrent(
    id: usize,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    if let Err(e) = tm.pause_torrent(id).await {
        let msg = e.to_string();
        emit_torrent_action_failed(&app, "pause", Some(id), &msg);
        return Err(msg);
    }
    let current_settings = settings::load_settings();
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "pause_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);
    Ok(())
}

#[tauri::command]
fn get_recent_torrent_errors() -> Vec<TorrentActionFailedEvent> {
    let recent = recent_torrent_errors()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    recent.iter().cloned().rev().collect()
}

fn normalize_issue_filter(filter: Option<&str>) -> Result<&str, String> {
    let normalized = filter.unwrap_or("all");
    if normalized != "all" && normalized != "errors" && normalized != "warnings" {
        return Err(format!("Unknown issue filter: {}", normalized));
    }
    Ok(normalized)
}

fn split_recent_torrent_issues_by_filter(
    entries: Vec<TorrentActionFailedEvent>,
    filter: Option<&str>,
) -> Result<(Vec<TorrentActionFailedEvent>, Vec<TorrentActionFailedEvent>), String> {
    let normalized = normalize_issue_filter(filter)?;

    let mut kept = Vec::with_capacity(entries.len());
    let mut removed = Vec::new();
    for entry in entries {
        let is_warning = entry.severity.eq_ignore_ascii_case("warning");
        let should_remove = match normalized {
            "all" => true,
            "errors" => !is_warning,
            "warnings" => is_warning,
            _ => false,
        };
        if should_remove {
            removed.push(entry);
        } else {
            kept.push(entry);
        }
    }
    Ok((kept, removed))
}

fn merge_recent_torrent_issues(
    mut existing: Vec<TorrentActionFailedEvent>,
    mut restored: Vec<TorrentActionFailedEvent>,
) -> Vec<TorrentActionFailedEvent> {
    existing.append(&mut restored);
    existing.sort_by_key(|entry| entry.timestamp_ms);
    if existing.len() > MAX_RECENT_TORRENT_ERRORS {
        let remove = existing.len() - MAX_RECENT_TORRENT_ERRORS;
        existing.drain(0..remove);
    }
    existing
}

fn clear_recent_torrent_issues_internal(filter: Option<&str>) -> Result<RecentIssueClearResult, String> {
    normalize_issue_filter(filter)?;

    let mut recent = recent_torrent_errors()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let existing = std::mem::take(&mut *recent);
    let (kept, removed) = split_recent_torrent_issues_by_filter(existing, filter)?;
    *recent = kept;
    drop(recent);

    let removed_count = removed.len();
    let mut last_cleared = last_cleared_torrent_issues()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if removed_count == 0 {
        *last_cleared = None;
        return Ok(RecentIssueClearResult {
            removed_count: 0,
            clear_token: None,
        });
    }

    let clear_token = next_torrent_issue_clear_token();
    *last_cleared = Some(ClearedTorrentIssuesBatch {
        token: clear_token,
        entries: removed,
    });

    Ok(RecentIssueClearResult {
        removed_count,
        clear_token: Some(clear_token),
    })
}

#[tauri::command]
fn clear_recent_torrent_errors() {
    let _ = clear_recent_torrent_issues_internal(None);
}

#[tauri::command]
fn clear_recent_torrent_issues(filter: Option<String>) -> Result<RecentIssueClearResult, String> {
    clear_recent_torrent_issues_internal(filter.as_deref())
}

#[tauri::command]
fn restore_recent_torrent_issues(expected_token: Option<u64>) -> usize {
    let mut recent = recent_torrent_errors()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut last_cleared = last_cleared_torrent_issues()
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let Some(batch) = last_cleared.take() else {
        return 0;
    };

    if !should_restore_cleared_batch(expected_token, batch.token) {
        *last_cleared = Some(batch);
        return 0;
    }

    let restored_count = batch.entries.len();
    let existing = std::mem::take(&mut *recent);
    *recent = merge_recent_torrent_issues(existing, batch.entries);

    restored_count
}

#[tauri::command]
fn get_torrent_diagnostics(state: State<'_, AppState>) -> Result<TorrentDiagnostics, String> {
    let settings = settings::load_settings();
    let torrents = match state.torrent_manager.as_ref() {
        Some(tm) => tm.get_torrents(),
        None => Vec::new(),
    };
    let recent_errors = get_recent_torrent_errors();
    let recent_warnings = recent_errors
        .iter()
        .filter(|entry| entry.severity.eq_ignore_ascii_case("warning"))
        .cloned()
        .take(64)
        .collect::<Vec<_>>();
    let recent_failures = recent_errors
        .iter()
        .filter(|entry| entry.severity.eq_ignore_ascii_case("error"))
        .cloned()
        .take(64)
        .collect::<Vec<_>>();

    let total_torrents = torrents.len();
    let live_torrents = torrents.iter().filter(|t| t.state == "live").count();
    let paused_torrents = torrents.iter().filter(|t| t.state == "paused").count();
    let error_torrents = torrents.iter().filter(|t| t.state == "error").count();
    let initializing_torrents = torrents
        .iter()
        .filter(|t| t.state == "initializing")
        .count();
    let completed_torrents = torrents.iter().filter(|t| t.finished).count();
    let pinned_torrents = torrents.iter().filter(|t| t.pinned).count();
    let queue_auto_paused = torrents
        .iter()
        .filter(|t| t.auto_pause_reason.as_deref() == Some("queue"))
        .count();
    let seeding_policy_auto_paused = torrents
        .iter()
        .filter(|t| t.auto_pause_reason.as_deref() == Some("seeding_policy"))
        .count();

    Ok(TorrentDiagnostics {
        generated_at_ms: now_unix_ms(),
        auto_manage_queue: settings.torrent_auto_manage_queue,
        max_active_downloads: settings.torrent_max_active_downloads,
        auto_stop_seeding: settings.torrent_auto_stop_seeding,
        seed_ratio_limit: settings.torrent_seed_ratio_limit,
        seed_time_limit_mins: settings.torrent_seed_time_limit_mins,
        total_torrents,
        live_torrents,
        paused_torrents,
        error_torrents,
        initializing_torrents,
        completed_torrents,
        pinned_torrents,
        queue_auto_paused,
        seeding_policy_auto_paused,
        recent_error_count: recent_failures.len(),
        recent_warning_count: recent_warnings.len(),
        recent_errors: recent_errors.into_iter().take(64).collect(),
        recent_warnings,
        recent_failures,
        torrents,
    })
}

#[tauri::command]
async fn resume_torrent(
    id: usize,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    if let Err(e) = tm.resume_torrent(id).await {
        let msg = e.to_string();
        emit_torrent_action_failed(&app, "resume", Some(id), &msg);
        return Err(msg);
    }
    let current_settings = settings::load_settings();
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "resume_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);
    Ok(())
}

#[tauri::command]
async fn pause_all_torrents(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<TorrentBulkActionResult, String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;

    let ids = tm
        .get_torrents()
        .into_iter()
        .filter(|torrent| torrent.state == "live")
        .map(|torrent| torrent.id)
        .collect::<Vec<_>>();
    let attempted = ids.len();

    let mut paused_count = 0usize;
    let mut failed_ids = Vec::new();
    for id in ids {
        match tm.pause_torrent(id).await {
            Ok(()) => paused_count += 1,
            Err(e) => {
                eprintln!("[torrent-bulk] failed to pause {}: {}", id, e);
                emit_torrent_action_failed(&app, "pause", Some(id), &e.to_string());
                failed_ids.push(id);
            }
        }
    }

    let current_settings = settings::load_settings();
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "pause_all_policy", None, &e);
    }
    emit_torrent_refresh(&app);
    Ok(TorrentBulkActionResult {
        attempted,
        succeeded: paused_count,
        failed: attempted.saturating_sub(paused_count),
        failed_ids,
    })
}

#[tauri::command]
async fn resume_all_torrents(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<TorrentBulkActionResult, String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;

    let ids = tm
        .get_torrents()
        .into_iter()
        .filter(|torrent| torrent.state == "paused" || torrent.state == "error")
        .map(|torrent| torrent.id)
        .collect::<Vec<_>>();
    let attempted = ids.len();

    let mut resumed_count = 0usize;
    let mut failed_ids = Vec::new();
    for id in ids {
        match tm.resume_torrent(id).await {
            Ok(()) => resumed_count += 1,
            Err(e) => {
                eprintln!("[torrent-bulk] failed to resume {}: {}", id, e);
                emit_torrent_action_failed(&app, "resume", Some(id), &e.to_string());
                failed_ids.push(id);
            }
        }
    }

    let current_settings = settings::load_settings();
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "resume_all_policy", None, &e);
    }
    emit_torrent_refresh(&app);
    Ok(TorrentBulkActionResult {
        attempted,
        succeeded: resumed_count,
        failed: attempted.saturating_sub(resumed_count),
        failed_ids,
    })
}

/// Remove a torrent from the session.
/// `delete_files` = true will also wipe the downloaded data from disk.
#[tauri::command]
async fn remove_torrent(
    id: usize,
    delete_files: bool,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    if let Err(e) = tm.remove_torrent(id, delete_files).await {
        let msg = e.to_string();
        emit_torrent_action_failed(&app, "remove", Some(id), &msg);
        return Err(msg);
    }
    let current_settings = settings::load_settings();
    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "remove_policy", Some(id), &e);
    }
    state.unregister_streaming_source(&id.to_string());
    emit_torrent_refresh(&app);
    Ok(())
}

/// Update the set of files to be downloaded within a torrent.
#[tauri::command]
async fn update_torrent_files(
    id: usize,
    included_ids: Vec<usize>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;
    if let Err(e) = tm.update_only_files(id, included_ids).await {
        let msg = e.to_string();
        emit_torrent_action_failed(&app, "update_files", Some(id), &msg);
        return Err(msg);
    }
    emit_torrent_refresh(&app);
    Ok(())
}

/// Open the torrent's save folder in the system file explorer.
#[tauri::command]
async fn open_torrent_folder(
    id: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let torrents = get_torrents(state).await?;
    let t = torrents
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Torrent {} not found", id))?;
    let path = std::path::Path::new(&t.save_path);
    if path.exists() {
        #[cfg(target_os = "windows")]
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn set_torrent_priority(
    id: usize,
    priority: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;

    let normalized = settings::normalize_torrent_priority_label(&priority)
        .ok_or_else(|| "Priority must be one of: high, normal, low".to_string())?;

    let info_hash = tm
        .get_torrents()
        .into_iter()
        .find(|torrent| torrent.id == id)
        .map(|torrent| torrent.info_hash)
        .ok_or_else(|| format!("Torrent {} not found", id))?;

    let mut current_settings = settings::load_settings();
    if normalized == "normal" {
        current_settings
            .torrent_priority_overrides
            .remove(&info_hash.to_ascii_lowercase());
    } else {
        current_settings
            .torrent_priority_overrides
            .insert(info_hash.to_ascii_lowercase(), normalized.to_string());
    }
    if let Err(e) = settings::save_settings(&current_settings) {
        emit_torrent_action_failed(&app, "set_priority", Some(id), &e);
        return Err(e);
    }

    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "set_priority_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);

    Ok(())
}

#[tauri::command]
async fn set_torrent_pinned(
    id: usize,
    pinned: bool,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tm = state.torrent_manager.as_ref()
        .ok_or_else(|| "Torrent subsystem is not available".to_string())?;

    let info_hash = tm
        .get_torrents()
        .into_iter()
        .find(|torrent| torrent.id == id)
        .map(|torrent| torrent.info_hash)
        .ok_or_else(|| format!("Torrent {} not found", id))?;

    let mut current_settings = settings::load_settings();
    let normalized_hash = info_hash.to_ascii_lowercase();
    if pinned {
        current_settings.torrent_pinned_hashes.insert(normalized_hash);
    } else {
        current_settings.torrent_pinned_hashes.remove(&normalized_hash);
    }
    if let Err(e) = settings::save_settings(&current_settings) {
        emit_torrent_action_failed(&app, "set_pinned", Some(id), &e);
        return Err(e);
    }

    if let Err(e) = enforce_torrent_policies(tm.as_ref(), &current_settings).await {
        emit_torrent_action_warning(&app, "set_pinned_policy", Some(id), &e);
    }
    emit_torrent_refresh(&app);

    Ok(())
}

/// Validate that an export/import file path is within safe user directories
fn validate_export_import_path(path: &str) -> Result<std::path::PathBuf, String> {
    let canonical = dunce::canonicalize(std::path::Path::new(path))
        .or_else(|_| {
            // File may not exist yet (export case) — canonicalize the parent
            let p = std::path::Path::new(path);
            if let Some(parent) = p.parent() {
                dunce::canonicalize(parent).map(|cp| cp.join(p.file_name().unwrap_or_default()))
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Invalid path"))
            }
        })
        .map_err(|e| format!("Invalid path: {}", e))?;

    let allowed_dirs: Vec<std::path::PathBuf> = [
        dirs::download_dir(),
        dirs::document_dir(),
        dirs::desktop_dir(),
    ]
    .iter()
    .filter_map(|d| d.as_ref().and_then(|p| dunce::canonicalize(p).ok()))
    .collect();

    if allowed_dirs.is_empty() {
        return Err("Cannot determine safe directories".to_string());
    }

    if !allowed_dirs.iter().any(|dir| canonical.starts_with(dir)) {
        return Err("Export/import path must be within Downloads, Documents, or Desktop".to_string());
    }

    Ok(canonical)
}

#[tauri::command]
async fn export_data(path: String) -> Result<(), String> {
    let validated_path = validate_export_import_path(&path)?;
    let settings = settings::load_settings();
    let downloads = persistence::load_downloads().unwrap_or_default();
    
    let data = crate::import_export::ExportData {
        version: "1.0".to_string(),
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        settings,
        downloads,
    };
    
    crate::import_export::save_export_to_file(&data, &validated_path.to_string_lossy())
}

#[tauri::command]
async fn import_data(path: String) -> Result<(), String> {
    let validated_path = validate_export_import_path(&path)?;
    let data = crate::import_export::load_export_from_file(&validated_path.to_string_lossy())?;
    
    // 1. Selectively merge safe settings fields — skip security-critical ones
    //    (proxy creds, cloud API keys, download_dir, telegram tokens, mqtt, vpn, etc.)
    let mut current_settings = settings::load_settings();
    current_settings.segments = data.settings.segments;
    current_settings.speed_limit_kbps = data.settings.speed_limit_kbps;
    current_settings.clipboard_monitor = data.settings.clipboard_monitor;
    current_settings.auto_start_extension = data.settings.auto_start_extension;
    current_settings.use_category_folders = data.settings.use_category_folders;
    current_settings.category_rules = data.settings.category_rules.clone();
    current_settings.auto_extract_archives = data.settings.auto_extract_archives;
    current_settings.cleanup_archives_after_extract = data.settings.cleanup_archives_after_extract;
    current_settings.auto_scrub_metadata = data.settings.auto_scrub_metadata;
    current_settings.prevent_sleep_during_download = data.settings.prevent_sleep_during_download;
    current_settings.pause_on_low_battery = data.settings.pause_on_low_battery;
    current_settings.min_threads = data.settings.min_threads;
    current_settings.max_threads = data.settings.max_threads;
    settings::save_settings(&current_settings)?;
    
    // 2. Restore Downloads (Merge/Append)
    let mut current_downloads = persistence::load_downloads().unwrap_or_default();
    let mut count = 0;
    
    for d in data.downloads {
        if !current_downloads.iter().any(|existing| existing.id == d.id) {
            current_downloads.push(d);
            count += 1;
        }
    }
    
    persistence::save_downloads(&current_downloads).map_err(|e| e.to_string())?;
    
    println!("Imported settings and {} new downloads.", count);
    Ok(())
}


#[tauri::command]
async fn start_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    url: String,
    path: String,
    force: Option<bool>,
    custom_headers: Option<std::collections::HashMap<String, String>>,
    expected_checksum: Option<String>,
) -> Result<(), String> {
    let force = force.unwrap_or(false);

    if state.has_active_download_id(&id) {
        return Err(duplicate_download_id_error());
    }
    if !force && state.has_active_download_url(&url) {
        return Err(duplicate_download_url_error());
    }

    {
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        if queue.contains(&id) {
            return Err(duplicate_download_id_error());
        }
        if !force && queue.contains_url(&url) {
            return Err(duplicate_download_url_error());
        }
        queue.mark_active(&id, &url);
    }

    {
        let mut meta = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
        meta.insert(id.clone(), queue_manager::RetryMetadata {
            url: url.clone(),
            path: path.clone(),
            priority: queue_manager::DownloadPriority::Normal,
            custom_headers: custom_headers.clone(),
            expected_checksum: expected_checksum.clone(),
            fresh_restart: false,
            retry_count: 0,
            max_retries: 0,
        });
    }

    let result = crate::engine::start_download_routed(
        &app,
        &state,
        id.clone(),
        url,
        path,
        custom_headers,
        force,
    ).await;

    if result.is_err() {
        crate::engine::session::clear_retry_metadata(&id);
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.mark_finished(&id);
    }

    result
}




mod pause_download_cmd {
    use super::*;

    fn collect_active_download_ids(state: &AppState) -> Vec<String> {
        let mut ids = Vec::new();
        ids.extend(
            state
                .downloads
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .keys()
                .cloned(),
        );
        ids.extend(
            state
                .hls_sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .keys()
                .cloned(),
        );
        ids.extend(
            state
                .dash_sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .keys()
                .cloned(),
        );
        ids
    }

    fn mark_queue_finished(id: &str, notify_queue: bool) {
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        if notify_queue {
            queue.mark_finished(id);
        } else {
            queue.mark_finished_silent(id);
        }
    }

    fn pause_download_by_id_inner(state: &AppState, id: &str, notify_queue: bool) -> bool {
        // first try regular downloads
        {
            let mut downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = downloads.remove(id) {
                let segments = session.manager.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                let total_downloaded: u64 = segments.iter()
                    .map(|s| s.downloaded_cursor.saturating_sub(s.start_byte))
                    .sum();
                let _ = session.stop_tx.send(());
                let filename = std::path::Path::new(&session.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string());
                let saved = persistence::SavedDownload {
                    id: id.to_string(),
                    url: session.url.clone(),
                    path: session.path.clone(),
                    filename,
                    total_size: session.manager.lock().unwrap_or_else(|e| e.into_inner()).file_size,
                    downloaded_bytes: total_downloaded,
                    status: "Paused".to_string(),
                    segments: Some(segments),
                    last_active: Some(chrono::Utc::now().to_rfc3339()),
                    error_message: None,
                    expected_checksum: crate::engine::session::get_expected_checksum(id),
                };
                let _ = persistence::upsert_download(saved);
                state.unregister_streaming_source(id);
                mark_queue_finished(id, notify_queue);
                return true;
            }
        }
        {
            let mut hls = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = hls.remove(id) {
                let downloaded = session.downloaded.load(std::sync::atomic::Ordering::Relaxed);
                let _ = session.stop_tx.send(());
                let filename = std::path::Path::new(&session.output_path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string());
                let saved = persistence::SavedDownload {
                    id: id.to_string(),
                    url: session.manifest_url.clone(),
                    path: session.output_path.clone(),
                    filename,
                    total_size: session.segment_sizes.iter().sum(),
                    downloaded_bytes: downloaded,
                    status: "Paused".to_string(),
                    segments: None,
                    last_active: Some(chrono::Utc::now().to_rfc3339()),
                    error_message: None,
                    expected_checksum: crate::engine::session::get_expected_checksum(id),
                };
                let _ = persistence::upsert_download(saved);
                mark_queue_finished(id, notify_queue);
                return true;
            }
        }
        {
            let mut dash = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = dash.remove(id) {
                let downloaded = session.downloaded.load(std::sync::atomic::Ordering::Relaxed);
                let _ = session.stop_tx.send(());
                let total = session.video_total + session.audio_total;
                let filename = std::path::Path::new(&session.output_path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string());
                let saved = persistence::SavedDownload {
                    id: id.to_string(),
                    url: session.manifest_url.clone(),
                    path: session.output_path.clone(),
                    filename,
                    total_size: total,
                    downloaded_bytes: downloaded,
                    status: "Paused".to_string(),
                    segments: None,
                    last_active: Some(chrono::Utc::now().to_rfc3339()),
                    error_message: None,
                    expected_checksum: crate::engine::session::get_expected_checksum(id),
                };
                let _ = persistence::upsert_download(saved);
                mark_queue_finished(id, notify_queue);
                return true;
            }
        }

        false
    }

    pub(crate) fn pause_download_by_id(state: &AppState, id: &str) -> bool {
        pause_download_by_id_inner(state, id, true)
    }

    pub(crate) fn pause_all_active_downloads(state: &AppState, notify_queue: bool) -> usize {
        let ids = collect_active_download_ids(state);
        let mut paused = 0usize;
        for id in ids {
            if pause_download_by_id_inner(state, &id, notify_queue) {
                paused += 1;
            }
        }
        queue_manager::persist_queue();
        paused
    }

    #[tauri::command]
    pub async fn pause_download(
        id: String,
        state: tauri::State<'_, AppState>,
    ) -> Result<(), String> {
        if pause_download_by_id(&state, &id) {
            return Ok(());
        }
        if let Ok(downloads) = persistence::load_downloads() {
            if let Some(mut d) = downloads.into_iter().find(|d| d.id == id) {
                d.status = "Paused".to_string();
                let _ = persistence::upsert_download(d);
            }
        }
        Ok(())
    }
}
pub(crate) use pause_download_cmd::pause_download;
pub(crate) use pause_download_cmd::pause_all_active_downloads;

// Return a shared snapshot of currently active downloads so the desktop app and
// localhost extension API expose the same protocol-aware status contract.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ActiveDownloadStatus {
    id: String,
    url: String,
    filename: Option<String>,
    downloaded: u64,
    total: u64,
    speed_bps: u64,
    status: String,
    can_pause: bool,
    can_cancel: bool,
}

pub(crate) fn collect_active_download_statuses(state: &AppState) -> Vec<ActiveDownloadStatus> {
    let mut result = Vec::new();
    {
        let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
        for (id, session) in downloads.iter() {
            let mgr = session.manager.lock().unwrap_or_else(|e| e.into_inner());
            let status = if mgr.is_complete() {
                "Complete".to_string()
            } else {
                "Downloading".to_string()
            };
            let can_control = status == "Downloading";
            result.push(ActiveDownloadStatus {
                id: id.clone(),
                url: session.url.clone(),
                filename: std::path::Path::new(&session.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string()),
                downloaded: mgr.total_downloaded(),
                total: mgr.file_size,
                speed_bps: mgr.total_speed(),
                status,
                can_pause: can_control,
                can_cancel: can_control,
            });
        }
    }
    {
        let hls = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
        for (id, session) in hls.iter() {
            let total: u64 = session.segment_sizes.iter().sum();
            let downloaded = session.downloaded.load(std::sync::atomic::Ordering::Relaxed);
            result.push(ActiveDownloadStatus {
                id: id.clone(),
                url: session.manifest_url.clone(),
                filename: std::path::Path::new(&session.output_path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string()),
                downloaded,
                total,
                speed_bps: 0,
                status: "Downloading".to_string(),
                can_pause: true,
                can_cancel: true,
            });
        }
    }
    {
        let dash = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        for (id, session) in dash.iter() {
            let total = session.video_total + session.audio_total;
            let downloaded = session.downloaded.load(std::sync::atomic::Ordering::Relaxed);
            result.push(ActiveDownloadStatus {
                id: id.clone(),
                url: session.manifest_url.clone(),
                filename: std::path::Path::new(&session.output_path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string()),
                downloaded,
                total,
                speed_bps: 0,
                status: "Downloading".to_string(),
                can_pause: true,
                can_cancel: true,
            });
        }
    }
    result
}

fn finalize_cancel_cleanup(state: &AppState, id: &str) -> Result<(), String> {
    {
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        let _ = queue.remove(id);
        queue.mark_finished(id);
    }
    queue_manager::persist_queue();
    scheduler::remove_scheduled_download(id);
    engine::session::clear_retry_metadata(id);
    bandwidth_allocator::ALLOCATOR.deregister(id);
    qos_manager::remove_download(id);
    state.unregister_streaming_source(id);
    persistence::remove_download(id)
}

pub(crate) fn cancel_download_by_id(state: &AppState, id: &str) -> Result<bool, String> {
    {
        let mut downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = downloads.remove(id) {
            let _ = session.stop_tx.send(());
            finalize_cancel_cleanup(state, id)?;
            return Ok(true);
        }
    }

    {
        let mut hls = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = hls.remove(id) {
            let _ = session.stop_tx.send(());
            finalize_cancel_cleanup(state, id)?;
            return Ok(true);
        }
    }

    {
        let mut dash = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = dash.remove(id) {
            let _ = session.stop_tx.send(());
            finalize_cancel_cleanup(state, id)?;
            return Ok(true);
        }
    }

    Ok(false)
}

pub(crate) fn control_active_download(state: &AppState, id: &str, action: &str) -> Result<bool, String> {
    match action {
        "pause" => Ok(pause_download_cmd::pause_download_by_id(state, id)),
        "cancel" => cancel_download_by_id(state, id),
        _ => Err("unknown action".to_string()),
    }
}

#[tauri::command]
fn get_downloads() -> Result<Vec<SavedDownload>, String> {
    persistence::load_downloads()
}

#[tauri::command]
fn list_active_downloads(state: State<'_, AppState>) -> Vec<ActiveDownloadStatus> {
    collect_active_download_statuses(&state)
}

#[tauri::command]
fn remove_download_entry(id: String, state: State<'_, AppState>) -> Result<(), String> {
    persistence::remove_download(&id)?;
    if !state.has_active_download_id(&id) {
        state.unregister_streaming_source(&id);
    }
    Ok(())
}

// ─── Queue Management Commands ───────────────────────────────────────────

#[tauri::command]
fn enqueue_download(
    state: State<'_, AppState>,
    id: String,
    url: String,
    path: String,
    priority: Option<String>,
    expected_checksum: Option<String>,
    custom_headers: Option<std::collections::HashMap<String, String>>,
    depends_on: Option<Vec<String>>,
    custom_segments: Option<u32>,
    group: Option<String>,
) -> Result<(), String> {
    if state.has_active_download_id(&id) {
        return Err(duplicate_download_id_error());
    }
    if state.has_active_download_url(&url) {
        return Err(duplicate_download_url_error());
    }

    let prio = priority.map(|p| queue_manager::DownloadPriority::from_str(&p))
        .unwrap_or(queue_manager::DownloadPriority::Normal);
    let settings = settings::load_settings();
    let max_retries = settings.queue_retry_max_retries;

    // If custom segments, store the override for session.rs to pick up
    if let Some(segs) = custom_segments {
        let segs = segs.clamp(1, 64);
        let mut overrides = queue_manager::DOWNLOAD_OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
        overrides.insert(id.clone(), queue_manager::DownloadOverrides { custom_segments: Some(segs), group: None });
    }

    let item = queue_manager::QueuedDownload {
        id,
        url,
        path,
        priority: prio,
        added_at: chrono::Utc::now().timestamp_millis(),
        custom_headers,
        expected_checksum,
        fresh_restart: false,
        retry_count: 0,
        max_retries,
        retry_delay_ms: 0,
        depends_on: depends_on.unwrap_or_default(),
        custom_segments,
        group,
    };

    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    if queue.contains(&item.id) {
        return Err(duplicate_download_id_error());
    }
    if queue.contains_url(&item.url) {
        return Err(duplicate_download_url_error());
    }
    let _ = queue.enqueue(item);
    drop(queue);
    queue_manager::persist_queue();
    Ok(())
}

#[tauri::command]
async fn update_download_url(
    id: String,
    new_url: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // 1. Fetch from persistence to check status and size
    let mut all_downloads = persistence::load_downloads()?;
    
    let mut download_to_update = None;
    for d in all_downloads.iter_mut() {
        if d.id == id {
            download_to_update = Some(d);
            break;
        }
    }
    
    let download = download_to_update.ok_or_else(|| "Download not found".to_string())?;
    
    // Safety check: Don't allow updating URL if it's currently downloading
    if download.status == "Downloading" || download.status == "Initializing" {
        return Err("Please pause the download before updating its address".to_string());
    }
    
    let original_size = download.total_size;
    
    // 2. Head check the new URL
    let config = crate::downloader::http_client::HttpClientConfig::default();
    let client = crate::downloader::http_client::build_stealth_client(&config)
        .map_err(|e| format!("Client error: {}", e))?;
    
    let scout = crate::downloader::http_client::FirstByteScout::new(client);
    let caps = scout.probe(&new_url).await.map_err(|e| format!("Failed to probe new URL: {}", e))?;
    
    // 3. Verify size matches if we knew the original size
    if original_size > 0 && caps.content_length.unwrap_or(0) > 0 {
        let new_size = caps.content_length.unwrap();
        if original_size != new_size {
            return Err(format!(
                "Size mismatch: The new URL points to a file of {} bytes, but the original was {} bytes. \
                This would cause corruption. Please ensure the link is for the exact same file.", 
                new_size, original_size
            ));
        }
    }
    
    // 4. Update the URL
    download.url = new_url.clone();
    
    // 5. Save back to disk
    persistence::save_downloads(&all_downloads)?;
    
    // 6. Update in-memory state
    if let Ok(mut map) = state.downloads.lock() {
        if let Some(in_mem) = map.get_mut(&id) {
            in_mem.url = new_url;
        }
    }
    
    // Emit refresh event
    let _ = app.emit("downloads_refresh", ());
    let _ = app.emit("queue_updated", ());
    
    Ok(())
}
    
    // ─── Queue Pause / Resume / Dependencies / Per-Download Overrides ─────────

#[tauri::command]
fn pause_download_queue() -> Result<(), String> {
    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.pause();
    Ok(())
}

#[tauri::command]
fn resume_download_queue() -> Result<(), String> {
    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.resume();
    Ok(())
}

#[tauri::command]
fn add_download_dependency(download_id: String, depends_on_id: String) -> Result<bool, String> {
    // Prevent circular dependency: A → B → A
    {
        let queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        // Simple cycle check: if depends_on_id itself depends on download_id
        let status = queue.status();
        for item in &status.queued_items {
            if item.id == depends_on_id && item.depends_on.contains(&download_id) {
                return Err("Circular dependency detected".to_string());
            }
        }
    }
    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let result = queue.add_dependency(&download_id, &depends_on_id);
    drop(queue);
    queue_manager::persist_queue();
    Ok(result)
}

#[tauri::command]
fn remove_download_dependency(download_id: String, depends_on_id: String) -> Result<bool, String> {
    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let result = queue.remove_dependency(&download_id, &depends_on_id);
    drop(queue);
    queue_manager::persist_queue();
    Ok(result)
}

#[tauri::command]
fn enqueue_download_chain(
    state: State<'_, AppState>,
    downloads: Vec<serde_json::Value>,
) -> Result<Vec<String>, String> {
    // Enqueue a chain of downloads where each depends on the previous one.
    // Input: [{id, url, path, priority?, expected_checksum?, custom_segments?, group?}, ...]
    // Returns: list of IDs in chain order.
    let settings = settings::load_settings();
    let max_retries = settings.queue_retry_max_retries;
    let mut ids = Vec::new();
    let mut prev_id: Option<String> = None;

    let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());

    for dl_json in &downloads {
        let id = dl_json["id"].as_str().unwrap_or("").to_string();
        let url = dl_json["url"].as_str().unwrap_or("").to_string();
        let path = dl_json["path"].as_str().unwrap_or("").to_string();

        if id.is_empty() || url.is_empty() || path.is_empty() {
            return Err("Each download must have id, url, and path".to_string());
        }
        if state.has_active_download_id(&id) || queue.contains(&id) {
            return Err(format!("Duplicate download ID: {}", id));
        }

        let prio = dl_json["priority"].as_str()
            .map(|p| queue_manager::DownloadPriority::from_str(p))
            .unwrap_or(queue_manager::DownloadPriority::Normal);
        let custom_segments = dl_json["custom_segments"].as_u64().map(|v| v as u32);
        let group = dl_json["group"].as_str().map(|s| s.to_string());

        let mut depends_on = Vec::new();
        if let Some(prev) = &prev_id {
            depends_on.push(prev.clone());
        }

        let item = queue_manager::QueuedDownload {
            id: id.clone(),
            url,
            path,
            priority: prio,
            added_at: chrono::Utc::now().timestamp_millis(),
            custom_headers: None,
            expected_checksum: dl_json["expected_checksum"].as_str().map(|s| s.to_string()),
            fresh_restart: false,
            retry_count: 0,
            max_retries,
            retry_delay_ms: 0,
            depends_on,
            custom_segments,
            group,
        };

        queue.enqueue(item);
        ids.push(id.clone());
        prev_id = Some(id);
    }

    drop(queue);
    queue_manager::persist_queue();
    Ok(ids)
}

#[tauri::command]
fn set_download_segments(download_id: String, segments: u32) -> Result<(), String> {
    let segments = segments.clamp(1, 64);
    let mut overrides = queue_manager::DOWNLOAD_OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let entry = overrides.entry(download_id).or_insert_with(queue_manager::DownloadOverrides::default);
    entry.custom_segments = Some(segments);
    Ok(())
}

// ─── Bandwidth History Commands ──────────────────────────────────────────

#[tauri::command]
fn get_bandwidth_history(since_secs: Option<u64>) -> bandwidth_history::BandwidthStats {
    bandwidth_history::get_stats(since_secs.unwrap_or(3600))
}

#[tauri::command]
fn get_bandwidth_samples(since_secs: u64) -> Vec<bandwidth_history::SpeedSample> {
    bandwidth_history::get_samples(since_secs)
}

// ─── Integrity Verification Commands ─────────────────────────────────────

#[tauri::command]
async fn verify_download_checksum(path: String, expected: String) -> Result<integrity::ChecksumResult, String> {
    integrity::verify_file_checksum(&path, &expected).await
}

#[tauri::command]
async fn compute_file_checksums(path: String) -> Result<Vec<integrity::ChecksumResult>, String> {
    integrity::compute_all_checksums(&path).await
}

#[tauri::command]
async fn compute_file_hash(path: String, algorithm: String) -> Result<String, String> {
    let algo = match algorithm.to_lowercase().as_str() {
        "sha256" | "sha-256" => integrity::HashAlgorithm::SHA256,
        "md5" => integrity::HashAlgorithm::MD5,
        "crc32" => integrity::HashAlgorithm::CRC32,
        _ => return Err(format!("Unsupported algorithm: {}. Use sha256, md5, or crc32.", algorithm)),
    };
    integrity::compute_file_hash(&path, algo).await
}

// ─── Network Monitor Commands ────────────────────────────────────────────

#[tauri::command]
async fn check_network_status() -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;

    match client.head("http://www.gstatic.com/generate_204").send().await {
        Ok(resp) => Ok(resp.status().is_success() || resp.status().as_u16() == 204),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
fn get_settings() -> serde_json::Value {
    let s = settings::load_settings();
    serde_json::to_value(s).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
fn get_auth_token() -> Result<String, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;
    let token_path = home.join(".hyperstream").join("auth_token");
    std::fs::read_to_string(&token_path).map_err(|e| format!("Failed to read auth token: {}", e))
}

// Writes a native messaging host manifest to common locations.  The
// extension must still populate "allowed_origins" with its ID(s) after
// installation.
#[tauri::command]
fn install_native_host() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy();
    let manifest = serde_json::json!({
        "name": "com.hyperstream",
        "description": "HyperStream native messaging host",
        "path": exe_str,
        "type": "stdio",
        "allowed_origins": Vec::<String>::new()
    });
    let manifest_str = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    let locations = if cfg!(target_os = "windows") {
        if let Some(local) = dirs::data_local_dir() {
            vec![local.join("Google/Chrome/User Data/NativeMessagingHosts/com.hyperstream.json")]
        } else { vec![] }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            vec![home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts/com.hyperstream.json")]
        } else { vec![] }
    } else {
        if let Some(home) = dirs::home_dir() {
            vec![home.join(".config/google-chrome/NativeMessagingHosts/com.hyperstream.json"), home.join(".config/chromium/NativeMessagingHosts/com.hyperstream.json")]
        } else { vec![] }
    };
    for path in locations.iter() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(path, &manifest_str) {
            return Err(format!("Failed to write manifest {}: {}", path.display(), e));
        }
    }
    Ok(format!("Manifest written to {} (edit allowed_origins manually)",
        locations.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")))
}

#[tauri::command]
async fn save_settings(
    settings: serde_json::Value,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    let new_settings: settings::Settings = serde_json::from_value(settings).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    // Update speed limiter when settings change
    speed_limiter::GLOBAL_LIMITER.set_limit(new_settings.speed_limit_kbps * 1024);
    if let Some(tm) = state.torrent_manager.as_ref() {
        tm.set_session_download_limit_kbps(new_settings.speed_limit_kbps);
    }
    // Update clipboard monitor
    clipboard::CLIPBOARD_MONITOR.set_enabled(new_settings.clipboard_monitor);
    state.connection_manager.set_default_limit(new_settings.max_connections_per_host.max(1).min(64) as usize);
    settings::save_settings(&new_settings)?;

    if let Some(tm) = state.torrent_manager.as_ref() {
        if let Err(e) = enforce_torrent_policies(tm.as_ref(), &new_settings).await {
            warnings.push(format!("Policy enforcement warning: {}", e));
            emit_torrent_action_warning(&app, "settings_policy", None, &e);
        }
        emit_torrent_refresh(&app);
    }

    Ok(warnings)
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    // Validate path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(&path) {
        if !canon.starts_with(&download_dir) {
            return Err("Path must be within the download directory".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Use explorer.exe directly instead of cmd /c start to avoid command injection
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn select_directory() -> Result<String, String> {
    let dialog = rfd::FileDialog::new()
        .set_title("Select Download Directory");
    
    match dialog.pick_folder() {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Err("No directory selected".to_string()),
    }
}

#[tauri::command]
fn select_file(filter: Option<String>) -> Result<String, String> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Select File");
    
    if let Some(ext) = filter {
        dialog = dialog.add_filter("Audio", &[&ext]);
    }
    
    match dialog.pick_file() {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Err("No file selected".to_string()),
    }
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    // Validate path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(&path) {
        if !canon.starts_with(&download_dir) {
            return Err("Path must be within the download directory".to_string());
        }
    }

    let folder = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn schedule_download(
    id: String, 
    url: String, 
    filename: String, 
    scheduled_time: String,
    stop_time: Option<String>,
    end_action: Option<String>
) -> Result<(), String> {
    scheduler::add_scheduled_download(scheduler::ScheduledDownload {
        id,
        url,
        filename,
        scheduled_time,
        stop_time,
        end_action,
        status: "pending".to_string(),
    });
    Ok(())
}

// ============ Spider / Site Grabber Commands ============

#[tauri::command]
async fn crawl_website(
    url: String, 
    max_depth: u32, 
    extensions: Vec<String>
) -> Result<Vec<spider::GrabbedFile>, String> {
    // Cap max_depth to prevent excessive crawling
    let max_depth = max_depth.min(5);
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0")
        .timeout(std::time::Duration::from_secs(15))
        // Use a custom redirect policy that re-validates each hop against SSRF rules.
        // Without this, an attacker could host a public page that 302-redirects to
        // http://169.254.169.254/ or http://127.0.0.1:8080/ and the spider would follow it.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.stop()
            } else {
                // Clone host to avoid borrowing `attempt` across the move
                let host = attempt.url().host_str().map(|h| h.to_string());
                if let Some(host) = host {
                    let h = host.to_lowercase();
                    if h == "localhost" || h.ends_with(".local") || h.ends_with(".internal") {
                        attempt.error(anyhow::anyhow!("Redirect to private host blocked: {}", host))
                    } else if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                        let is_private = match ip {
                            std::net::IpAddr::V4(v4) => {
                                v4.is_loopback() || v4.is_private() || v4.is_link_local()
                                    || v4.is_broadcast() || v4.is_unspecified() || v4.octets()[0] == 0
                            }
                            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
                        };
                        if is_private {
                            attempt.error(anyhow::anyhow!("Redirect to private IP blocked: {}", ip))
                        } else {
                            attempt.follow()
                        }
                    } else {
                        attempt.follow()
                    }
                } else {
                    attempt.follow()
                }
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    
    let spider = spider::Spider::new(client);
    spider.crawl(spider::SpiderOptions {
        url,
        max_depth,
        same_domain: true,
        extensions,
    }).await
}


// ============ ZIP Preview Commands ============

#[tauri::command]
fn preview_zip_partial(data: Vec<u8>) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip_partial(&data)
}

#[tauri::command]
fn preview_zip_file(path: String) -> Result<zip_preview::ZipPreview, String> {
    // Validate path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(&path) {
        if !canon.starts_with(&download_dir) {
            return Err("Path must be within the download directory".to_string());
        }
    }
    zip_preview::preview_zip(std::path::Path::new(&path))
}

#[tauri::command]
fn extract_single_file(zip_path: String, entry_name: String, dest_path: String) -> Result<(), String> {
    // Validate both paths are within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(&zip_path) {
        if !canon.starts_with(&download_dir) {
            return Err("Zip path must be within the download directory".to_string());
        }
    }
    // Normalize dest_path to prevent traversal
    let dest = std::path::Path::new(&dest_path);
    let abs_dest = if dest.is_absolute() { dest.to_path_buf() } else { download_dir.join(dest) };
    let mut normalized = std::path::PathBuf::new();
    for component in abs_dest.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Destination must be within the download directory".to_string());
    }
    zip_preview::extract_file(
        std::path::Path::new(&zip_path),
        &entry_name,
        &normalized
    )
}

#[tauri::command]
async fn preview_zip_remote(url: String) -> Result<zip_preview::ZipPreview, String> {
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    zip_preview::preview_zip_remote(url, client).await
}

#[tauri::command]
async fn download_zip_entry(url: String, entry_name: String, dest_path: String) -> Result<(), String> {
    // Validate URL scheme
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {},
        s => return Err(format!("Unsupported URL scheme: {}", s)),
    }

    // Validate dest_path stays within the download directory using path normalization
    // (canonicalize fails on non-existent paths, so we normalize manually)
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    let dest = std::path::Path::new(&dest_path);
    let abs_dest = if dest.is_absolute() {
        dest.to_path_buf()
    } else {
        download_dir.join(dest)
    };
    // Normalize path: resolve .. components without filesystem access
    let mut normalized = std::path::PathBuf::new();
    for component in abs_dest.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Destination path must be within the download directory".to_string());
    }

    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = zip_preview::download_entry_remote(url, entry_name, client).await?;
    // Write to the normalized path to prevent traversal
    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&normalized, bytes).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_zip_last_bytes(path: String, length: usize) -> Result<Vec<u8>, String> {
    // Cap read length to 10MB
    const MAX_READ: usize = 10 * 1024 * 1024;
    if length > MAX_READ {
        return Err(format!("Read length {} exceeds max {} bytes", length, MAX_READ));
    }
    // Validate path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    let canon = dunce::canonicalize(&path).map_err(|e| e.to_string())?;
    if !canon.starts_with(&download_dir) {
        return Err("Path must be within the download directory".to_string());
    }
    zip_preview::read_last_bytes(&canon, length)
}


// ============ HLS/DASH Stream Parser Commands ============

#[tauri::command]
async fn init_tor_network() -> Result<u16, String> {
    network::tor::init_tor().await
}

#[tauri::command]
fn get_tor_status() -> Option<u16> {
    network::tor::get_socks_port()
}

#[tauri::command]
async fn perform_semantic_search(query: String) -> Result<Vec<ai::SearchResult>, String> {
    ai::semantic_search(&query)
}

#[tauri::command]
async fn index_all_downloads() -> Result<usize, String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut count = 0;
    
    // Spawn task to avoid blocking main thread too long, though we await it here for result
    // Ideally should be background.
    for d in downloads {
        if d.status == "Complete" {
             if let Ok(_) = ai::index_file(&d.path) {
                 count += 1;
             }
        }
    }
    Ok(count)
}

#[tauri::command]
async fn join_workspace(host_ip: String) -> Result<(), String> {
    network::sync_client::connect_to_workspace(host_ip).await
}


#[tauri::command]
async fn parse_hls_stream(url: String) -> Result<media::HlsStream, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| e.to_string())?;
    
    let parser = media::HlsParser::new(client);
    parser.parse(&url).await
}

/// Fetch an HLS master playlist and return the available quality variants.
/// The frontend calls this to populate the quality-picker dialog before starting
/// the actual download.  Works for both master playlists (returns multiple
/// variants) and plain media playlists (returns a single synthetic entry).
#[tauri::command]
async fn probe_hls_variants(url: String) -> Result<Vec<media::HlsVariant>, String> {
    engine::hls::probe_hls_url_variants(&url).await
}

#[tauri::command]
fn parse_dash_manifest(content: String, base_url: String) -> Result<media::dash_parser::DashManifest, String> {
    media::dash_parser::parse_mpd(&content, &base_url)
}

#[tauri::command]
async fn fetch_dash_manifest(url: String) -> Result<media::dash_parser::DashManifest, String> {
    let settings = settings::load_settings();
    let proxy_config = proxy::ProxyConfig::from_settings(&settings);
    let client = if settings.dpi_evasion {
        network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config), None)
    } else {
        network::masq::build_client(Some(&proxy_config), None)
    }
    .map_err(|e| e.to_string())?;

    let body = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch MPD: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read MPD body: {}", e))?;

    media::dash_parser::parse_mpd(&body, &url)
}

// ============ Multi-Source / Mirror Download Commands ============

#[tauri::command]
async fn start_multi_source_download(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    primary_url: String,
    mirrors: Vec<(String, String)>,
    path: String,
    custom_headers: Option<std::collections::HashMap<String, String>>,
    expected_checksum: Option<String>,
) -> Result<(), String> {
    if state.has_active_download_id(&id) {
        return Err(duplicate_download_id_error());
    }
    if state.has_active_download_url(&primary_url) {
        return Err(duplicate_download_url_error());
    }

    {
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        if queue.contains(&id) {
            return Err(duplicate_download_id_error());
        }
        if queue.contains_url(&primary_url) {
            return Err(duplicate_download_url_error());
        }
        queue.mark_active(&id, &primary_url);
    }

    {
        let mut meta = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
        meta.insert(id.clone(), queue_manager::RetryMetadata {
            url: primary_url.clone(),
            path: path.clone(),
            priority: queue_manager::DownloadPriority::Normal,
            custom_headers: custom_headers.clone(),
            expected_checksum: expected_checksum.clone(),
            fresh_restart: false,
            retry_count: 0,
            max_retries: 0,
        });
    }

    let result = engine::multi_source::start_multi_source_download(
        &app, &state, id.clone(), primary_url, mirrors, path, custom_headers, expected_checksum,
    ).await;

    if let Err(_) = result {
        crate::engine::session::clear_retry_metadata(&id);
        let mut queue = queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.mark_finished(&id);
    }

    result
}

#[tauri::command]
async fn probe_mirrors(
    primary_url: String,
    mirror_urls: Vec<(String, String)>,
) -> Result<Vec<engine::multi_source::MirrorStats>, String> {
    let settings = settings::load_settings();
    let proxy_config = proxy::ProxyConfig::from_settings(&settings);
    let client = if settings.dpi_evasion {
        network::masq::build_impersonator_client(
            network::masq::BrowserProfile::Chrome, Some(&proxy_config), None,
        )
    } else {
        network::masq::build_client(Some(&proxy_config), None)
    }.map_err(|e| e.to_string())?;

    let pool = engine::multi_source::MirrorPool::new(&primary_url, &mirror_urls);
    pool.probe_all(&client).await;
    Ok(pool.get_stats())
}

#[tauri::command]
fn check_ffmpeg_available() -> bool {
    media::muxer::is_ffmpeg_available()
}

// ============ Download History Commands ============

#[tauri::command]
fn get_download_history(filter: download_history::HistoryFilter) -> download_history::HistoryPage {
    download_history::query(&filter)
}

#[tauri::command]
fn search_download_history(query: String, limit: Option<usize>) -> Vec<download_history::HistoryEntry> {
    download_history::search(&query, limit.unwrap_or(100))
}

#[tauri::command]
fn clear_download_history() -> Result<(), String> {
    download_history::clear()
}

#[tauri::command]
fn delete_history_entry(id: String) -> Result<(), String> {
    download_history::delete_entry(&id)
}

#[tauri::command]
fn export_download_history_csv() -> Result<String, String> {
    download_history::export_csv()
}

#[tauri::command]
fn get_history_summary() -> download_history::HistorySummary {
    download_history::summary()
}

// ============ Activity Log Commands ============

#[tauri::command]
fn get_activity_log(
    app: tauri::AppHandle,
    limit: Option<usize>,
    event_type: Option<String>,
) -> Result<Vec<event_sourcing::LedgerEvent>, String> {
    let log = app.state::<std::sync::Arc<event_sourcing::SharedLog>>();
    log.read_recent(
        limit.unwrap_or(200),
        event_type.as_deref(),
    )
}

// ============ CAS Duplicate Detection Commands ============

#[tauri::command]
fn check_cas_duplicate(etag: Option<String>, md5: Option<String>) -> Option<String> {
    cas_manager::check_cas(etag.as_deref(), md5.as_deref())
}

#[tauri::command]
fn register_cas_entry(etag: Option<String>, md5: Option<String>, path: String) {
    cas_manager::register_cas(etag.as_deref(), md5.as_deref(), &path);
}

#[derive(serde::Serialize)]
struct HeadUrlMetadata {
    etag: Option<String>,
    content_md5: Option<String>,
}

#[tauri::command]
async fn head_url_metadata(url: String) -> Result<HeadUrlMetadata, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.head(&url).send().await.map_err(|e| e.to_string())?;
    let headers = resp.headers();
    let etag = headers.get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let content_md5 = headers.get("content-md5").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    Ok(HeadUrlMetadata { etag, content_md5 })
}

// ============ Site Rules Commands ============

#[tauri::command]
fn list_site_rules() -> Vec<site_rules::SiteRule> {
    site_rules::list_rules()
}

#[tauri::command]
fn get_site_rule(id: String) -> Result<site_rules::SiteRule, String> {
    site_rules::get_rule(&id).ok_or_else(|| format!("Rule '{}' not found", id))
}

#[tauri::command]
fn add_site_rule(rule: site_rules::SiteRule) -> Result<(), String> {
    site_rules::add_rule(rule)
}

#[tauri::command]
fn update_site_rule(rule: site_rules::SiteRule) -> Result<(), String> {
    site_rules::update_rule(rule)
}

#[tauri::command]
fn delete_site_rule(id: String) -> Result<(), String> {
    site_rules::delete_rule(&id)
}

#[tauri::command]
fn import_site_rule_presets() -> Result<usize, String> {
    site_rules::import_presets()
}

#[tauri::command]
fn test_site_rule(url: String) -> site_rules::EffectiveConfig {
    site_rules::test_url(&url)
}

#[tauri::command]
fn get_site_rule_presets() -> Vec<site_rules::SiteRule> {
    site_rules::builtin_presets()
}

// ============ File Categorizer Commands ============

#[tauri::command]
fn list_file_categories() -> Vec<file_categorizer::FileCategory> {
    file_categorizer::list_categories()
}

#[tauri::command]
fn get_file_category(id: String) -> Result<file_categorizer::FileCategory, String> {
    file_categorizer::get_category(&id).ok_or_else(|| format!("Category '{}' not found", id))
}

#[tauri::command]
fn add_file_category(category: file_categorizer::FileCategory) -> Result<(), String> {
    file_categorizer::add_category(category)
}

#[tauri::command]
fn update_file_category(category: file_categorizer::FileCategory) -> Result<(), String> {
    file_categorizer::update_category(category)
}

#[tauri::command]
fn delete_file_category(id: String) -> Result<(), String> {
    file_categorizer::delete_category(&id)
}

#[tauri::command]
fn categorize_file(filename: String) -> file_categorizer::CategorizeResult {
    file_categorizer::categorize(&filename)
}

#[tauri::command]
fn categorize_files_batch(filenames: Vec<String>) -> Vec<file_categorizer::CategorizeResult> {
    file_categorizer::categorize_batch(&filenames)
}

#[tauri::command]
fn get_file_category_stats(download_dir: String) -> Vec<file_categorizer::CategoryStats> {
    file_categorizer::compute_stats(&download_dir)
}

#[tauri::command]
fn reset_file_categories() -> Result<(), String> {
    file_categorizer::reset_to_defaults()
}

// ============ Bandwidth Allocator Commands ============

#[tauri::command]
fn register_download_bandwidth(id: String, config: bandwidth_allocator::BandwidthConfig) {
    bandwidth_allocator::ALLOCATOR.register(&id, config);
}

#[tauri::command]
fn deregister_download_bandwidth(id: String) {
    bandwidth_allocator::ALLOCATOR.deregister(&id);
}

#[tauri::command]
fn get_bandwidth_allocations() -> Vec<bandwidth_allocator::AllocationSnapshot> {
    bandwidth_allocator::ALLOCATOR.snapshot()
}

#[tauri::command]
fn rebalance_bandwidth() {
    bandwidth_allocator::ALLOCATOR.rebalance();
}

// ============ Crash Recovery Commands ============

#[tauri::command]
fn scan_crashed_downloads() -> Result<crash_recovery::RecoveryReport, String> {
    crash_recovery::scan_and_recover()
}

#[tauri::command]
fn get_interrupted_downloads() -> Result<Vec<persistence::SavedDownload>, String> {
    crash_recovery::get_interrupted()
}

#[tauri::command]
fn resume_interrupted_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    crash_recovery::resume_one(&app, &id)
}

#[tauri::command]
fn resume_all_interrupted(app: tauri::AppHandle) -> Result<u32, String> {
    let interrupted = crash_recovery::get_interrupted()?;
    let count = interrupted.len() as u32;
    for dl in &interrupted {
        let _ = crash_recovery::resume_one(&app, &dl.id);
    }
    Ok(count)
}

// ============ Muxer Commands ============

#[tauri::command]
async fn mux_video_audio(video_path: String, audio_path: String, output_path: String) -> Result<(), String> {
    // Validate all paths are within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    for (label, p) in [("Video", &video_path), ("Audio", &audio_path)] {
        if let Ok(canon) = dunce::canonicalize(p) {
            if !canon.starts_with(&download_dir) {
                return Err(format!("{} path must be within the download directory", label));
            }
        }
    }
    // Normalize output path (may not exist yet)
    let out = std::path::Path::new(&output_path);
    let abs_out = if out.is_absolute() { out.to_path_buf() } else { download_dir.join(out) };
    let mut normalized = std::path::PathBuf::new();
    for component in abs_out.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Output path must be within the download directory".to_string());
    }

    media::muxer::merge_streams(
        std::path::Path::new(&video_path),
        std::path::Path::new(&audio_path),
        &normalized
    )
}

#[tauri::command]
fn check_ffmpeg_installed() -> bool {
    media::muxer::is_ffmpeg_available()
}

#[tauri::command]
fn decrypt_aes_128(input_path: String, output_path: String, key_hex: String, iv_hex: String) -> Result<(), String> {
    // Validate both paths are within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));

    let abs_input = dunce::canonicalize(&input_path)
        .map_err(|e| format!("Invalid input path: {}", e))?;
    if !abs_input.starts_with(&download_dir) {
        return Err("Input path must be within the download directory".to_string());
    }

    let abs_output_parent = dunce::canonicalize(
        std::path::Path::new(&output_path).parent().unwrap_or(std::path::Path::new("."))
    ).unwrap_or_else(|_| std::path::PathBuf::from(&output_path));
    if !abs_output_parent.starts_with(&download_dir) {
        return Err("Output path must be within the download directory".to_string());
    }

    let key = media::decrypt::decode_hex(&key_hex)?;
    let iv = media::decrypt::decode_hex(&iv_hex)?;
    
    let encrypted_data = std::fs::read(&input_path).map_err(|e| e.to_string())?;
    let decrypted = media::decrypt::decrypt_aes128(&encrypted_data, &key, &iv)?;
    
    std::fs::write(&output_path, decrypted).map_err(|e| e.to_string())?;
    Ok(())
}


#[tauri::command]
async fn test_browser_fingerprint() -> Result<String, String> {
    let settings = settings::load_settings();
    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);

    // Enable DPI evasion for the test to verify headers
    let client = network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config), None)
        .map_err(|e| e.to_string())?;
    
    // Hit a trace URL (using httpbin for now to show headers)
    let resp = client.get("https://httpbin.org/headers")
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

// ============ Proxy Configuration Commands ============

#[tauri::command]
fn get_proxy_config() -> serde_json::Value {
    let settings = settings::load_settings();
    let config = proxy::ProxyConfig::from_settings(&settings);
    serde_json::to_value(config).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
async fn test_proxy(config: proxy::ProxyConfig) -> Result<bool, String> {
    // Use rquest because it's our main driver
    let client = rquest::Client::builder()
        .proxy(config.to_rquest_proxy().ok_or("Invalid Proxy Config")?)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Verify connectivity
    let _ = client.head("https://www.google.com")
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;
        
    Ok(true)
}

// ============ Virtual Drive Commands ============
// #[tauri::command]
// async fn mount_drive(id: String, path: String) -> Result<u16, String> {
//    virtual_drive::DRIVE_MANAGER.mount(id, path).await
// }

// #[tauri::command]
// async fn unmount_drive(id: String) -> Result<(), String> {
//    virtual_drive::DRIVE_MANAGER.unmount(id)
// }

// ============ Cloud Commands ============
#[tauri::command]
async fn upload_to_cloud(app_handle: tauri::AppHandle, path: String, target_name: Option<String>) -> Result<String, String> {
    let settings = settings::load_settings();

    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid path")?;
        
    let key = target_name.unwrap_or(filename.to_string());
    
    // Construct full path if simple filename given?
    // Usually 'path' from frontend 'task.filename' might be just name.
    // Need to resolve.
    let full_path = if std::path::Path::new(&path).is_absolute() {
        std::path::PathBuf::from(&path)
    } else {
         let download_dir = &settings.download_dir;
         // Resolve relative to download dir.
         // Need to handle user directory expansion if needed, but assuming absolute or simple join.
         std::path::PathBuf::from(download_dir).join(&path)
    };
    
    // Check if file exists there, if not check Desktop (legacy default)
    let final_path = if full_path.exists() {
        full_path
    } else {
         // Fallback to Desktop construction as seen in other parts
         // This is hacky but matches 'open_folder' logic seen previously
         let mut p = dirs::desktop_dir().ok_or("No desktop")?;
         p.push(&path);
         p
    };

    cloud_bridge::CloudBridge::upload_file_with_progress(&settings, final_path.to_str().ok_or_else(|| "Invalid path encoding".to_string())?, &key, &app_handle).await
}

// ============ Media Commands ============
#[tauri::command]
async fn process_media(_app_handle: tauri::AppHandle, path: String, action: String) -> Result<String, String> {
    // action: "check", "preview", "audio"
    if action == "check" {
        return if media_processor::MediaProcessor::check_ffmpeg() {
            Ok("Available".to_string())
        } else {
            Err("FFmpeg not found".to_string())
        };
    }

    let settings = settings::load_settings();

    let final_path = resolve_download_path(&path, &settings.download_dir)?;
    
    let input_str = final_path.to_str().ok_or_else(|| "Invalid path encoding".to_string())?;

    match action.as_str() {
        "preview" => {
            let output_path = final_path.with_extension("webp");
            media_processor::MediaProcessor::generate_preview(input_str, output_path.to_str().ok_or_else(|| "Invalid output path encoding".to_string())?)
        },
        "audio" => {
            let output_path = final_path.with_extension("mp3");
            media_processor::MediaProcessor::extract_audio(input_str, output_path.to_str().ok_or_else(|| "Invalid output path encoding".to_string())?)
        },
        _ => Err("Unknown action".to_string())
    }
}


// ============ Import/Export Commands (Disabled) ============
/*
#[tauri::command]
fn export_downloads(path: String) -> Result<(), String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut export = import_export::HyperStreamExport::new();
    
    for d in downloads {
        export.downloads.push(import_export::ExportedDownload {
            url: d.url,
            filename: d.filename,
            save_path: d.path,
            category: None,
            total_size: d.total_size,
            downloaded_bytes: d.downloaded_bytes,
            status: d.status,
            added_at: String::new(), // Not stored in SavedDownload
        });
    }
    
    export.to_json_file(std::path::Path::new(&path))
}

#[tauri::command]
fn import_downloads(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    let export = import_export::HyperStreamExport::from_json_file(std::path::Path::new(&path))?;
    Ok(export.downloads)
}

#[tauri::command]
fn import_from_idm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_idm(std::path::Path::new(&path))
}

#[tauri::command]
fn export_downloads_csv(path: String) -> Result<(), String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut export = import_export::HyperStreamExport::new();
    
    for d in downloads {
        export.downloads.push(import_export::ExportedDownload {
            url: d.url,
            filename: d.filename,
            save_path: d.path,
            category: None,
            total_size: d.total_size,
            downloaded_bytes: d.downloaded_bytes,
            status: d.status,
            added_at: String::new(),
        });
    }
    
    export.to_csv_file(std::path::Path::new(&path))
}
*/


// ============ Virus Scanning Commands (Disabled) ============
/*
#[tauri::command]
async fn scan_file_for_virus(path: String) -> Result<String, String> {
    let scanner = virus_scanner::VirusScanner::new();
    if !scanner.is_available() {
        return Ok("Scanner not available".to_string());
    }
    
    let result = scanner.scan_file(std::path::Path::new(&path)).await;
    match result {
        virus_scanner::ScanResult::Clean => Ok("Clean".to_string()),
        virus_scanner::ScanResult::Infected { threat_name } => Ok(format!("Infected: {}", threat_name)),
        virus_scanner::ScanResult::Error { message } => Err(message),
        virus_scanner::ScanResult::NotScanned => Ok("Not scanned".to_string()),
    }
}

#[tauri::command]
fn is_antivirus_available() -> bool {
    virus_scanner::VirusScanner::new().is_available()
}
*/

// ============ Speed Limiter Commands ============

#[tauri::command]
async fn acquire_bandwidth(amount: u32) -> Result<(), String> {
    speed_limiter::GLOBAL_LIMITER.acquire(amount as u64).await;
    Ok(())
}

#[tauri::command]
fn set_speed_limit(limit_kbps: u64, state: State<'_, AppState>) {
    speed_limiter::GLOBAL_LIMITER.set_limit(limit_kbps * 1024);
    if let Some(tm) = state.torrent_manager.as_ref() {
        tm.set_session_download_limit_kbps(limit_kbps);
    }
}

#[tauri::command]
fn get_speed_limit() -> u64 {
    speed_limiter::GLOBAL_LIMITER.get_limit() / 1024
}

#[tauri::command]
fn get_active_speed_profile() -> Option<settings::SpeedProfile> {
    let settings = settings::load_settings();
    if !settings.speed_profiles_enabled || settings.speed_profiles.is_empty() {
        return None;
    }
    let now = chrono::Local::now();
    use chrono::{Timelike, Datelike};
    let current_minutes = now.hour() as u16 * 60 + now.minute() as u16;
    let current_day = now.weekday().num_days_from_monday() as u8;
    settings.speed_profiles.iter().find(|p| {
        speed_profiles::profile_matches_pub(p, current_minutes, current_day)
    }).cloned()
}

// ============ ChatOps Commands ============

#[tauri::command]
fn get_chatops_pending_urls(state: State<'_, AppState>) -> Vec<String> {
    state.chatops_manager.take_pending_urls()
}

// ============ Clipboard Commands ============

#[tauri::command]
fn get_clipboard_monitor_enabled() -> bool {
    clipboard::CLIPBOARD_MONITOR.is_enabled()
}

// ============ LAN API Commands ============

#[tauri::command]
fn generate_lan_pairing_code() -> String {
    lan_api::LanApiServer::generate_pairing_code()
}

// ============ Advanced Subsystems ============

#[tauri::command]
fn set_qos_global_limit(limit: u64) {
    qos_manager::set_global_bandwidth_limit(limit);
}

#[tauri::command]
fn remove_geofence_rule(rule_id: String) -> Result<String, String> {
    geofence::remove_geofence_rule(rule_id)
}

#[tauri::command]
fn toggle_geofence_rule(rule_id: String) -> Result<String, String> {
    geofence::toggle_geofence_rule(rule_id)
}

#[tauri::command]
fn get_preset_regions() -> Vec<serde_json::Value> {
    geofence::get_preset_regions()
}

#[tauri::command]
async fn get_fastest_mirror(urls: Vec<String>) -> Result<String, String> {
    crate::bandwidth_arb::get_fastest_mirror(urls).await
}

#[tauri::command]
fn parse_ipfs_uri_cmd(input: String) -> Option<String> {
    crate::ipfs_gateway::parse_ipfs_uri(&input)
}

#[tauri::command]
fn get_rclone_version() -> Result<String, String> {
    crate::rclone_bridge::rclone_version()
}

#[tauri::command]
fn get_rclone_ls(remote_path: String) -> Result<String, String> {
    crate::rclone_bridge::rclone_ls(remote_path)
}


#[tauri::command]
fn get_lan_pairing_qr_data(port: u16, code: String) -> String {
    let server = lan_api::LanApiServer::new(port);
    server.get_pairing_qr_data(&code)
}

#[tauri::command]
fn get_qos_download_limit(id: String) -> u64 {
    qos_manager::get_download_limit(&id)
}

#[tauri::command]
fn update_qos_download_speed(id: String, bps: u64, total: u64) {
    qos_manager::update_download_speed(&id, bps, total);
}

#[tauri::command]
fn remove_qos_download(id: String) {
    qos_manager::remove_download(&id);
}

#[tauri::command]
fn match_geofence_cmd(url: String) -> Option<geofence::GeofenceRule> {
    geofence::match_geofence(&url)
}

#[tauri::command]
fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

// ============ Scheduler Helper Commands ============

#[tauri::command]
fn is_quiet_hours() -> bool {
    scheduler::is_quiet_hours()
}

#[tauri::command]
fn get_time_info() -> scheduler::TimeInfo {
    scheduler::get_current_time_info()
}

#[tauri::command]
fn get_scheduled_downloads() -> Vec<scheduler::ScheduledDownload> {
    scheduler::get_scheduled_downloads()
}

#[tauri::command]
fn remove_scheduled_download(id: String) {
    scheduler::remove_scheduled_download(&id);
}


#[tauri::command]
fn force_start_scheduled_download<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>, id: String) -> Result<(), String> {
    if let Some(download) = scheduler::force_start_download(&id) {
        // Emit start event immediately
        app_handle.emit("scheduled_download_start", serde_json::json!({
            "id": download.id,
            "url": download.url,
            "filename": download.filename
        })).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Download not found or already started".to_string())
    }
}



// ============ Plugin System Commands ============

#[tauri::command]
async fn get_plugin_metadata(app_handle: tauri::AppHandle, script: String) -> Result<Option<plugin_vm::lua_host::PluginMetadata>, String> {
    let client = rquest::Client::builder()
        // .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.get_plugin_metadata().await.map_err(|e| e.to_string())
}

// ============ Network Validation Commands ============

#[tauri::command]
fn analyze_http_status(status_code: u16) -> String {
    use rquest::StatusCode;
    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK);
    let strategy = downloader::network::analyze_status(status);
    format!("{:?}", strategy)
}

#[tauri::command]
fn check_captive_portal(first_bytes: Vec<u8>) -> bool {
    downloader::network::is_captive_portal(&first_bytes)
}

// ============ Disk Operation Commands ============

#[tauri::command]
fn preallocate_download_file(path: String, size: u64) -> Result<(), String> {
    // Validate path is within download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    let p = std::path::Path::new(&path);
    let abs_path = if p.is_absolute() { p.to_path_buf() } else { download_dir.join(p) };
    let mut normalized = std::path::PathBuf::new();
    for component in abs_path.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Path must be within the download directory".to_string());
    }
    // Cap preallocation to 100 GB
    const MAX_PREALLOC: u64 = 100 * 1024 * 1024 * 1024;
    if size > MAX_PREALLOC {
        return Err(format!("Preallocation size {} exceeds maximum {} bytes", size, MAX_PREALLOC));
    }
    downloader::disk::preallocate_file(&normalized, size)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

// ============ Adaptive Thread Commands ============

#[tauri::command]
fn get_adaptive_thread_count() -> u32 {
    adaptive_threads::THREAD_CONTROLLER.get_threads()
}

#[tauri::command]
fn update_thread_count(current_speed: u64, max_speed: u64) -> u32 {
    adaptive_threads::THREAD_CONTROLLER.update(current_speed, max_speed)
}

#[tauri::command]
fn add_bandwidth_sample(bytes: u64) {
    adaptive_threads::BANDWIDTH_MONITOR.add_sample(bytes);
}

#[tauri::command]
fn get_average_bandwidth() -> u64 {
    adaptive_threads::BANDWIDTH_MONITOR.get_average_speed()
}

// ============ More Scheduler Commands ============

#[tauri::command]
fn get_next_download_time() -> String {
    scheduler::get_next_download_time().to_rfc3339()
}

// ============ FDM Import Command (Disabled) ============
/*
#[tauri::command]
fn import_from_fdm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_fdm(std::path::Path::new(&path))
}
*/

// ============ Feeds Commands ============
#[tauri::command]
async fn fetch_feed(url: String) -> Result<Vec<feeds::FeedItem>, String> {
    feeds::fetch_feed(&url).await
}

#[tauri::command]
async fn perform_search(
    query: String,
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>,
) -> Result<Vec<search::SearchResult>, String> {
    let pm = pm.inner().clone();
    tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async move {
            pm.search(&query).await
        })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_feeds() -> Vec<feeds::FeedConfig> {
    feeds::FEED_MANAGER.get_feeds()
}

#[tauri::command]
fn get_feed_items(feed_id: String) -> Vec<feeds::FeedItem> {
    feeds::FEED_MANAGER.get_items(&feed_id)
}

#[tauri::command]
fn mark_feed_item_read(feed_id: String, link: String) {
    feeds::FEED_MANAGER.mark_item_read(&feed_id, &link);
}

#[tauri::command]
fn add_feed(config: feeds::FeedConfig) -> Result<(), String> {
    feeds::FEED_MANAGER.add_feed(config)
}

#[tauri::command]
fn update_feed(config: feeds::FeedConfig) -> Result<(), String> {
    feeds::FEED_MANAGER.update_feed(config)
}

#[tauri::command]
fn remove_feed(id: String) {
    feeds::FEED_MANAGER.remove_feed(&id);
}

#[tauri::command]
async fn manual_refresh_feed(app: tauri::AppHandle, feed_id: String) -> Result<(), String> {
    feeds::FEED_MANAGER.refresh_feed(&app, &feed_id).await
}

// ============ Tray & Setup ============



// ============ ZIP Extraction Commands ============

#[tauri::command]
fn read_file_bytes_at_offset(path: String, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_bytes_at_offset(std::path::Path::new(&path), offset, length)
}

// ============ HTTP Client Commands ============

#[tauri::command]
fn validate_http_response(
    status_code: u16,
    content_length: Option<u64>,
    content_type: Option<String>,
    accept_ranges: Option<String>
) -> Result<(), String> {
    use rquest::StatusCode;
    let validator = downloader::network::ResponseValidator::new();
    let status = StatusCode::from_u16(status_code).map_err(|e| e.to_string())?;
    validator.validate(
        status,
        content_length,
        content_type.as_deref(),
        accept_ranges.as_deref()
    )
}

#[tauri::command]
fn parse_retry_after_header(value: String) -> Option<u64> {
    downloader::network::parse_retry_after(&value)
        .map(|d| d.as_secs())
}

#[tauri::command]
fn check_error_content_type(content_type: Option<String>, expected_type: Option<String>) -> bool {
    downloader::network::is_error_content_type(
        content_type.as_deref(),
        expected_type.as_deref()
    )
}

// ============ Stealth HTTP Client Commands ============

#[tauri::command]
fn get_chrome_user_agent() -> String {
    downloader::http_client::CHROME_USER_AGENT.to_string()
}

#[tauri::command]
fn get_default_http_config() -> serde_json::Value {
    let config = downloader::http_client::HttpClientConfig::default();
    serde_json::json!({
        "timeout_secs": config.timeout.as_secs(),
        "connect_timeout_secs": config.connect_timeout.as_secs(),
        "user_agent": config.user_agent,
        "follow_redirects": config.follow_redirects,
        "max_redirects": config.max_redirects,
        "danger_accept_invalid_certs": config.danger_accept_invalid_certs
    })
}

// ============ Retry Strategy Commands ============

#[tauri::command]
fn calculate_retry_backoff(current_delay_ms: u64) -> u64 {
    let config = downloader::network::RetryConfig::default();
    let current = std::time::Duration::from_millis(current_delay_ms);
    let next = downloader::network::calculate_backoff(current, &config);
    next.as_millis() as u64
}

#[tauri::command]
fn get_retry_config() -> serde_json::Value {
    let config = downloader::network::RetryConfig::default();
    serde_json::json!({
        "max_immediate_retries": config.max_immediate_retries,
        "max_delayed_retries": config.max_delayed_retries,
        "initial_delay_ms": config.initial_delay.as_millis() as u64,
        "max_delay_ms": config.max_delay.as_millis() as u64,
        "jitter_factor": config.jitter_factor
    })
}

#[tauri::command]
fn analyze_error_strategy(error_type: String) -> String {
    // Map common error types to retry strategies
    match error_type.as_str() {
        "timeout" => "Delayed(5s)".to_string(),
        "connection" | "connect" => "Immediate".to_string(),
        "forbidden" | "403" => "RefreshLink".to_string(),
        "not_found" | "404" => "Fatal(File Not Found)".to_string(),
        "too_many_requests" | "429" => "Delayed(30s)".to_string(),
        "server_error" | "500" => "Delayed(10s)".to_string(),
        "bad_gateway" | "502" => "Delayed(5s)".to_string(),
        "service_unavailable" | "503" => "Delayed(15s)".to_string(),
        _ => "Delayed(3s)".to_string(),
    }
}

// ============ Range Request Commands ============

#[tauri::command]
async fn start_range_download(url: String, start: u64, end: u64) -> Result<Vec<u8>, String> {
    // Validate URL scheme to prevent SSRF
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {},
        s => return Err(format!("Unsupported URL scheme: {}", s)),
    }

    // Cap range size to 50 MB to prevent OOM
    const MAX_RANGE_SIZE: u64 = 50 * 1024 * 1024;
    if end > start && (end - start) > MAX_RANGE_SIZE {
        return Err(format!("Range too large: {} bytes (max {})", end - start, MAX_RANGE_SIZE));
    }

    let settings = settings::load_settings();
    let mut config = downloader::http_client::HttpClientConfig::default();
    config.proxy = Some(crate::proxy::ProxyConfig::from_settings(&settings));
    
    let client = downloader::http_client::build_stealth_client(&config)
        .map_err(|e| e.to_string())?;
    
    let response = client
        .get(&url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RANGE_SIZE {
        return Err(format!("Response body too large: {} bytes", bytes.len()));
    }
    Ok(bytes.to_vec())
}

#[tauri::command]
async fn validate_download_url(url: String) -> Result<serde_json::Value, String> {
    let settings = settings::load_settings();
    let mut config = downloader::http_client::HttpClientConfig::default();
    config.proxy = Some(crate::proxy::ProxyConfig::from_settings(&settings));

    let client = downloader::http_client::build_client(&config)
        .map_err(|e| e.to_string())?;
    
    let scout = downloader::http_client::FirstByteScout::new(client);
    let caps = scout.probe(&url).await?;
    
    Ok(serde_json::json!({
        "supports_range": caps.supports_range,
        "valid_content": caps.valid_content,
        "content_length": caps.content_length,
        "content_type": caps.content_type,
        "etag": caps.etag,
        "last_modified": caps.last_modified,
        "recommended_segments": caps.recommended_segments,
        "ignores_range": caps.ignores_range
    }))
}

// ============ Plugin Extraction Commands ============

#[tauri::command]
async fn extract_stream_url(app_handle: tauri::AppHandle, script: String, page_url: String) -> Result<Option<serde_json::Value>, String> {
    let client = rquest::Client::builder()
        // .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    
    match host.extract_stream(&page_url).await {
        Ok(Some(result)) => Ok(Some(serde_json::json!({
            "url": result.url,
            "cookies": result.cookies,
            "headers": result.headers,
            "filename": result.filename
        }))),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ============ Proxy Bypass Commands ============

#[tauri::command]
fn should_bypass_proxy(url: String) -> bool {
    let config = proxy::ProxyConfig::default();
    config.should_bypass(&url)
}

#[tauri::command]
fn is_proxy_enabled() -> bool {
    let config = proxy::ProxyConfig::default();
    config.is_enabled()
}

// ============ Download Stats Commands ============

#[tauri::command]
fn get_download_stats(file_size: u64, segment_count: u32) -> serde_json::Value {
    let manager = downloader::manager::DownloadManager::new(file_size, segment_count);
    let stats = manager.get_stats();
    serde_json::json!({
        "total_segments": stats.total_segments,
        "active_segments": stats.active_segments,
        "complete_segments": stats.complete_segments,
        "total_speed_bps": stats.total_speed_bps,
        "downloaded_bytes": stats.downloaded_bytes,
        "total_bytes": stats.total_bytes,
        "progress_percent": stats.progress_percent
    })
}

#[tauri::command]
fn get_work_steal_config() -> serde_json::Value {
    let config = downloader::structures::WorkStealConfig::default();
    serde_json::json!({
        "min_split_size": config.min_split_size,
        "steal_ratio": config.steal_ratio,
        "speed_threshold_ratio": config.speed_threshold_ratio
    })
}

// ============ Retry State Commands ============

lazy_static::lazy_static! {
    static ref RETRY_STATE: std::sync::Mutex<downloader::network::RetryState> = 
        std::sync::Mutex::new(downloader::network::RetryState::default());
}

#[tauri::command]
fn get_retry_state() -> serde_json::Value {
    let state = RETRY_STATE.lock().unwrap_or_else(|e| e.into_inner());
    serde_json::json!({
        "immediate_attempts": state.immediate_attempts,
        "delayed_attempts": state.delayed_attempts,
        "current_delay_ms": state.current_delay.as_millis() as u64,
        "last_error": state.last_error
    })
}

#[tauri::command]
fn reset_retry_state() {
    let mut state = RETRY_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.reset();
}

#[tauri::command]
fn analyze_network_error(error_type: String) -> String {
    // Use a mock error to demonstrate analyze_error
    let strategy = match error_type.as_str() {
        "timeout" => downloader::network::RetryStrategy::Delayed(std::time::Duration::from_secs(5)),
        "connection" => downloader::network::RetryStrategy::Immediate,
        "forbidden" => downloader::network::RetryStrategy::RefreshLink,
        "not_found" => downloader::network::RetryStrategy::Fatal("File Not Found".to_string()),
        _ => downloader::network::RetryStrategy::Delayed(std::time::Duration::from_secs(3)),
    };
    format!("{:?}", strategy)
}

// ============ Resume File Commands ============

#[tauri::command]
fn open_file_for_resume(path: String) -> Result<u64, String> {
    // Validate path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    let p = std::path::Path::new(&path);
    let abs_path = if p.is_absolute() { p.to_path_buf() } else { download_dir.join(p) };
    let mut normalized = std::path::PathBuf::new();
    for component in abs_path.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Path must be within the download directory".to_string());
    }

    let file = downloader::disk::open_for_resume(&normalized)
        .map_err(|e| e.to_string())?;
    let size = file.metadata()
        .map(|m| m.len())
        .map_err(|e| e.to_string())?;
    Ok(size)
}

// ============ Plugin Config Commands ============

#[tauri::command]
async fn set_plugin_config(app_handle: tauri::AppHandle, script: String, config: std::collections::HashMap<String, String>) -> Result<(), String> {
    let client = rquest::Client::builder()
        // .min_tls_version(rquest::Version::TLS_1_2)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.set_config(config).await.map_err(|e| e.to_string())
}

// ============ DiskWriter Stats Commands ============

#[tauri::command]
fn get_disk_writer_config() -> serde_json::Value {
    let config = downloader::disk::DiskWriterConfig::default();
    serde_json::json!({
        "max_pending_writes": config.max_pending_writes,
        "coalesce_threshold": config.coalesce_threshold,
        "use_sparse": config.use_sparse
    })
}

#[tauri::command]
async fn refresh_download_url(state: State<'_, AppState>, app_handle: tauri::AppHandle, id: String, new_url: String) -> Result<(), String> {
    // Validate new URL scheme
    let parsed = reqwest::Url::parse(&new_url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {},
        s => return Err(format!("Unsupported URL scheme: {}", s)),
    }
    println!("DEBUG: Refreshing URL for {}: {}", id, new_url);
    
    // 1. Update in persistence
    let mut downloads = persistence::load_downloads().unwrap_or_default();
    if let Some(download) = downloads.iter_mut().find(|d| d.id == id) {
        download.url = new_url.clone();
        download.status = "Paused".to_string(); // Reset error state to Paused so it can be resumed
        persistence::save_downloads(&downloads).map_err(|e| e.to_string())?;
    } else {
        return Err("Download not found".to_string());
    }

    // 2. Stop active session if any
    {
        let mut active_downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = active_downloads.remove(&id) {
            // Signal stop
            let _ = session.stop_tx.send(());
            println!("DEBUG: Stopped active session for refresh: {}", id);
        }
    }
    
    // 3. Emit event
    app_handle.emit("download_refreshed", serde_json::json!({
        "id": id,
        "url": new_url
    })).map_err(|e| e.to_string())?;

    Ok(())
}






#[tauri::command]
async fn install_plugin(
    app_handle: tauri::AppHandle,
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>,
    url: String,
    filename: Option<String>,
) -> Result<String, String> {
    let installed = plugin_vm::updater::install_plugin_from_url(&app_handle, url, filename, None).await?;
    pm.load_plugins().await?;
    Ok(installed)
}

#[tauri::command]
async fn move_download_item(id: String, direction: String) -> Result<(), String> {
    persistence::move_download(&id, &direction)
}

#[tauri::command]
fn set_chaos_config(latency_ms: u64, error_rate: u64, enabled: bool) {
    crate::network::chaos::GLOBAL_CHAOS.update(enabled, latency_ms, error_rate);
}

#[tauri::command]
fn get_chaos_config() -> serde_json::Value {
    // Return simple JSON
    serde_json::json!({
        "enabled": crate::network::chaos::GLOBAL_CHAOS.enabled.load(std::sync::atomic::Ordering::Relaxed),
        "latency_ms": crate::network::chaos::GLOBAL_CHAOS.latency_ms.load(std::sync::atomic::Ordering::Relaxed),
        "error_rate": crate::network::chaos::GLOBAL_CHAOS.error_rate_percent.load(std::sync::atomic::Ordering::Relaxed)
    })
}

#[tauri::command]
async fn download_as_warc(url: String, save_path: String) -> Result<String, String> {
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    let path = std::path::PathBuf::from(&save_path);
    let resolved = if path.is_absolute() {
        path
    } else {
        std::path::PathBuf::from(&settings.download_dir).join(path)
    };
    // Validate resolved path stays within download directory
    let canonical_resolved = dunce::canonicalize(resolved.parent().unwrap_or(&resolved))
        .unwrap_or_else(|_| resolved.clone());
    if !canonical_resolved.starts_with(&download_dir) {
        return Err("Save path must be within the download directory".to_string());
    }
    crate::warc_archiver::download_as_warc(url, resolved).await
}

#[tauri::command]
fn run_in_sandbox(path: String) -> Result<String, String> {
    crate::sandbox::run_in_sandbox(path)
}

#[tauri::command]
async fn notarize_file(path: String) -> Result<serde_json::Value, String> {
    crate::notarize::notarize_file(path).await
}

#[tauri::command]
async fn verify_notarization(path: String) -> Result<serde_json::Value, String> {
    crate::notarize::verify_notarization(path).await
}

#[tauri::command]
async fn find_mirrors(path: String) -> Result<crate::mirror_hunter::MirrorDiscoveryResult, String> {
    // Validate that path is within download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canonical = dunce::canonicalize(&path)
        .map_err(|e| format!("Cannot resolve file path: {}", e))?;
    if !canonical.starts_with(&download_dir) {
        return Err("Path is outside download directory".to_string());
    }
    crate::mirror_hunter::find_mirrors(path).await
}

#[tauri::command]
fn list_usb_drives() -> Result<Vec<crate::usb_flasher::UsbDrive>, String> {
    crate::usb_flasher::list_usb_drives()
}

#[tauri::command]
async fn flash_to_usb(iso_path: String, drive_number: u32) -> Result<String, String> {
    crate::usb_flasher::flash_to_usb(iso_path, drive_number).await
}

#[tauri::command]
async fn replay_request(
    url: String, method: String,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>
) -> Result<crate::api_replay::ReplayResult, String> {
    crate::api_replay::replay_request(url, method, headers, body).await
}

#[tauri::command]
async fn fuzz_url(url: String) -> Result<serde_json::Value, String> {
    let result = crate::api_replay::fuzz_url(url).await?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[tauri::command]
async fn validate_c2pa(path: String) -> Result<serde_json::Value, String> {
    crate::c2pa_validator::validate_c2pa(path).await
}

#[tauri::command]
async fn arbitrage_download(urls: Vec<String>) -> Result<serde_json::Value, String> {
    let results = crate::bandwidth_arb::arbitrage_probe(urls).await?;
    serde_json::to_value(results).map_err(|e| e.to_string())
}

#[tauri::command]
async fn stego_hide(image_path: String, secret_data: String) -> Result<serde_json::Value, String> {
    crate::stego_vault::stego_hide(image_path, secret_data).await
}

#[tauri::command]
async fn stego_extract(image_path: String) -> Result<serde_json::Value, String> {
    crate::stego_vault::stego_extract(image_path).await
}

#[tauri::command]
fn launch_tui_dashboard() -> Result<String, String> {
    crate::tui_dashboard::launch_tui_dashboard()
}

#[tauri::command]
async fn auto_extract_archive(path: String, destination: Option<String>) -> Result<serde_json::Value, String> {
    // Validate archive path is within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(&path) {
        if !canon.starts_with(&download_dir) {
            return Err("Archive path must be within the download directory".to_string());
        }
    }
    // Validate destination if provided
    if let Some(ref dest) = destination {
        let dest_path = std::path::Path::new(dest);
        if dest_path.exists() {
            if let Ok(canon_dest) = dunce::canonicalize(dest_path) {
                if !canon_dest.starts_with(&download_dir) {
                    return Err("Destination must be within the download directory".to_string());
                }
            }
        } else if let Some(parent) = dest_path.parent() {
            if let Ok(canon_parent) = dunce::canonicalize(parent) {
                if !canon_parent.starts_with(&download_dir) {
                    return Err("Destination must be within the download directory".to_string());
                }
            }
        }
    }
    crate::auto_extract::extract_archive(path, destination).await
}

#[tauri::command]
async fn download_ipfs(cid: String, save_path: String) -> Result<serde_json::Value, String> {
    crate::ipfs_gateway::download_ipfs(cid, save_path).await
}

#[tauri::command]
async fn query_file(path: String, sql: String) -> Result<serde_json::Value, String> {
    crate::sql_query::query_file(path, sql).await
}

#[tauri::command]
async fn discover_dlna() -> Result<Vec<crate::dlna_cast::DlnaDevice>, String> {
    crate::dlna_cast::discover_dlna().await
}

#[tauri::command]
async fn cast_to_dlna(file_path: String, device_location: String) -> Result<String, String> {
    crate::dlna_cast::cast_to_dlna(file_path, device_location).await
}

#[tauri::command]
fn set_download_priority(id: String, level: String) -> Result<String, String> {
    crate::qos_manager::set_download_priority(id, level)
}

#[tauri::command]
fn get_qos_stats() -> Result<crate::qos_manager::QosStats, String> {
    crate::qos_manager::get_qos_stats()
}

#[tauri::command]
async fn optimize_mods(paths: Vec<String>) -> Result<serde_json::Value, String> {
    let result = crate::mod_optimizer::optimize_mods(paths).await?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[tauri::command]
fn rclone_list_remotes() -> Result<Vec<crate::rclone_bridge::RcloneRemote>, String> {
    crate::rclone_bridge::rclone_list_remotes()
}

#[tauri::command]
async fn rclone_transfer(source: String, destination: String) -> Result<String, String> {
    crate::rclone_bridge::rclone_transfer(source, destination).await
}

#[tauri::command]
async fn generate_subtitles(video_path: String) -> Result<serde_json::Value, String> {
    crate::subtitle_gen::generate_subtitles(video_path).await
}

#[tauri::command]
fn mount_drive(path: String, letter: String) -> Result<String, String> {
    crate::virtual_drive::mount_drive(path, letter)
}

#[tauri::command]
fn unmount_drive(letter: String) -> Result<String, String> {
    crate::virtual_drive::unmount_drive(letter)
}

#[tauri::command]
fn list_virtual_drives() -> Result<Vec<crate::virtual_drive::MountedDrive>, String> {
    crate::virtual_drive::list_virtual_drives()
}

#[tauri::command]
fn set_geofence_rule(url_pattern: String, region: String, proxy_type: String, proxy_address: String) -> Result<String, String> {
    crate::geofence::set_geofence_rule(url_pattern, region, proxy_type, proxy_address)
}

#[tauri::command]
fn get_geofence_rules() -> Result<Vec<crate::geofence::GeofenceRule>, String> {
    crate::geofence::get_geofence_rules()
}

#[tauri::command]
async fn upscale_image(path: String) -> Result<crate::ai::upscale::UpscaleResult, String> {
    crate::ai::upscale::upscale_image(&path).await
}

#[tauri::command]
fn set_app_firewall_rule(exe_path: String, blocked: bool) -> Result<String, String> {
    crate::network::wfp::set_app_firewall_rule(&exe_path, blocked)
}

#[tauri::command]
async fn fetch_with_ja3(url: String, browser: String) -> Result<String, String> {
    crate::network::tls_ja3::fetch_with_ja3(&url, &browser).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load settings and apply
    let initial_settings = settings::load_settings();
    speed_limiter::GLOBAL_LIMITER.set_limit(initial_settings.speed_limit_kbps * 1024);
    clipboard::CLIPBOARD_MONITOR.set_enabled(initial_settings.clipboard_monitor);
    
    // Load custom sounds from settings (Z1)
    let custom_sound_settings = initial_settings.clone();
    tauri::async_runtime::spawn(async move {
        audio_events::AUDIO_PLAYER.load_custom_sounds_from_settings(&custom_sound_settings).await;
    });
    
    // Auto-start Tor if enabled
    if initial_settings.use_tor {
        tauri::async_runtime::spawn(async {
            println!("Tor enabled in settings, initializing...");
            if let Err(e) = crate::network::tor::init_tor().await {
                eprintln!("Failed to auto-init Tor: {}", e);
            }
        });
    }

    // Create channel for HTTP server to send download requests
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<http_server::DownloadRequest>();

// ── Video Stream Detection Commands ──────────────────────────────────

#[tauri::command]
async fn probe_video_url(url: String) -> Result<Option<video_detector::DetectedStream>, String> {
    video_detector::probe_url(&url).await
}

#[tauri::command]
async fn scan_page_for_streams(url: String) -> Result<Vec<video_detector::DetectedStream>, String> {
    video_detector::scan_page_for_streams(&url).await
}

#[tauri::command]
fn classify_network_requests(
    requests: Vec<(String, String)>,
) -> Vec<video_detector::DetectedStream> {
    video_detector::classify_network_requests(&requests)
}

    // Create channel for batch link requests from browser extension
    let (batch_tx, mut batch_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<http_server::BatchLink>>();

    // Create channel for detected video/audio streams from browser extension
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<http_server::DetectedStreamRequest>>();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_download, 
            pause_download_cmd::pause_download, 
            get_downloads, 
            list_active_downloads,
            install_native_host,
            remove_download_entry, 
            update_download_url,
            get_settings,
            get_auth_token,
            save_settings,
            open_file, 
            open_folder,
            select_directory,
            select_file,
            schedule_download,
            get_scheduled_downloads,
            crawl_website,
            mux_video_audio,
            check_ffmpeg_installed,
            decrypt_aes_128,
            test_browser_fingerprint,
            get_proxy_config,
            test_proxy,
            acquire_bandwidth,
            set_speed_limit,
            get_speed_limit,
            get_active_speed_profile,
            // ChatOps Commands
            get_chatops_pending_urls,
            // Clipboard Commands
            get_clipboard_monitor_enabled,
            // Plugin Commands
            get_all_plugins,
            reload_plugins,
            // Old commands
            // get_plugin_metadata, 
            // set_plugin_config,
            // install_plugin,
            generate_lan_pairing_code,
            get_lan_pairing_qr_data,
            get_local_ip,
            is_quiet_hours,
            get_time_info,
            remove_scheduled_download,
            force_start_scheduled_download,
            get_plugin_metadata,
            get_adaptive_thread_count,
            update_thread_count,
            add_bandwidth_sample,
            get_average_bandwidth,
            set_plugin_config,
            install_plugin,
            move_download_item,
            refresh_download_url,
            extract_stream_url,
            should_bypass_proxy,
            is_proxy_enabled,
            get_download_stats,
            get_work_steal_config,
            get_retry_state,
            reset_retry_state,
            analyze_network_error,
            open_file_for_resume,
            get_disk_writer_config,
            add_magnet_link,
            add_torrent_file,
            play_torrent,
            get_torrents,
            get_torrent_diagnostics,
            get_recent_torrent_errors,
            clear_recent_torrent_errors,
            clear_recent_torrent_issues,
            restore_recent_torrent_issues,
            get_torrent_files,
            pause_torrent,
            resume_torrent,
            pause_all_torrents,
            resume_all_torrents,
            remove_torrent,
            update_torrent_files,
            open_torrent_folder,
            set_torrent_priority,
            set_torrent_pinned,
            set_chaos_config,
            get_chaos_config,
            // Advanced Subsystems
            set_qos_global_limit,
            remove_geofence_rule,
            toggle_geofence_rule,
            get_preset_regions,
            get_fastest_mirror,
            parse_ipfs_uri_cmd,
            get_rclone_version,
            get_rclone_ls,
            get_qos_download_limit,
            update_qos_download_speed,
            remove_qos_download,
            match_geofence_cmd,
            // HLS/Dash Commands
            parse_hls_stream,
            probe_hls_variants,
            parse_dash_manifest,
            fetch_dash_manifest,
            check_ffmpeg_available,
            // Multi-Source / Mirror Commands
            start_multi_source_download,
            probe_mirrors,
            // Download History Commands
            get_download_history,
            search_download_history,
            clear_download_history,
            delete_history_entry,
            export_download_history_csv,
            get_history_summary,
            // Activity Log Commands
            get_activity_log,
            // CAS Duplicate Detection Commands
            check_cas_duplicate,
            register_cas_entry,
            head_url_metadata,
            // Site Rules Commands
            list_site_rules,
            get_site_rule,
            add_site_rule,
            update_site_rule,
            delete_site_rule,
            import_site_rule_presets,
            test_site_rule,
            get_site_rule_presets,
            // File Categorizer Commands
            list_file_categories,
            get_file_category,
            add_file_category,
            update_file_category,
            delete_file_category,
            categorize_file,
            categorize_files_batch,
            get_file_category_stats,
            reset_file_categories,
            // Bandwidth Allocator Commands
            register_download_bandwidth,
            deregister_download_bandwidth,
            get_bandwidth_allocations,
            rebalance_bandwidth,
            // Crash Recovery Commands
            scan_crashed_downloads,
            get_interrupted_downloads,
            resume_interrupted_download,
            resume_all_interrupted,
            // Network Validation Commands
            analyze_http_status,
            check_captive_portal,
            validate_http_response,
            parse_retry_after_header,
            check_error_content_type,
            get_chrome_user_agent,
            get_default_http_config,
            calculate_retry_backoff,
            get_retry_config,
            analyze_error_strategy,
            start_range_download,
            validate_download_url,
            // Disk Commands
            preallocate_download_file,
            check_file_exists,
            get_file_size,
            read_file_bytes_at_offset,
            get_next_download_time,
            // ZIP Commands
            read_zip_last_bytes,
            preview_zip_partial,
            preview_zip_file,
            // Feeds
            fetch_feed,          // Q5
            get_feeds,
            get_feed_items,
            mark_feed_item_read,
            add_feed,
            update_feed,
            remove_feed,
            manual_refresh_feed,
            extract_single_file,
            preview_zip_remote,  // Remote Q3
            download_zip_entry,  // Remote Q3
            export_data,         // Q4
            import_data,         // Q4
            // Search
            perform_search,
            // mount_drive,
            // unmount_drive,
            upload_to_cloud,
            process_media,
            init_tor_network,
            get_tor_status,
            perform_semantic_search,
            index_all_downloads,
            join_workspace,
            get_plugin_source,
            save_plugin_source,
            delete_plugin,
            // Audio Settings Commands
            get_audio_enabled,
            set_audio_enabled,
            get_audio_volume,
            set_audio_volume,
            play_test_sound,
            // Webhook Commands
            get_webhooks,
            add_webhook,
            update_webhook,
            delete_webhook,
            test_webhook,
            // Archive Commands
            detect_archive,
            extract_archive,
            cleanup_archive,
            check_unrar_available,
            extract_zip_all,
            // P2P Commands
            create_p2p_share,
            join_p2p_share,
            list_p2p_sessions,
            close_p2p_session,
            get_p2p_stats,
            // P2P Upload Limit
            set_p2p_upload_limit,
            get_p2p_upload_limit,
            get_p2p_peer_reputation,
            // Custom Sound Files
            set_custom_sound_path,
            clear_custom_sound_path,
            get_custom_sound_paths,
            // Metadata Scrubber
            scrub_metadata,
            get_file_metadata,
            // Ephemeral Web Server
            start_ephemeral_share,
            stop_ephemeral_share,
            list_ephemeral_shares,
            // Wayback Machine
            check_wayback_availability,
            get_wayback_url,
            // DOI Resolver
            doi_resolver::resolve_doi,
            // Docker Image Puller
            docker_pull::fetch_docker_manifest,
            download_as_warc,
            run_in_sandbox,
            notarize_file,
            verify_notarization,
            find_mirrors,
            list_usb_drives,
            flash_to_usb,
            replay_request,
            fuzz_url,
            validate_c2pa,
            arbitrage_download,
            stego_hide,
            stego_extract,
            launch_tui_dashboard,
            auto_extract_archive,
            download_ipfs,
            query_file,
            discover_dlna,
            cast_to_dlna,
            set_download_priority,
            get_qos_stats,
            optimize_mods,
            rclone_list_remotes,
            rclone_transfer,
            generate_subtitles,
            mount_drive,
            unmount_drive,
            list_virtual_drives,
            set_geofence_rule,
            get_geofence_rules,
            upscale_image,
            set_app_firewall_rule,
            fetch_with_ja3,
            // Queue Management - Legacy (many were moved to queue_manager_cmds below)
            enqueue_download,
            pause_download_queue,
            resume_download_queue,
            add_download_dependency,
            remove_download_dependency,
            enqueue_download_chain,
            set_download_segments,
            // Bandwidth History
            get_bandwidth_history,
            get_bandwidth_samples,
            // Integrity Verification
            verify_download_checksum,
            compute_file_checksums,
            compute_file_hash,
            // Network Monitor
            check_network_status,
            // Video Stream Detection
            probe_video_url,
            scan_page_for_streams,
            classify_network_requests,
            // Settings Cache Commands
            commands::settings_cmds::get_settings_cache_stats,
            commands::settings_cmds::validate_settings,
            commands::settings_cmds::reload_settings_from_disk,
            commands::settings_cmds::get_cache_generation,
            commands::settings_cmds::invalidate_settings_cache,
            commands::settings_cmds::get_settings_with_stats,
            // Download Groups Commands
            commands::download_groups_cmds::create_download_group,
            commands::download_groups_cmds::add_member_to_group,
            commands::download_groups_cmds::add_group_dependency,
            commands::download_groups_cmds::get_group_details,
            commands::download_groups_cmds::start_group_download,
            commands::download_groups_cmds::pause_group_download,
            commands::download_groups_cmds::get_next_group_member,
            commands::download_groups_cmds::update_member_progress,
            commands::download_groups_cmds::complete_group_member,
            commands::download_groups_cmds::list_all_groups,
            commands::download_groups_cmds::delete_download_group,
            commands::download_groups_cmds::restore_groups_from_disk,
            commands::download_groups_cmds::save_groups_to_disk,
            commands::settings_cmds::save_settings_with_validation,
            commands::settings_cmds::get_field_validation_errors,
            // Production-grade Cache Commands
            commands::settings_cmds::get_cache_metrics,
            commands::settings_cmds::recover_settings_from_fallback,
            commands::settings_cmds::set_cache_degraded_mode,
            commands::settings_cmds::force_cache_refresh,
            commands::settings_cmds::check_cache_health,
            // Crash Recovery Commands (already defined in lib.rs)
            scan_crashed_downloads,
            get_interrupted_downloads,
            resume_interrupted_download,
            resume_all_interrupted,
            // Queue Manager Commands
            commands::queue_manager_cmds::get_queue_status,
            commands::queue_manager_cmds::get_queue_groups,
            commands::queue_manager_cmds::get_queue_items,
            commands::queue_manager_cmds::remove_from_queue,
            commands::queue_manager_cmds::set_queue_priority,
            commands::queue_manager_cmds::move_queue_item_to_front,
            commands::queue_manager_cmds::move_queue_item_up,
            commands::queue_manager_cmds::clear_download_queue,
            commands::queue_manager_cmds::pause_queue,
            commands::queue_manager_cmds::resume_queue,
            commands::queue_manager_cmds::is_queue_paused,
            commands::queue_manager_cmds::set_max_concurrent_downloads,
            commands::queue_manager_cmds::get_max_concurrent_downloads,
            commands::queue_manager_cmds::get_queue_stats,
            // Download State Management Commands
            commands::state_management_cmds::get_download_state,
            commands::state_management_cmds::get_all_download_states,
            commands::state_management_cmds::validate_resume_safety,
            commands::state_management_cmds::get_download_diagnostics,
            commands::state_management_cmds::get_downloads_health_summary,
            // Segment Integrity Verification Commands
            commands::segment_integrity_cmds::verify_download_integrity,
            commands::segment_integrity_cmds::verify_segments,
            commands::segment_integrity_cmds::get_cached_integrity_report,
            commands::segment_integrity_cmds::get_integrity_monitoring_metrics,
            commands::segment_integrity_cmds::generate_recovery_strategies,
            commands::segment_integrity_cmds::batch_verify_downloads,
            commands::segment_integrity_cmds::get_integrity_summary,
            commands::segment_integrity_cmds::export_integrity_report,
            // Mirror Scoring and Failure Prediction Commands
            commands::mirror_scoring_cmds::get_mirror_score,
            commands::mirror_scoring_cmds::record_mirror_success,
            commands::mirror_scoring_cmds::record_mirror_failure,
            commands::mirror_scoring_cmds::get_ranked_mirrors,
            commands::mirror_scoring_cmds::predict_segment_failure_risk,
            commands::mirror_scoring_cmds::get_all_mirror_metrics,
            // Download Groups Commands
            commands::download_groups_cmds::create_download_group,
            commands::download_groups_cmds::add_member_to_group,
            commands::download_groups_cmds::add_group_dependency,
            commands::download_groups_cmds::get_group_details,
            commands::download_groups_cmds::start_group_download,
            commands::download_groups_cmds::pause_group_download,
            commands::download_groups_cmds::get_next_group_member,
            commands::download_groups_cmds::update_member_progress,
            commands::download_groups_cmds::complete_group_member,
            commands::download_groups_cmds::list_all_groups,
            // Advanced Group Operations Commands
            commands::advanced_group_cmds::analyze_group_dependencies,
            commands::advanced_group_cmds::detect_url_batch,
            commands::advanced_group_cmds::recommend_execution_strategy,
            commands::advanced_group_cmds::validate_group_dependencies,
            commands::advanced_group_cmds::get_group_execution_plan,
            // Group Metrics Commands
            commands::group_metrics_cmds::get_group_metrics,
            commands::group_metrics_cmds::get_all_group_metrics,
            commands::group_metrics_cmds::get_group_member_metrics,
            commands::group_metrics_cmds::get_group_trends,
            commands::group_metrics_cmds::get_group_performance_summary,
            commands::group_metrics_cmds::estimate_group_completion_time,
            commands::group_metrics_cmds::get_system_download_stats,
            // Download Recovery Commands (Corruption Detection & Auto-Repair)
            recovery_commands::detect_corruption,
            recovery_commands::get_recovery_strategy,
            recovery_commands::execute_recovery,
            recovery_commands::get_corruption_report,
            recovery_commands::get_mirror_rankings,
            recovery_commands::update_mirror_reliability,
            recovery_commands::auto_execute_recovery,
            recovery_commands::cleanup_recovery_data,
            // Parallel Mirror Retry Commands
            parallel_mirror_commands::get_parallel_retry_config,
            parallel_mirror_commands::update_parallel_retry_config,
            parallel_mirror_commands::select_optimal_mirrors,
            parallel_mirror_commands::estimate_aggregated_throughput,
            parallel_mirror_commands::simulate_parallel_retry,
            // Mirror Analytics Commands
            mirror_analytics_commands::analyze_mirror_statistics,
            mirror_analytics_commands::compare_two_mirrors,
            mirror_analytics_commands::get_mirror_trend,
            mirror_analytics_commands::get_mirror_recommendation,
            mirror_analytics_commands::health_check_mirrors,
            mirror_analytics_commands::calculate_percentiles,
            // Speed Acceleration Commands
            speed_acceleration_commands::get_acceleration_stats,
            speed_acceleration_commands::record_bandwidth_measurement,
            speed_acceleration_commands::estimate_download_time,
            speed_acceleration_commands::get_optimal_segment_strategy,
            speed_acceleration_commands::predict_network_changes,
            speed_acceleration_commands::get_bandwidth_history,
            // Failure Prediction Commands
            failure_prediction_commands::record_download_metrics,
            failure_prediction_commands::analyze_failure_risk,
            failure_prediction_commands::record_prediction_accuracy,
            failure_prediction_commands::record_missed_failure,
            failure_prediction_commands::get_prediction_accuracy_stats,
            failure_prediction_commands::get_current_failure_prediction,
            failure_prediction_commands::reset_failure_prediction,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            
            clipboard::CLIPBOARD_MONITOR.start(app.handle().clone());
            scheduler::start_scheduler(app.handle().clone());
            feeds::start_poller(app.handle().clone());
            queue_manager::init_queue(&app.handle().clone());
            network_monitor::start_network_monitor(app.handle().clone());
            speed_profiles::start_speed_profile_scheduler();
            bandwidth_allocator::start_rebalancer();
            crash_recovery::recover_on_startup(&handle);
            // Initialize bandwidth history (restore persisted data + start sampler)
            bandwidth_history::restore_history();
            bandwidth_history::start_bandwidth_sampler(handle.clone());
            // Initialize event bus and event sourcing
            event_bus::init_event_bus(&handle);
            app.manage(std::sync::Arc::new(event_sourcing::SharedLog::new(&handle)));
            
            // Initialize Download Groups engine (restores groups from disk)
            group_engine::init_group_engine(&handle);
            
            tauri::async_runtime::spawn(async move {
                let lan_server = lan_api::LanApiServer::new(8765);
                if let Err(e) = lan_server.start().await {
                    eprintln!("LAN API server error: {}", e);
                }
            });
            
            // Init P2P node
            let p2p_node = tauri::async_runtime::block_on(async {
                match network::p2p::P2PNode::new(14735).await {
                    Ok(node) => node,
                    Err(e) => {
                        eprintln!("Warning: P2P failed to start on port 14735: {}. Trying fallback port...", e);
                        // Try fallback port
                        match network::p2p::P2PNode::new(14736).await {
                            Ok(node) => node,
                            Err(e2) => {
                                eprintln!("Warning: P2P also failed on fallback port: {}. Trying dynamic port...", e2);
                                match network::p2p::P2PNode::new(0).await {
                            Ok(node) => node,
                            Err(e3) => {
                                eprintln!("CRITICAL: P2P node failed on all ports including dynamic: {}. P2P features disabled.", e3);
                                // Gracefully degrade: create disabled node without WebSocket server
                                network::p2p::P2PNode::disabled()
                            }
                        }
                            }
                        }
                    }
                }
            });
            let p2p_node = Arc::new(p2p_node);
            
            let p2p_file_map: crate::http_server::FileMap = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
            
            let torrent_manager: Option<Arc<network::bittorrent::manager::TorrentManager>> = tauri::async_runtime::block_on(async {
                 let settings = settings::load_settings();
                 let speed_limit_kbps = settings.speed_limit_kbps;
                 let mut path = std::path::PathBuf::from(&settings.download_dir);
                 path.push("Torrents");
                 std::fs::create_dir_all(&path).unwrap_or_default();
                 let make_manager = |tm: network::bittorrent::manager::TorrentManager| {
                     let tm = Arc::new(tm);
                     tm.set_session_download_limit_kbps(speed_limit_kbps);
                     Some(tm)
                 };
                 match network::bittorrent::manager::TorrentManager::new(path).await {
                     Ok(tm) => make_manager(tm),
                     Err(e) => {
                         eprintln!("Warning: Torrent Manager failed to start: {}", e);
                         // Use a fallback temp directory instead of panicking
                         let fallback = std::env::temp_dir().join("hyperstream_torrents");
                         std::fs::create_dir_all(&fallback).unwrap_or_default();
                         match network::bittorrent::manager::TorrentManager::new(fallback).await {
                             Ok(tm) => make_manager(tm),
                             Err(e2) => {
                                 eprintln!("CRITICAL: Torrent Manager failed even with temp dir: {}. Torrent features will be unavailable.", e2);
                                 // Try one last time with a unique temp dir
                                 let last_resort = std::env::temp_dir().join(format!("hyperstream_torrents_{}", std::process::id()));
                                 std::fs::create_dir_all(&last_resort).unwrap_or_default();
                                 match network::bittorrent::manager::TorrentManager::new(last_resort).await {
                                     Ok(tm) => make_manager(tm),
                                     Err(e3) => {
                                         eprintln!("FATAL: Torrent subsystem completely unavailable: {}. Continuing without torrent support.", e3);
                                         None
                                     }
                                 }
                             }
                         }
                     }
                 }
            });

            // HTTP server spawn is deferred until after AppState initialization

            // Spawn Game Mode Monitor
            tauri::async_runtime::spawn(async move {
                crate::system_monitor::run_game_mode_monitor().await;
            });
            
            // ============ SYSTEM TRAY ============
            let quit_i = MenuItem::with_id(app.handle(), "quit", "Quit", true, None::<&str>)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let show_i = MenuItem::with_id(app.handle(), "show", "Show HyperStream", true, None::<&str>)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let menu = Menu::with_items(app.handle(), &[&show_i, &quit_i])
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "quit" => {
                            let state = app.state::<AppState>();
                            let paused = pause_download_cmd::pause_all_active_downloads(&state, false);
                            if paused > 0 {
                                println!("[Shutdown] Snapshotted {} active download(s) before tray quit", paused);
                            }
                            app.exit(0);
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            ..
                        } => {
                             let app = tray.app_handle();
                             if let Some(window) = app.get_webview_window("main") {
                                 let _ = window.show();
                                 let _ = window.set_focus();
                             }
                        }
                        _ => {}
                    }
                });
            // Set icon if available (avoid panic if window icon is missing)
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder.build(app.handle());
            // =====================================
            
            // Initialize ChatOps
            let settings_arc = std::sync::Arc::new(std::sync::Mutex::new(crate::settings::load_settings()));
            let chatops_manager = std::sync::Arc::new(crate::network::chatops::ChatOpsManager::new(
                settings_arc.clone(),
            ));
            chatops_manager.start();

            let conn_limit = crate::settings::load_settings().max_connections_per_host.max(1).min(64) as usize;
            // Manage AppState for Tauri's State<> system
            app.handle().manage(AppState { 
                 downloads: Mutex::new(HashMap::new()),
                 hls_sessions: Mutex::new(HashMap::new()),
                 dash_sessions: Mutex::new(HashMap::new()),
                 p2p_node: p2p_node.clone(),
                 p2p_file_map: p2p_file_map.clone(),
                 torrent_manager: torrent_manager.clone(),
                 connection_manager: network::connection_manager::ConnectionManager::new(conn_limit),
                 chatops_manager: chatops_manager.clone(),
                 recovery_manager: crate::download_recovery::DownloadRecoveryManager::new(),
                 failure_prediction_engine: Arc::new(Mutex::new(
                     crate::failure_prediction::FailurePredictionEngine::new(
                         crate::failure_prediction::PredictionConfig::default(),
                     ),
                 )),
            });

            // Spawn HTTP server (after AppState is managed)
            {
                let tx_clone = tx.clone();
                let batch_tx_clone = batch_tx.clone();
                let stream_tx_clone = stream_tx.clone();
                let map_clone = p2p_file_map.clone();
                let tm_clone = torrent_manager.clone();
                let app_handle_clone = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    crate::http_server::start_server(
                        tx_clone,
                        batch_tx_clone,
                        stream_tx_clone,
                        map_clone,
                        tm_clone,
                        app_handle_clone,
                    ).await;
                });
            }

            // Automatic torrent queue management (active slot enforcement).
            let queue_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let settings = crate::settings::load_settings();
                    let state = queue_app_handle.state::<AppState>();
                    if let Some(tm) = state.torrent_manager.as_ref() {
                        let queue_limit = if settings.torrent_auto_manage_queue {
                            settings.torrent_max_active_downloads as usize
                        } else {
                            0
                        };
                        if let Err(e) = tm
                            .enforce_queue_limits(queue_limit)
                            .await
                        {
                            eprintln!("[torrent-queue] enforcement error: {}", e);
                        }

                        if let Err(e) = tm
                            .enforce_seeding_policy(
                                settings.torrent_auto_stop_seeding,
                                settings.torrent_seed_ratio_limit,
                                settings.torrent_seed_time_limit_mins,
                            )
                            .await
                        {
                            eprintln!("[torrent-seeding] enforcement error: {}", e);
                        }
                    }
                }
            });

            // Initialize Plugin Manager
            let plugin_manager = crate::plugin_vm::manager::PluginManager::new(app.handle().clone());
            // Start async load
            let pm_clone = std::sync::Arc::new(plugin_manager);
            app.handle().manage(pm_clone.clone());
            
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pm_clone.load_plugins().await {
                   eprintln!("Failed to load plugins: {}", e);
                }
            });
            
            // Smart Sleep + Battery Polling
            let battery_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let settings = crate::settings::load_settings();
                    let state = battery_app_handle.state::<AppState>();
                    
                    let active_count = {
                        let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner()).len();
                        let hls = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner()).len();
                        let dash = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner()).len();
                        downloads + hls + dash
                    };
                    
                    if settings.prevent_sleep_during_download {
                        crate::power_manager::prevent_sleep(active_count > 0);
                    } else {
                        crate::power_manager::prevent_sleep(false);
                    }
                    
                    if settings.pause_on_low_battery {
                        if let Some(pct) = crate::power_manager::get_battery_percentage() {
                            if pct <= 15 && active_count > 0 {
                                println!("🔋 Battery critical ({}%). Pausing all downloads.", pct);
                                let paused = pause_download_cmd::pause_all_active_downloads(&state, false);
                                if paused > 0 {
                                    println!("🔋 Paused {} active download(s) due to low battery.", paused);
                                }
                            }
                        }
                    }
                }
            });
            
            tauri::async_runtime::spawn(async move {
                while let Some(req) = rx.recv().await {
                    println!("DEBUG: Processing download from extension: {}", req.url);
                    let _ = handle.emit("extension_download", serde_json::json!({
                        "url": &req.url,
                        "filename": &req.filename,
                        "customHeaders": &req.custom_headers,
                        "pageUrl": &req.page_url,
                        "source": &req.source
                    }));
                    // broadcast to any HTTP listeners
                    let _ = crate::http_server::get_event_sender().send(serde_json::json!({
                        "type": "extension_download",
                        "url": &req.url,
                        "filename": &req.filename,
                        "customHeaders": &req.custom_headers,
                        "pageUrl": &req.page_url,
                        "source": &req.source
                    }));
                }
            });

            // Handle batch link requests from browser extension
            let batch_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(links) = batch_rx.recv().await {
                    println!("DEBUG: Batch links received from extension: {} links", links.len());
                    let _ = batch_handle.emit("batch_links", &links);
                    let _ = crate::http_server::get_event_sender().send(serde_json::json!({
                        "type": "batch_links",
                        "links": links
                    }));
                }
            });

            // Handle detected video/audio streams from browser extension
            let stream_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(streams) = stream_rx.recv().await {
                    println!("DEBUG: Detected {} video/audio streams from extension", streams.len());
                    let _ = stream_handle.emit("detected_streams", &streams);
                    let _ = crate::http_server::get_event_sender().send(serde_json::json!({
                        "type": "detected_streams",
                        "streams": streams
                    }));
                }
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ============ Plugin Manager Commands ============

#[tauri::command]
async fn get_all_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<Vec<crate::plugin_vm::lua_host::PluginMetadata>, String> {
    Ok(pm.get_plugins_list())
}

#[tauri::command]
async fn reload_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<(), String> {
    pm.load_plugins().await.map_err(|e| e.to_string())
}

/// Validate plugin filename to prevent path traversal attacks.
fn validate_plugin_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty() || filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("Invalid plugin filename".to_string());
    }
    Ok(())
}

#[tauri::command]
async fn get_plugin_source(app_handle: tauri::AppHandle, filename: String) -> Result<String, String> {
    validate_plugin_filename(&filename)?;
    let path = crate::plugin_vm::get_plugins_dir(&app_handle).join(format!("{}.lua", filename));
    if !path.exists() {
        return Err("Plugin file not found".to_string());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_plugin_source(
    app_handle: tauri::AppHandle,
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>,
    filename: String,
    content: String,
) -> Result<(), String> {
    validate_plugin_filename(&filename)?;
    let plugins_dir = crate::plugin_vm::get_plugins_dir(&app_handle);
    if !plugins_dir.exists() {
        std::fs::create_dir_all(&plugins_dir).map_err(|e| e.to_string())?;
    }
    let path = plugins_dir.join(format!("{}.lua", filename));
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    pm.load_plugins().await?;
    Ok(())
}

#[tauri::command]
async fn delete_plugin(
    app_handle: tauri::AppHandle,
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>,
    filename: String,
) -> Result<(), String> {
    validate_plugin_filename(&filename)?;
    let path = crate::plugin_vm::get_plugins_dir(&app_handle).join(format!("{}.lua", filename));
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    pm.load_plugins().await?;
    Ok(())
}

// ============ Audio Settings Commands ============
#[tauri::command]
async fn get_audio_enabled() -> bool {
    audio_events::AUDIO_PLAYER.is_enabled().await
}

#[tauri::command]
async fn set_audio_enabled(enabled: bool) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_enabled(enabled).await;
    Ok(())
}

#[tauri::command]
async fn get_audio_volume() -> f32 {
    audio_events::AUDIO_PLAYER.get_volume().await
}

#[tauri::command]
async fn set_audio_volume(volume: f32) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_volume(volume).await;
    Ok(())
}

#[tauri::command]
async fn play_test_sound(sound_type: String) -> Result<(), String> {
    let event = match sound_type.as_str() {
        "success" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        "start" => audio_events::SoundEvent::DownloadStart,
        _ => return Err(format!("Unknown sound type: {}", sound_type)),
    };
    
    audio_events::AUDIO_PLAYER.play(event).await;
    Ok(())
}

// ============ Webhook Commands ============
#[tauri::command]
async fn get_webhooks() -> Result<Vec<webhooks::WebhookConfig>, String> {
    let settings = settings::load_settings();
    Ok(settings.webhooks.unwrap_or_default())
}

#[tauri::command]
async fn add_webhook(config: webhooks::WebhookConfig) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    let mut config = config;
    if config.id.is_empty() {
        config.id = webhooks::generate_webhook_id();
    }
    webhooks.push(config);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
async fn update_webhook(id: String, config: webhooks::WebhookConfig) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    
    if let Some(webhook) = webhooks.iter_mut().find(|w| w.id == id) {
        *webhook = config;
        settings.webhooks = Some(webhooks);
        settings::save_settings(&settings)
    } else {
        Err("Webhook not found".to_string())
    }
}

#[tauri::command]
async fn delete_webhook(id: String) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    webhooks.retain(|w| w.id != id);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
async fn test_webhook(id: String) -> Result<(), String> {
    let settings = settings::load_settings();
    let webhooks = settings.webhooks.unwrap_or_default();
    
    let config = webhooks.iter()
        .find(|w| w.id == id)
        .ok_or("Webhook not found")?;
    
    let payload = webhooks::WebhookPayload {
        event: "DownloadComplete".to_string(),
        download_id: "test_123".to_string(),
        filename: "test_file.zip".to_string(),
        url: "https://example.com/test.zip".to_string(),
        size: 104857600, // 100 MB
        speed: 10485760, // 10 MB/s
        filepath: Some("C:\\Downloads\\test_file.zip".to_string()),
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    let manager = webhooks::WebhookManager::new();
    manager.load_configs(vec![config.clone()]).await;
    manager.trigger(webhooks::WebhookEvent::DownloadComplete, payload).await;
    
    Ok(())
}

// ============ Archive Commands ============
#[tauri::command]
async fn detect_archive(path: String) -> Option<archive_manager::ArchiveInfo> {
    archive_manager::ArchiveManager::detect_archive(&path)
}

#[tauri::command]
async fn extract_archive(archive_path: String, dest_dir: Option<String>) -> Result<String, String> {
    // Validate paths are within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;

    if let Ok(canon) = dunce::canonicalize(&archive_path) {
        if !canon.starts_with(&download_dir) {
            return Err("Archive path must be within the download directory".to_string());
        }
    }

    // Use same directory as archive if dest not specified
    let dest = if let Some(d) = dest_dir {
        // Validate dest is also within download dir
        let abs_dest = if std::path::Path::new(&d).is_absolute() {
            std::path::PathBuf::from(&d)
        } else {
            download_dir.join(&d)
        };
        let mut normalized = std::path::PathBuf::new();
        for component in abs_dest.components() {
            match component {
                std::path::Component::ParentDir => { normalized.pop(); },
                std::path::Component::CurDir => {},
                c => normalized.push(c.as_os_str()),
            }
        }
        if !normalized.starts_with(&download_dir) {
            return Err("Destination path must be within the download directory".to_string());
        }
        normalized.to_string_lossy().to_string()
    } else {
        std::path::Path::new(&archive_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string()
    };
    
    archive_manager::ArchiveManager::extract_archive(&archive_path, &dest)
}

#[tauri::command]
async fn cleanup_archive(archive_path: String) -> Result<(), String> {
    archive_manager::ArchiveManager::cleanup_archive(&archive_path)
}

#[tauri::command]
fn check_unrar_available() -> bool {
    archive_manager::ArchiveManager::check_unrar_available()
}

#[tauri::command]
fn extract_zip_all(zip_path: String, dest_dir: String) -> Result<usize, String> {
    // Validate paths are within the download directory
    let settings = settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;

    if let Ok(canon) = dunce::canonicalize(&zip_path) {
        if !canon.starts_with(&download_dir) {
            return Err("Zip path must be within the download directory".to_string());
        }
    }

    let abs_dest = if std::path::Path::new(&dest_dir).is_absolute() {
        std::path::PathBuf::from(&dest_dir)
    } else {
        download_dir.join(&dest_dir)
    };
    let mut normalized = std::path::PathBuf::new();
    for component in abs_dest.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir => {},
            c => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&download_dir) {
        return Err("Destination path must be within the download directory".to_string());
    }

    zip_preview::extract_all(std::path::Path::new(&zip_path), &normalized)
}

// ============ P2P Commands ============
#[tauri::command]
async fn create_p2p_share(
    download_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.create_share_session(download_id).await
}

#[tauri::command]
async fn join_p2p_share(
    code: String,
    peer_addr: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.join_share_session(code, peer_addr).await
}

#[tauri::command]
fn list_p2p_sessions(state: tauri::State<'_, AppState>) -> Vec<network::p2p::P2PShareSession> {
    state.p2p_node.list_sessions()
}

#[tauri::command]
fn close_p2p_session(session_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.p2p_node.close_session(&session_id)
}

#[tauri::command]
fn get_p2p_stats(state: tauri::State<'_, AppState>) -> network::p2p::P2PStats {
    state.p2p_node.get_stats()
}

// Old dummy commands removed

// ============ P2P Upload Limit Commands (G1) ============
#[tauri::command]
fn set_p2p_upload_limit(kbps: u64, state: tauri::State<'_, AppState>) {
    state.p2p_node.set_upload_limit(kbps);
}

#[tauri::command]
fn get_p2p_upload_limit(state: tauri::State<'_, AppState>) -> u64 {
    state.p2p_node.get_upload_limit()
}

#[tauri::command]
fn get_p2p_peer_reputation(peer_id: String, state: tauri::State<'_, AppState>) -> Option<network::p2p::PeerReputation> {
    state.p2p_node.get_reputation(&peer_id)
}

// ============ Custom Sound File Commands (Z1) ============
#[tauri::command]
async fn set_custom_sound_path(event_type: String, path: String) -> Result<(), String> {
    let event = match event_type.as_str() {
        "start" => audio_events::SoundEvent::DownloadStart,
        "complete" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        _ => return Err(format!("Unknown sound event: {}", event_type)),
    };
    
    // Validate file exists
    if !std::path::Path::new(&path).exists() {
        return Err("Sound file does not exist".to_string());
    }
    
    audio_events::AUDIO_PLAYER.set_custom_sound(event, std::path::PathBuf::from(&path)).await;
    
    // Save to settings
    let mut settings = settings::load_settings();
    match event_type.as_str() {
        "start" => settings.custom_sound_start = Some(path),
        "complete" => settings.custom_sound_complete = Some(path),
        "error" => settings.custom_sound_error = Some(path),
        _ => {}
    }
    settings::save_settings(&settings)?;
    
    Ok(())
}

#[tauri::command]
async fn clear_custom_sound_path(event_type: String) -> Result<(), String> {
    let event = match event_type.as_str() {
        "start" => audio_events::SoundEvent::DownloadStart,
        "complete" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        _ => return Err(format!("Unknown sound event: {}", event_type)),
    };
    
    audio_events::AUDIO_PLAYER.clear_custom_sound(event).await;
    
    // Save to settings
    let mut settings = settings::load_settings();
    match event_type.as_str() {
        "start" => settings.custom_sound_start = None,
        "complete" => settings.custom_sound_complete = None,
        "error" => settings.custom_sound_error = None,
        _ => {}
    }
    settings::save_settings(&settings)?;
    
    Ok(())
}

#[tauri::command]
async fn get_custom_sound_paths() -> std::collections::HashMap<String, String> {
    audio_events::AUDIO_PLAYER.get_custom_sounds().await
}

// ============ Metadata Scrubber Commands ============
#[tauri::command]
fn scrub_metadata(path: String) -> Result<metadata_scrubber::ScrubResult, String> {
    metadata_scrubber::scrub_file(&path)
}

#[tauri::command]
fn get_file_metadata(path: String) -> Result<metadata_scrubber::MetadataInfo, String> {
    metadata_scrubber::get_metadata_info(&path)
}

// ============ Ephemeral Web Server Commands ============
#[tauri::command]
async fn start_ephemeral_share(path: String, timeout_mins: Option<u64>) -> Result<ephemeral_server::EphemeralShare, String> {
    // Validate that shared file is within the download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canonical = dunce::canonicalize(&path)
        .map_err(|e| format!("Cannot resolve file path: {}", e))?;
    if !canonical.starts_with(&download_dir) {
        return Err("Cannot share files outside the download directory".to_string());
    }
    let timeout = timeout_mins.unwrap_or(60); // Default 1 hour
    ephemeral_server::EPHEMERAL_MANAGER.start_share(path, timeout).await
}

#[tauri::command]
fn stop_ephemeral_share(id: String) -> Result<(), String> {
    ephemeral_server::EPHEMERAL_MANAGER.stop_share(&id)
}

#[tauri::command]
fn list_ephemeral_shares() -> Vec<ephemeral_server::EphemeralShare> {
    ephemeral_server::EPHEMERAL_MANAGER.list_shares()
}

// ============ Wayback Machine Commands ============
#[tauri::command]
async fn check_wayback_availability(url: String) -> Result<Option<wayback::WaybackSnapshot>, String> {
    wayback::check_wayback(&url).await
}

#[tauri::command]
fn get_wayback_url(wayback_url: String) -> String {
    wayback::get_wayback_download_url(&wayback_url)
}

#[cfg(test)]
mod active_download_status_tests {
    use super::*;
    use crate::core_state::{AppState, DownloadSession, HlsSession};
    use crate::downloader::manager::DownloadManager;
    use crate::downloader::structures::SegmentState;
    use std::collections::HashMap;

    fn make_test_state() -> AppState {
        AppState {
            downloads: Mutex::new(HashMap::new()),
            hls_sessions: Mutex::new(HashMap::new()),
            dash_sessions: Mutex::new(HashMap::new()),
            p2p_node: Arc::new(network::p2p::P2PNode::disabled()),
            p2p_file_map: Arc::new(Mutex::new(HashMap::new())),
            torrent_manager: None,
            connection_manager: network::connection_manager::ConnectionManager::default(),
            chatops_manager: Arc::new(network::chatops::ChatOpsManager::new(Arc::new(Mutex::new(
                crate::settings::load_settings(),
            )))),
            recovery_manager: crate::download_recovery::DownloadRecoveryManager::new(),
            failure_prediction_engine: Arc::new(Mutex::new(
                crate::failure_prediction::FailurePredictionEngine::new(
                    crate::failure_prediction::PredictionConfig::default(),
                ),
            )),
        }
    }

    fn make_temp_writer(name: &str) -> Arc<Mutex<std::fs::File>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hyperstream-{name}-{unique}.tmp"));
        Arc::new(Mutex::new(std::fs::File::create(path).expect("temp writer")))
    }

    #[test]
    fn collect_active_download_statuses_reports_all_protocols() {
        let state = make_test_state();
        let manager = Arc::new(Mutex::new(DownloadManager::new(1_000, 1)));
        {
            let manager_guard = manager.lock().unwrap_or_else(|e| e.into_inner());
            let mut segments = manager_guard.segments.write().unwrap_or_else(|e| e.into_inner());
            let segment = segments.first_mut().expect("http segment");
            segment.state = SegmentState::Downloading;
            segment.downloaded_cursor = 400;
            segment.speed_bps = 128;
        }

        let (http_stop_tx, _) = tokio::sync::broadcast::channel(1);
        state.downloads.lock().unwrap_or_else(|e| e.into_inner()).insert(
            "http-1".to_string(),
            DownloadSession {
                manager,
                stop_tx: http_stop_tx,
                url: "https://example.com/file.bin".to_string(),
                path: "/tmp/file.bin".to_string(),
                file_writer: make_temp_writer("http"),
                group_context: None,
            },
        );

        let (hls_stop_tx, _) = tokio::sync::broadcast::channel(1);
        state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(
            "hls-1".to_string(),
            HlsSession {
                manifest_url: "https://example.com/playlist.m3u8".to_string(),
                output_path: "/tmp/video.mp4".to_string(),
                segments: Vec::new(),
                segment_sizes: vec![300, 300],
                downloaded: Arc::new(std::sync::atomic::AtomicU64::new(150)),
                speed_bps: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                stop_tx: hls_stop_tx,
                file_writer: make_temp_writer("hls"),
            },
        );

        let (dash_stop_tx, _) = tokio::sync::broadcast::channel(1);
        state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(
            "dash-1".to_string(),
            crate::engine::dash::DashSession {
                manifest_url: "https://example.com/manifest.mpd".to_string(),
                output_path: "/tmp/dash.mp4".to_string(),
                video_rep: None,
                audio_rep: None,
                video_total: 500,
                audio_total: 200,
                downloaded: Arc::new(std::sync::atomic::AtomicU64::new(175)),
                stop_tx: dash_stop_tx,
            },
        );

        let statuses = collect_active_download_statuses(&state);
        assert_eq!(statuses.len(), 3);

        let http = statuses.iter().find(|item| item.id == "http-1").expect("http status");
        assert_eq!(http.downloaded, 400);
        assert_eq!(http.total, 1_000);
        assert_eq!(http.speed_bps, 128);
        assert_eq!(http.status, "Downloading");
        assert!(http.can_pause);
        assert!(http.can_cancel);

        let hls = statuses.iter().find(|item| item.id == "hls-1").expect("hls status");
        assert_eq!(hls.total, 600);
        assert_eq!(hls.downloaded, 150);
        assert_eq!(hls.speed_bps, 0);
        assert_eq!(hls.status, "Downloading");

        let dash = statuses.iter().find(|item| item.id == "dash-1").expect("dash status");
        assert_eq!(dash.total, 700);
        assert_eq!(dash.downloaded, 175);
        assert_eq!(dash.speed_bps, 0);
        assert_eq!(dash.status, "Downloading");
        assert!(dash.can_pause);
        assert!(dash.can_cancel);
    }

    #[test]
    fn collect_active_download_statuses_hides_controls_for_complete_http_sessions() {
        let state = make_test_state();
        let manager = Arc::new(Mutex::new(DownloadManager::new(100, 1)));
        {
            let manager_guard = manager.lock().unwrap_or_else(|e| e.into_inner());
            let mut segments = manager_guard.segments.write().unwrap_or_else(|e| e.into_inner());
            let segment = segments.first_mut().expect("http segment");
            segment.state = SegmentState::Complete;
            segment.downloaded_cursor = 100;
        }

        let (stop_tx, _) = tokio::sync::broadcast::channel(1);
        state.downloads.lock().unwrap_or_else(|e| e.into_inner()).insert(
            "http-complete".to_string(),
            DownloadSession {
                manager,
                stop_tx,
                url: "https://example.com/complete.bin".to_string(),
                path: "/tmp/complete.bin".to_string(),
                file_writer: make_temp_writer("complete"),
                group_context: None,
            },
        );

        let status = collect_active_download_statuses(&state)
            .into_iter()
            .find(|item| item.id == "http-complete")
            .expect("complete status");

        assert_eq!(status.status, "Complete");
        assert!(!status.can_pause);
        assert!(!status.can_cancel);
    }
}

#[cfg(test)]
mod torrent_add_config_tests {
    use super::*;

    const SAMPLE_HASH: &str = "ABCDEF1234567890ABCDEF1234567890ABCDEF12";

    fn sample_issue(timestamp_ms: u64, severity: &str, action: &str) -> TorrentActionFailedEvent {
        TorrentActionFailedEvent {
            timestamp_ms,
            severity: severity.to_string(),
            category: "test".to_string(),
            action: action.to_string(),
            id: Some(1),
            error: "boom".to_string(),
        }
    }

    #[test]
    fn apply_sets_priority_and_pin() {
        let mut s = settings::Settings::default();
        apply_torrent_preferences_to_settings(&mut s, SAMPLE_HASH, Some("high"), Some(true));

        let key = SAMPLE_HASH.to_ascii_lowercase();
        assert_eq!(
            s.torrent_priority_overrides.get(&key).map(|v| v.as_str()),
            Some("high")
        );
        assert!(s.torrent_pinned_hashes.contains(&key));
    }

    #[test]
    fn apply_normal_priority_removes_override_and_preserves_pin_when_pin_not_provided() {
        let mut s = settings::Settings::default();
        let key = SAMPLE_HASH.to_ascii_lowercase();
        s.torrent_priority_overrides
            .insert(key.clone(), "low".to_string());
        s.torrent_pinned_hashes.insert(key.clone());

        apply_torrent_preferences_to_settings(&mut s, SAMPLE_HASH, Some("normal"), None);

        assert!(!s.torrent_priority_overrides.contains_key(&key));
        assert!(s.torrent_pinned_hashes.contains(&key));
    }

    #[test]
    fn apply_unpin_only_removes_pin_without_touching_priority() {
        let mut s = settings::Settings::default();
        let key = SAMPLE_HASH.to_ascii_lowercase();
        s.torrent_priority_overrides
            .insert(key.clone(), "high".to_string());
        s.torrent_pinned_hashes.insert(key.clone());

        apply_torrent_preferences_to_settings(&mut s, SAMPLE_HASH, None, Some(false));

        assert_eq!(
            s.torrent_priority_overrides.get(&key).map(|v| v.as_str()),
            Some("high")
        );
        assert!(!s.torrent_pinned_hashes.contains(&key));
    }

    #[test]
    fn apply_no_options_is_noop() {
        let mut s = settings::Settings::default();
        let key = SAMPLE_HASH.to_ascii_lowercase();
        s.torrent_priority_overrides
            .insert(key.clone(), "low".to_string());
        s.torrent_pinned_hashes.insert(key);

        let before_overrides = s.torrent_priority_overrides.clone();
        let before_pins = s.torrent_pinned_hashes.clone();

        apply_torrent_preferences_to_settings(&mut s, SAMPLE_HASH, None, None);

        assert_eq!(s.torrent_priority_overrides, before_overrides);
        assert_eq!(s.torrent_pinned_hashes, before_pins);
    }

    #[test]
    fn action_category_covers_add_config_actions() {
        assert_eq!(torrent_action_category("add_magnet_config"), "config");
        assert_eq!(torrent_action_category("add_torrent_file_config"), "config");
    }

    #[test]
    fn action_category_keeps_known_policy_actions_as_policy() {
        assert_eq!(torrent_action_category("add_magnet_policy"), "policy");
        assert_eq!(torrent_action_category("settings_policy"), "policy");
    }

    #[test]
    fn should_restore_batch_accepts_none_or_matching_token() {
        assert!(should_restore_cleared_batch(None, 42));
        assert!(should_restore_cleared_batch(Some(42), 42));
        assert!(!should_restore_cleared_batch(Some(7), 42));
    }

    #[test]
    fn split_recent_issues_by_filter_keeps_and_removes_expected_entries() {
        let entries = vec![
            sample_issue(10, "error", "pause"),
            sample_issue(20, "warning", "add_magnet_policy"),
            sample_issue(30, "error", "resume"),
        ];

        let (kept_errors, removed_errors) =
            split_recent_torrent_issues_by_filter(entries.clone(), Some("errors")).unwrap();
        assert_eq!(kept_errors.len(), 1);
        assert_eq!(removed_errors.len(), 2);
        assert!(kept_errors
            .iter()
            .all(|entry| entry.severity.eq_ignore_ascii_case("warning")));
        assert!(removed_errors
            .iter()
            .all(|entry| entry.severity.eq_ignore_ascii_case("error")));

        let (kept_warnings, removed_warnings) =
            split_recent_torrent_issues_by_filter(entries, Some("warnings")).unwrap();
        assert_eq!(kept_warnings.len(), 2);
        assert_eq!(removed_warnings.len(), 1);
        assert!(kept_warnings
            .iter()
            .all(|entry| entry.severity.eq_ignore_ascii_case("error")));
        assert!(removed_warnings
            .iter()
            .all(|entry| entry.severity.eq_ignore_ascii_case("warning")));
    }

    #[test]
    fn merge_recent_issues_restores_timestamp_order_and_caps_length() {
        let existing = vec![
            sample_issue(100, "error", "pause"),
            sample_issue(300, "warning", "add_magnet_policy"),
        ];
        let restored = vec![sample_issue(200, "error", "resume")];
        let merged = merge_recent_torrent_issues(existing, restored);

        let merged_timestamps = merged
            .iter()
            .map(|entry| entry.timestamp_ms)
            .collect::<Vec<_>>();
        assert_eq!(merged_timestamps, vec![100, 200, 300]);

        let oversized = (0..(MAX_RECENT_TORRENT_ERRORS + 3))
            .map(|idx| sample_issue(idx as u64, "error", "pause"))
            .collect::<Vec<_>>();
        let clamped = merge_recent_torrent_issues(oversized, Vec::new());
        assert_eq!(clamped.len(), MAX_RECENT_TORRENT_ERRORS);
        assert_eq!(clamped.first().map(|entry| entry.timestamp_ms), Some(3));
    }

    #[test]
    fn error_message_normalization_keeps_short_messages() {
        let msg = "disk full";
        assert_eq!(normalize_torrent_error_message(msg), "disk full");
    }

    #[test]
    fn error_message_normalization_truncates_long_messages() {
        let msg = "x".repeat(MAX_TORRENT_ERROR_MESSAGE_CHARS + 64);
        let normalized = normalize_torrent_error_message(&msg);
        assert!(normalized.ends_with("..."));
        assert_eq!(normalized.chars().count(), MAX_TORRENT_ERROR_MESSAGE_CHARS + 3);
    }
}

// ============ DOWNLOAD GROUPS INTEGRATION TESTS ============
#[cfg(test)]
mod download_group_tests {
    use crate::download_groups::{DownloadGroup, GroupMember, ExecutionStrategy};
    use crate::group_dag_solver::DagSolver;

    #[test]
    fn test_simple_chain_a_b_c() {
        let mut group = DownloadGroup::new("Chain A→B→C");
        group.strategy = ExecutionStrategy::Sequential;

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let member_b = GroupMember::new(
            "http://example.com/b.zip".to_string(),
            vec![member_a.id.clone()],
        );
        let member_c = GroupMember::new(
            "http://example.com/c.zip".to_string(),
            vec![member_b.id.clone()],
        );

        let id_a = member_a.id.clone();
        let id_b = member_b.id.clone();
        let id_c = member_c.id.clone();

        group.members.insert(id_a.clone(), member_a);
        group.members.insert(id_b.clone(), member_b);
        group.members.insert(id_c.clone(), member_c);

        let topo = DagSolver::topological_sort(&group).expect("Topo sort failed");
        assert_eq!(topo.order, vec![id_a, id_b, id_c]);
        assert_eq!(topo.critical_path_length, 3);
    }

    #[test]
    fn test_fan_out_a_spawns_b_c_d() {
        let mut group = DownloadGroup::new("Fan-out A→[B,C,D]");
        group.strategy = ExecutionStrategy::Hybrid;

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let id_a = member_a.id.clone();

        let member_b = GroupMember::new(
            "http://example.com/b.zip".to_string(),
            vec![id_a.clone()],
        );
        let member_c = GroupMember::new(
            "http://example.com/c.zip".to_string(),
            vec![id_a.clone()],
        );
        let member_d = GroupMember::new(
            "http://example.com/d.zip".to_string(),
            vec![id_a.clone()],
        );

        let id_b = member_b.id.clone();
        let id_c = member_c.id.clone();
        let id_d = member_d.id.clone();

        group.members.insert(id_a.clone(), member_a);
        group.members.insert(id_b.clone(), member_b);
        group.members.insert(id_c.clone(), member_c);
        group.members.insert(id_d.clone(), member_d);

        let topo = DagSolver::topological_sort(&group).expect("Topo sort failed");
        assert_eq!(topo.order[0], id_a);
        assert!(topo.order[1..].contains(&id_b));
        assert!(topo.order[1..].contains(&id_c));
        assert!(topo.order[1..].contains(&id_d));
        assert_eq!(topo.critical_path_length, 2);
    }

    #[test]
    fn test_diamond_a_to_d_via_b_c() {
        let mut group = DownloadGroup::new("Diamond A→[B,C]→D");
        group.strategy = ExecutionStrategy::Hybrid;

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let id_a = member_a.id.clone();

        let member_b = GroupMember::new(
            "http://example.com/b.zip".to_string(),
            vec![id_a.clone()],
        );
        let member_c = GroupMember::new(
            "http://example.com/c.zip".to_string(),
            vec![id_a.clone()],
        );

        let id_b = member_b.id.clone();
        let id_c = member_c.id.clone();

        let member_d = GroupMember::new(
            "http://example.com/d.zip".to_string(),
            vec![id_b.clone(), id_c.clone()],
        );
        let id_d = member_d.id.clone();

        group.members.insert(id_a.clone(), member_a);
        group.members.insert(id_b.clone(), member_b);
        group.members.insert(id_c.clone(), member_c);
        group.members.insert(id_d.clone(), member_d);

        let topo = DagSolver::topological_sort(&group).expect("Topo sort failed");
        assert_eq!(topo.order[0], id_a);
        assert!(topo.order[1..3].contains(&id_b));
        assert!(topo.order[1..3].contains(&id_c));
        assert_eq!(topo.order[3], id_d);
        assert_eq!(topo.critical_path_length, 3);
    }

    #[test]
    fn test_self_circular_dependency_detected() {
        let mut group = DownloadGroup::new("Self-cycle A→A");

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let id_a = member_a.id.clone();

        let member_a_bad = GroupMember::new(
            "http://example.com/a.zip".to_string(),
            vec![id_a.clone()],
        );

        group.members.insert(id_a.clone(), member_a_bad);

        let result = DagSolver::topological_sort(&group);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Circular"));
    }

    #[test]
    fn test_two_member_cycle_a_b_detected() {
        let mut group = DownloadGroup::new("Two-cycle A↔B");

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let id_a = member_a.id.clone();

        let member_b = GroupMember::new(
            "http://example.com/b.zip".to_string(),
            vec![id_a.clone()],
        );
        let id_b = member_b.id.clone();

        let member_a_bad = GroupMember::new(
            "http://example.com/a.zip".to_string(),
            vec![id_b.clone()],
        );

        group.members.insert(id_a.clone(), member_a_bad);
        group.members.insert(id_b.clone(), member_b);

        let result = DagSolver::topological_sort(&group);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_group() {
        let group = DownloadGroup::new("Empty");
        let result = DagSolver::topological_sort(&group);
        assert!(result.is_ok());
        assert!(result.unwrap().order.is_empty());
    }

    #[test]
    fn test_single_member_group() {
        let mut group = DownloadGroup::new("Single");
        let member = GroupMember::new("http://example.com/file.zip".to_string(), vec![]);
        let id = member.id.clone();
        group.members.insert(id.clone(), member);

        let topo = DagSolver::topological_sort(&group).expect("Topo sort failed");
        assert_eq!(topo.order.len(), 1);
        assert_eq!(topo.order[0], id);
    }

    #[test]
    fn test_complex_nested_dependencies() {
        let mut group = DownloadGroup::new("Complex DAG");

        let member_a = GroupMember::new("http://example.com/a.zip".to_string(), vec![]);
        let id_a = member_a.id.clone();

        let member_b = GroupMember::new("http://example.com/b.zip".to_string(), vec![id_a.clone()]);
        let id_b = member_b.id.clone();

        let member_c = GroupMember::new("http://example.com/c.zip".to_string(), vec![id_a.clone()]);
        let id_c = member_c.id.clone();

        let member_d = GroupMember::new(
            "http://example.com/d.zip".to_string(),
            vec![id_b.clone(), id_c.clone()],
        );
        let id_d = member_d.id.clone();

        let member_e = GroupMember::new(
            "http://example.com/e.zip".to_string(),
            vec![id_b.clone(), id_c.clone()],
        );
        let id_e = member_e.id.clone();

        let member_f = GroupMember::new(
            "http://example.com/f.zip".to_string(),
            vec![id_d.clone(), id_e.clone()],
        );
        let id_f = member_f.id.clone();

        group.members.insert(id_a.clone(), member_a);
        group.members.insert(id_b.clone(), member_b);
        group.members.insert(id_c.clone(), member_c);
        group.members.insert(id_d.clone(), member_d);
        group.members.insert(id_e.clone(), member_e);
        group.members.insert(id_f.clone(), member_f);

        let topo = DagSolver::topological_sort(&group).expect("Topo sort failed");
        
        assert_eq!(topo.order[0], id_a);
        assert!(topo.order[1..3].contains(&id_b));
        assert!(topo.order[1..3].contains(&id_c));
        let d_idx = topo.order.iter().position(|x| x == &id_d).unwrap();
        let e_idx = topo.order.iter().position(|x| x == &id_e).unwrap();
        assert!(d_idx > 2 && e_idx > 2);
        assert_eq!(topo.order[topo.order.len() - 1], id_f);
        assert_eq!(topo.critical_path_length, 4);
    }
}
