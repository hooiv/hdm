use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::{Component, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use bytes::Bytes;
use librqbit::api::{Api, ApiTorrentListOpts, TorrentIdOrHash};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Session, SessionOptions,
    SessionPersistenceConfig,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncSeek};
use url::Url;

const MAX_TORRENT_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_TORRENT_SOURCE_LEN: usize = 16 * 1024;
const SESSION_LISTEN_PORT_START: u16 = 42_000;
const SESSION_LISTEN_PORT_END: u16 = 42_100;
const SESSION_DEFER_WRITES_MB: usize = 64;
const AUTO_PAUSE_REASON_QUEUE: &str = "queue";
const AUTO_PAUSE_REASON_SEEDING_POLICY: &str = "seeding_policy";

const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "https://tracker.opentrackr.org:443/announce",
];

/// Full torrent status — mirrors what the frontend `TorrentStatus` type expects.
#[derive(Clone, Serialize)]
pub struct TorrentStatus {
    pub id: usize,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub uploaded: u64,
    pub progress_percent: f64,
    /// Download speed in bytes/s.
    pub speed_download: u64,
    /// Upload speed in bytes/s.
    pub speed_upload: u64,
    /// Number of currently-active (unchoked) peers.
    pub peers_live: usize,
    /// Total peers seen (connected + queued + connecting).
    pub peers_total: usize,
    /// Serialised state: "initializing" | "live" | "paused" | "error"
    pub state: String,
    /// Estimated seconds remaining, or None if not downloadable.
    pub eta_secs: Option<u64>,
    /// Upload / download ratio.
    pub ratio: f64,
    /// Queue priority: "high" | "normal" | "low".
    pub priority: String,
    /// True when this torrent is pinned and should not be auto-paused.
    pub pinned: bool,
    /// Why this torrent was auto-paused ("queue" | "seeding_policy"), if applicable.
    pub auto_pause_reason: Option<String>,
    /// Absolute path to the output folder.
    pub save_path: String,
    /// Non-None only when state == "error".
    pub error: Option<String>,
    pub finished: bool,
}

/// Per-file information inside a torrent.
#[derive(Clone, Serialize, Deserialize)]
pub struct TorrentFileInfo {
    pub id: usize,
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// Bytes already downloaded for this file.
    pub downloaded: u64,
    pub progress_percent: f64,
    /// Whether this file is selected for download.
    pub included: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddTorrentOutcome {
    pub id: usize,
    pub already_managed: bool,
}

// ──────────────────────────────────────────────────────────────────────────────

pub struct TorrentManager {
    session: Arc<Session>,
    api: Api,
    default_output_dir: PathBuf,
    queue_auto_paused: std::sync::Mutex<HashSet<usize>>,
    seed_started_at: std::sync::Mutex<HashMap<usize, Instant>>,
    auto_pause_reasons: std::sync::Mutex<HashMap<usize, String>>,
}

impl TorrentManager {
    /// Initialise a new session, storing downloaded files under `output_dir`.
    pub async fn new(output_dir: PathBuf) -> anyhow::Result<Self> {
        let output_dir = prepare_output_dir(output_dir)?;
        let persistence_dir = output_dir.join(".rqbit-session");
        std::fs::create_dir_all(&persistence_dir).with_context(|| {
            format!(
                "failed to create torrent persistence directory {}",
                persistence_dir.display()
            )
        })?;

        let session_opts = SessionOptions {
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(persistence_dir),
            }),
            listen_port_range: Some(SESSION_LISTEN_PORT_START..SESSION_LISTEN_PORT_END),
            enable_upnp_port_forwarding: true,
            defer_writes_up_to: Some(SESSION_DEFER_WRITES_MB),
            concurrent_init_limit: Some(suggested_concurrent_init_limit()),
            ..Default::default()
        };

        let session = match Session::new_with_opts(output_dir.clone(), session_opts).await {
            Ok(session) => session,
            Err(e) => {
                eprintln!(
                    "[torrent] advanced session init failed: {}. Falling back to defaults.",
                    e
                );
                Session::new(output_dir.clone())
                    .await
                    .context("failed to initialize fallback torrent session")?
            }
        };

        let api = Api::new(session.clone(), None);
        Ok(Self {
            session,
            api,
            default_output_dir: output_dir,
            queue_auto_paused: std::sync::Mutex::new(HashSet::new()),
            seed_started_at: std::sync::Mutex::new(HashMap::new()),
            auto_pause_reasons: std::sync::Mutex::new(HashMap::new()),
        })
    }

    // ── add ────────────────────────────────────────────────────────────────

    /// Add a torrent from a magnet link *or* an HTTP(S) URL pointing to a
    /// `.torrent` file.
    pub async fn add_magnet(
        &self,
        magnet_url: &str,
        save_path: Option<String>,
        paused: bool,
    ) -> anyhow::Result<AddTorrentOutcome> {
        let source = normalize_torrent_source(magnet_url)?;
        let source = if source.starts_with("magnet:") {
            augment_magnet_with_trackers(&source)?
        } else {
            source
        };

        let mut opts = default_add_torrent_options();
        opts.output_folder = normalize_save_path(save_path, &self.default_output_dir)?;
        opts.paused = paused;
        opts.overwrite = false;

        let response = self
            .session
            .add_torrent(AddTorrent::from_url(source), Some(opts))
            .await?;
        response_to_outcome(response)
    }

    /// Add a torrent from raw `.torrent` file bytes (e.g. from a file dialog).
    pub async fn add_torrent_bytes(
        &self,
        bytes: Bytes,
        save_path: Option<String>,
        paused: bool,
        only_files: Option<Vec<usize>>,
    ) -> anyhow::Result<AddTorrentOutcome> {
        validate_torrent_bytes(&bytes)?;

        let mut opts = default_add_torrent_options();
        opts.output_folder = normalize_save_path(save_path, &self.default_output_dir)?;
        opts.paused = paused;
        opts.overwrite = false;
        opts.only_files = sanitize_only_files(only_files);

        let response = self
            .session
            .add_torrent(AddTorrent::from_bytes(bytes), Some(opts))
            .await?;
        response_to_outcome(response)
    }

    // ── list / status ──────────────────────────────────────────────────────

    /// Return a full `TorrentStatus` for every managed torrent.
    pub fn get_torrents(&self) -> Vec<TorrentStatus> {
        // api_torrent_list_ext with with_stats=true populates the `stats` field
        // on each TorrentDetailsResponse, giving us speeds, progress, etc.
        let list = self
            .api
            .api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
        let settings = crate::settings::load_settings();
        let priority_overrides = settings.torrent_priority_overrides;
        let pinned_hashes = settings.torrent_pinned_hashes;
        let auto_pause_reasons = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let mut torrents = list
            .torrents
            .into_iter()
            .filter_map(|item| {
                let id = item.id?;
                let stats = item.stats?;

                let down_bps = stats
                    .live
                    .as_ref()
                    .map(|l| mib_per_sec_to_bps(l.download_speed.mbps))
                    .unwrap_or(0);
                let up_bps = stats
                    .live
                    .as_ref()
                    .map(|l| mib_per_sec_to_bps(l.upload_speed.mbps))
                    .unwrap_or(0);

                let peers_live = stats
                    .live
                    .as_ref()
                    .map(|l| l.snapshot.peer_stats.live)
                    .unwrap_or(0);
                let peers_total = stats
                    .live
                    .as_ref()
                    .map(|l| {
                        let ps = &l.snapshot.peer_stats;
                        ps.live + ps.queued + ps.connecting
                    })
                    .unwrap_or(0);

                let progress_percent = if stats.total_bytes > 0 {
                    stats.progress_bytes as f64 / stats.total_bytes as f64 * 100.0
                } else {
                    0.0
                };

                let remaining = stats.total_bytes.saturating_sub(stats.progress_bytes);
                let eta_secs = if down_bps > 0 && remaining > 0 {
                    Some(remaining / down_bps)
                } else {
                    None
                };

                let ratio = if stats.progress_bytes > 0 {
                    stats.uploaded_bytes as f64 / stats.progress_bytes as f64
                } else {
                    0.0
                };

                let state = format!("{}", stats.state);
                let priority = priority_label_for_info_hash(&item.info_hash, &priority_overrides);
                let pinned = is_pinned_info_hash(&item.info_hash, &pinned_hashes);
                let auto_pause_reason = if state == "paused" {
                    auto_pause_reasons.get(&id).cloned()
                } else {
                    None
                };

                Some(TorrentStatus {
                    id,
                    name: item.name.unwrap_or_else(|| "Fetching metadata…".into()),
                    info_hash: item.info_hash,
                    total_size: stats.total_bytes,
                    downloaded: stats.progress_bytes,
                    uploaded: stats.uploaded_bytes,
                    progress_percent,
                    speed_download: down_bps,
                    speed_upload: up_bps,
                    peers_live,
                    peers_total,
                    state,
                    eta_secs,
                    ratio,
                    priority: priority.to_string(),
                    pinned,
                    auto_pause_reason,
                    save_path: item.output_folder,
                    error: stats.error,
                    finished: stats.finished,
                })
            })
            .collect::<Vec<_>>();

        torrents.sort_by_key(|t| t.id);
        torrents
    }

    /// Return per-file details (name, size, downloaded bytes, included flag).
    pub fn get_torrent_files(&self, id: usize) -> anyhow::Result<Vec<TorrentFileInfo>> {
        let details = self
            .api
            .api_torrent_details(TorrentIdOrHash::Id(id))?;

        // per-file byte progress is in stats
        let file_progress: Vec<u64> = self
            .api
            .api_stats_v1(TorrentIdOrHash::Id(id))
            .map(|s| s.file_progress)
            .unwrap_or_default();

        let files = match details.files {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };

        let result = files
            .into_iter()
            .enumerate()
            .map(|(idx, f)| {
                let downloaded = file_progress.get(idx).copied().unwrap_or(0);
                let progress_percent = if f.length > 0 {
                    downloaded as f64 / f.length as f64 * 100.0
                } else {
                    0.0
                };
                TorrentFileInfo {
                    id: idx,
                    name: f.name,
                    size: f.length,
                    downloaded,
                    progress_percent,
                    included: f.included,
                }
            })
            .collect();

        Ok(result)
    }

    // ── actions ────────────────────────────────────────────────────────────

    pub async fn pause_torrent(&self, id: usize) -> anyhow::Result<()> {
        self.api
            .api_torrent_action_pause(TorrentIdOrHash::Id(id))
            .await?;
        let mut auto_paused = self
            .queue_auto_paused
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_paused.remove(&id);
        let mut seed_started = self
            .seed_started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        seed_started.remove(&id);
        let mut auto_pause_reasons = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_pause_reasons.remove(&id);
        Ok(())
    }

    pub async fn resume_torrent(&self, id: usize) -> anyhow::Result<()> {
        self.api
            .api_torrent_action_start(TorrentIdOrHash::Id(id))
            .await?;
        let mut auto_paused = self
            .queue_auto_paused
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_paused.remove(&id);
        let mut auto_pause_reasons = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_pause_reasons.remove(&id);
        Ok(())
    }

    /// Remove a torrent. If `delete_files` is true, the downloaded data is
    /// also deleted from disk.
    pub async fn remove_torrent(&self, id: usize, delete_files: bool) -> anyhow::Result<()> {
        if delete_files {
            self.api
                .api_torrent_action_delete(TorrentIdOrHash::Id(id))
                .await?;
        } else {
            self.api
                .api_torrent_action_forget(TorrentIdOrHash::Id(id))
                .await?;
        }
        let mut auto_paused = self
            .queue_auto_paused
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_paused.remove(&id);
        let mut seed_started = self
            .seed_started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        seed_started.remove(&id);
        let mut auto_pause_reasons = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        auto_pause_reasons.remove(&id);
        Ok(())
    }

    /// Select which file IDs should be downloaded (deselect the rest).
    pub async fn update_only_files(
        &self,
        id: usize,
        included_ids: Vec<usize>,
    ) -> anyhow::Result<()> {
        let details = self
            .api
            .api_torrent_details(TorrentIdOrHash::Id(id))?;
        let file_count = details.files.as_ref().map(|files| files.len()).unwrap_or(0);
        let set = normalize_included_file_ids(included_ids, file_count)?;
        self.api
            .api_torrent_action_update_only_files(TorrentIdOrHash::Id(id), &set)
            .await?;
        Ok(())
    }

    // ── streaming helpers ───────────────────────────────────────────────────

    /// Return the index of the largest file in the torrent (for streaming).
    pub fn get_largest_file_id(&self, id: usize) -> Option<usize> {
        let details = self
            .api
            .api_torrent_details(TorrentIdOrHash::Id(id))
            .ok()?;
        let files = details.files?;
        files
            .iter()
            .enumerate()
            .max_by_key(|(_, f)| f.length)
            .map(|(idx, _)| idx)
    }

    pub fn get_file_length(&self, torrent_id: usize, file_id: usize) -> Option<u64> {
        let details = self
            .api
            .api_torrent_details(TorrentIdOrHash::Id(torrent_id))
            .ok()?;
        let files = details.files?;
        files.get(file_id).map(|f| f.length)
    }

    /// Open a seekable byte-stream for the given file (for HTTP range serving).
    pub fn create_stream(
        &self,
        torrent_id: usize,
        file_id: usize,
    ) -> anyhow::Result<impl AsyncRead + AsyncSeek + Unpin + Send> {
        let stream = self
            .api
            .api_stream(TorrentIdOrHash::Id(torrent_id), file_id)?;
        Ok(stream)
    }

    /// Apply a global session download limit for torrents.
    /// `0` means unlimited.
    pub fn set_session_download_limit_kbps(&self, limit_kbps: u64) {
        let limit_bps = kbps_to_nonzero_bps(limit_kbps);
        self.session.ratelimits.set_download_bps(limit_bps);
    }

    /// Keep active torrents under `max_active` and auto-resume queue-managed
    /// paused torrents when slots are available.
    /// `max_active = 0` disables queue enforcement.
    pub async fn enforce_queue_limits(&self, max_active: usize) -> anyhow::Result<()> {
        let settings = crate::settings::load_settings();
        let priority_overrides = settings.torrent_priority_overrides;
        let pinned_hashes = settings.torrent_pinned_hashes;

        if max_active == 0 {
            let queue_paused_ids = {
                let auto_paused = self
                    .queue_auto_paused
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                auto_paused.iter().copied().collect::<Vec<_>>()
            };
            for id in queue_paused_ids {
                if let Err(e) = self.api.api_torrent_action_start(TorrentIdOrHash::Id(id)).await {
                    eprintln!(
                        "[torrent-queue] failed to resume queue-paused torrent {} after disabling queue management: {}",
                        id, e
                    );
                }
            }

            let mut auto_paused = self
                .queue_auto_paused
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_paused.clear();
            let mut auto_pause_reasons = self
                .auto_pause_reasons
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_pause_reasons.retain(|_, reason| reason != AUTO_PAUSE_REASON_QUEUE);
            return Ok(());
        }

        let statuses = self.get_torrents();
        let mut auto_paused = {
            let auto_paused_guard = self
                .queue_auto_paused
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_paused_guard.clone()
        };
        let mut auto_pause_reasons = {
            let auto_pause_reasons_guard = self
                .auto_pause_reasons
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_pause_reasons_guard.clone()
        };

        auto_paused.retain(|id| {
            statuses
                .iter()
                .any(|status| status.id == *id && is_auto_resume_candidate(status))
        });
        auto_pause_reasons.retain(|id, reason| {
            if reason != AUTO_PAUSE_REASON_QUEUE {
                return true;
            }
            statuses
                .iter()
                .any(|status| status.id == *id && status.state == "paused")
        });

        let pinned_resume_ids = collect_pinned_resume_ids(&statuses, &auto_paused, &pinned_hashes);
        for id in pinned_resume_ids {
            if let Err(e) = self.api.api_torrent_action_start(TorrentIdOrHash::Id(id)).await {
                eprintln!(
                    "[torrent-queue] failed to resume pinned torrent {}: {}",
                    id, e
                );
                continue;
            }
            auto_paused.remove(&id);
            auto_pause_reasons.remove(&id);
        }

        let active_statuses =
            sorted_active_non_pinned_statuses(&statuses, &priority_overrides, &pinned_hashes);

        if active_statuses.len() > max_active {
            let to_pause = active_statuses.into_iter().skip(max_active);
            for status in to_pause {
                if let Err(e) = self
                    .api
                    .api_torrent_action_pause(TorrentIdOrHash::Id(status.id))
                    .await
                {
                    eprintln!(
                        "[torrent-queue] failed to auto-pause {}: {}",
                        status.id, e
                    );
                    continue;
                }
                auto_paused.insert(status.id);
                auto_pause_reasons.insert(status.id, AUTO_PAUSE_REASON_QUEUE.to_string());
            }
        } else {
            let available_slots = max_active.saturating_sub(active_statuses.len());
            if available_slots > 0 {
                let resume_candidates = collect_resume_candidate_ids(
                    &statuses,
                    &auto_paused,
                    &priority_overrides,
                    &pinned_hashes,
                );

                for id in resume_candidates.into_iter().take(available_slots) {
                    if let Err(e) = self
                        .api
                        .api_torrent_action_start(TorrentIdOrHash::Id(id))
                        .await
                    {
                        eprintln!(
                            "[torrent-queue] failed to auto-resume {}: {}",
                            id, e
                        );
                        continue;
                    }
                    auto_paused.remove(&id);
                    auto_pause_reasons.remove(&id);
                }
            }
        }

        let refreshed = self.get_torrents();
        auto_paused.retain(|id| {
            refreshed
                .iter()
                .any(|status| status.id == *id && is_auto_resume_candidate(status))
        });
        auto_pause_reasons.retain(|id, reason| {
            if reason != AUTO_PAUSE_REASON_QUEUE {
                return true;
            }
            refreshed
                .iter()
                .any(|status| status.id == *id && status.state == "paused")
        });

        let mut auto_paused_guard = self
            .queue_auto_paused
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *auto_paused_guard = auto_paused;
        let mut auto_pause_reasons_guard = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *auto_pause_reasons_guard = auto_pause_reasons;

        Ok(())
    }

    /// Auto-pause completed torrents based on seeding ratio and/or seed time.
    pub async fn enforce_seeding_policy(
        &self,
        auto_stop: bool,
        ratio_limit: f64,
        seed_time_limit_mins: u32,
    ) -> anyhow::Result<()> {
        let statuses = self.get_torrents();
        let pinned_hashes = crate::settings::load_settings().torrent_pinned_hashes;
        let mut seed_started = {
            let seed_started_guard = self
                .seed_started_at
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            seed_started_guard.clone()
        };
        let mut auto_pause_reasons = {
            let auto_pause_reasons_guard = self
                .auto_pause_reasons
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_pause_reasons_guard.clone()
        };

        seed_started.retain(|id, _| {
            statuses.iter().any(|status| {
                status.id == *id
                    && status.finished
                    && status.state == "live"
                    && !is_pinned_info_hash(&status.info_hash, &pinned_hashes)
            })
        });

        if !auto_stop {
            let mut seed_started_guard = self
                .seed_started_at
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *seed_started_guard = seed_started;
            let mut auto_pause_reasons_guard = self
                .auto_pause_reasons
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            auto_pause_reasons_guard
                .retain(|_, reason| reason != AUTO_PAUSE_REASON_SEEDING_POLICY);
            return Ok(());
        }

        let ratio_threshold = normalized_seed_ratio_limit(ratio_limit);
        let time_limit = seed_time_limit_duration(seed_time_limit_mins);
        if ratio_threshold.is_none() && time_limit.is_none() {
            let mut seed_started_guard = self
                .seed_started_at
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *seed_started_guard = seed_started;
            return Ok(());
        }

        let now = Instant::now();
        for status in statuses
            .iter()
            .filter(|status| status.finished && status.state == "live")
            .filter(|status| !is_pinned_info_hash(&status.info_hash, &pinned_hashes))
        {
            let started_at = seed_started.entry(status.id).or_insert(now);
            let ratio_hit = ratio_threshold.is_some_and(|limit| status.ratio >= limit);
            let time_hit = time_limit.is_some_and(|limit| started_at.elapsed() >= limit);

            if ratio_hit || time_hit {
                if let Err(e) = self
                    .api
                    .api_torrent_action_pause(TorrentIdOrHash::Id(status.id))
                    .await
                {
                    eprintln!(
                        "[torrent-seeding] failed to auto-pause {}: {}",
                        status.id, e
                    );
                    continue;
                }
                seed_started.remove(&status.id);
                let mut queue_auto_paused = self
                    .queue_auto_paused
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                queue_auto_paused.remove(&status.id);
                auto_pause_reasons
                    .insert(status.id, AUTO_PAUSE_REASON_SEEDING_POLICY.to_string());
            }
        }

        let refreshed = self.get_torrents();
        seed_started.retain(|id, _| {
            refreshed
                .iter()
                .any(|status| {
                    status.id == *id
                        && status.finished
                        && status.state == "live"
                        && !is_pinned_info_hash(&status.info_hash, &pinned_hashes)
                })
        });
        auto_pause_reasons.retain(|id, reason| {
            if reason != AUTO_PAUSE_REASON_SEEDING_POLICY {
                return true;
            }
            refreshed
                .iter()
                .any(|status| status.id == *id && status.state == "paused")
        });

        let mut seed_started_guard = self
            .seed_started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *seed_started_guard = seed_started;
        let mut auto_pause_reasons_guard = self
            .auto_pause_reasons
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *auto_pause_reasons_guard = auto_pause_reasons;

        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers

fn suggested_concurrent_init_limit() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(3)
}

fn kbps_to_nonzero_bps(limit_kbps: u64) -> Option<NonZeroU32> {
    if limit_kbps == 0 {
        return None;
    }
    let bps = limit_kbps.saturating_mul(1024).min(u32::MAX as u64) as u32;
    NonZeroU32::new(bps)
}

fn is_queue_active(status: &TorrentStatus) -> bool {
    (status.state == "live" || status.state == "initializing") && !status.finished
}

fn is_auto_resume_candidate(status: &TorrentStatus) -> bool {
    status.state == "paused" && !status.finished && status.error.is_none()
}

fn is_pinned_info_hash(info_hash: &str, pinned_hashes: &HashSet<String>) -> bool {
    pinned_hashes.contains(&info_hash.to_ascii_lowercase())
}

fn collect_pinned_resume_ids(
    statuses: &[TorrentStatus],
    auto_paused: &HashSet<usize>,
    pinned_hashes: &HashSet<String>,
) -> Vec<usize> {
    auto_paused
        .iter()
        .copied()
        .filter_map(|id| statuses.iter().find(|status| status.id == id))
        .filter(|status| {
            is_auto_resume_candidate(status) && is_pinned_info_hash(&status.info_hash, pinned_hashes)
        })
        .map(|status| status.id)
        .collect()
}

fn sorted_active_non_pinned_statuses<'a>(
    statuses: &'a [TorrentStatus],
    overrides: &HashMap<String, String>,
    pinned_hashes: &HashSet<String>,
) -> Vec<&'a TorrentStatus> {
    let mut active_statuses = statuses
        .iter()
        .filter(|status| is_queue_active(status))
        .filter(|status| !is_pinned_info_hash(&status.info_hash, pinned_hashes))
        .collect::<Vec<_>>();
    active_statuses.sort_by_key(|status| queue_priority_sort_key(status, overrides, pinned_hashes));
    active_statuses
}

fn collect_resume_candidate_ids(
    statuses: &[TorrentStatus],
    auto_paused: &HashSet<usize>,
    overrides: &HashMap<String, String>,
    pinned_hashes: &HashSet<String>,
) -> Vec<usize> {
    let mut resume_candidates = auto_paused
        .iter()
        .copied()
        .filter_map(|id| statuses.iter().find(|status| status.id == id))
        .filter(|status| is_auto_resume_candidate(status))
        .collect::<Vec<_>>();
    resume_candidates.sort_by_key(|status| queue_priority_sort_key(status, overrides, pinned_hashes));
    resume_candidates.into_iter().map(|status| status.id).collect()
}

fn priority_label_for_info_hash<'a>(
    info_hash: &str,
    overrides: &'a HashMap<String, String>,
) -> &'a str {
    let normalized_hash = info_hash.to_ascii_lowercase();
    overrides
        .get(&normalized_hash)
        .and_then(|priority| crate::settings::normalize_torrent_priority_label(priority))
        .unwrap_or("normal")
}

fn queue_priority_rank(label: &str) -> u8 {
    match label {
        "high" => 0,
        "normal" => 1,
        "low" => 2,
        _ => 1,
    }
}

fn queue_priority_sort_key(
    status: &TorrentStatus,
    overrides: &HashMap<String, String>,
    pinned_hashes: &HashSet<String>,
) -> (u8, u8, usize) {
    let pinned_rank = if is_pinned_info_hash(&status.info_hash, pinned_hashes) {
        0
    } else {
        1
    };
    let label = priority_label_for_info_hash(&status.info_hash, overrides);
    (pinned_rank, queue_priority_rank(label), status.id)
}

fn normalized_seed_ratio_limit(ratio_limit: f64) -> Option<f64> {
    if ratio_limit.is_finite() && ratio_limit > 0.0 {
        Some(ratio_limit)
    } else {
        None
    }
}

fn seed_time_limit_duration(seed_time_limit_mins: u32) -> Option<Duration> {
    if seed_time_limit_mins == 0 {
        None
    } else {
        Some(Duration::from_secs(seed_time_limit_mins as u64 * 60))
    }
}

fn default_add_torrent_options() -> AddTorrentOptions {
    AddTorrentOptions {
        trackers: Some(DEFAULT_TRACKERS.iter().map(|v| (*v).to_string()).collect()),
        defer_writes: Some(true),
        ..Default::default()
    }
}

fn prepare_output_dir(output_dir: PathBuf) -> anyhow::Result<PathBuf> {
    let dir = if output_dir.is_absolute() {
        output_dir
    } else {
        std::env::current_dir()
            .context("failed to get current working directory")?
            .join(output_dir)
    };

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create torrent output directory {}", dir.display()))?;

    Ok(std::fs::canonicalize(&dir).unwrap_or(dir))
}

fn normalize_torrent_source(source: &str) -> anyhow::Result<String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        bail!("torrent source cannot be empty");
    }
    if trimmed.len() > MAX_TORRENT_SOURCE_LEN {
        bail!("torrent source is too long");
    }

    if looks_like_info_hash(trimmed) {
        return Ok(format!("magnet:?xt=urn:btih:{}", trimmed.to_ascii_lowercase()));
    }

    let url = Url::parse(trimmed).context("invalid torrent URL")?;
    match url.scheme() {
        "magnet" => {
            validate_magnet_has_btih(&url)?;
            Ok(url.to_string())
        }
        "http" | "https" => Ok(url.to_string()),
        other => bail!("unsupported torrent URL scheme: {}", other),
    }
}

fn validate_magnet_has_btih(url: &Url) -> anyhow::Result<()> {
    let has_btih = url.query_pairs().any(|(k, v)| {
        k.eq_ignore_ascii_case("xt")
            && v.to_ascii_lowercase().starts_with("urn:btih:")
    });

    if !has_btih {
        bail!("magnet link must contain xt=urn:btih:<hash>");
    }

    Ok(())
}

fn augment_magnet_with_trackers(magnet: &str) -> anyhow::Result<String> {
    let mut url = Url::parse(magnet).context("invalid magnet link")?;
    validate_magnet_has_btih(&url)?;

    let existing_trackers: HashSet<String> = url
        .query_pairs()
        .filter(|(k, _)| k.eq_ignore_ascii_case("tr"))
        .map(|(_, v)| v.into_owned())
        .collect();

    {
        let mut pairs = url.query_pairs_mut();
        for tracker in DEFAULT_TRACKERS {
            if !existing_trackers.contains(*tracker) {
                pairs.append_pair("tr", tracker);
            }
        }
    }

    Ok(url.to_string())
}

fn normalize_save_path(
    save_path: Option<String>,
    default_output_dir: &PathBuf,
) -> anyhow::Result<Option<String>> {
    let Some(raw_path) = save_path else {
        return Ok(None);
    };

    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 4096 {
        bail!("save path is too long");
    }

    let candidate = PathBuf::from(trimmed);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        if candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            bail!("relative save path must not contain '..'");
        }
        default_output_dir.join(candidate)
    };

    std::fs::create_dir_all(&candidate)
        .with_context(|| format!("failed to create save directory {}", candidate.display()))?;

    let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
    if !canonical.is_dir() {
        bail!("save path is not a directory");
    }

    Ok(Some(canonical.to_string_lossy().into_owned()))
}

fn sanitize_only_files(only_files: Option<Vec<usize>>) -> Option<Vec<usize>> {
    let mut only_files = only_files?;
    only_files.sort_unstable();
    only_files.dedup();
    if only_files.is_empty() {
        None
    } else {
        Some(only_files)
    }
}

fn normalize_included_file_ids(
    included_ids: Vec<usize>,
    file_count: usize,
) -> anyhow::Result<HashSet<usize>> {
    if file_count == 0 {
        bail!("torrent metadata is not ready; file selection is unavailable");
    }
    if included_ids.is_empty() {
        bail!("at least one file must remain selected");
    }

    let mut set = HashSet::with_capacity(included_ids.len());
    for id in included_ids {
        if id >= file_count {
            bail!(
                "file id {} is out of range for torrent with {} files",
                id,
                file_count
            );
        }
        set.insert(id);
    }

    if set.is_empty() {
        bail!("at least one file must remain selected");
    }

    Ok(set)
}

fn validate_torrent_bytes(bytes: &Bytes) -> anyhow::Result<()> {
    if bytes.is_empty() {
        bail!("torrent file is empty");
    }
    if bytes.len() > MAX_TORRENT_METADATA_BYTES {
        bail!(
            "torrent metadata exceeds {} MiB limit",
            MAX_TORRENT_METADATA_BYTES / (1024 * 1024)
        );
    }
    Ok(())
}

fn looks_like_info_hash(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Convert MiB/s (as returned by `librqbit` speed estimator) to bytes per second.
#[inline]
fn mib_per_sec_to_bps(mib_per_sec: f64) -> u64 {
    if !mib_per_sec.is_finite() || mib_per_sec <= 0.0 {
        return 0;
    }

    let bps = mib_per_sec * 1024.0 * 1024.0;
    if bps >= u64::MAX as f64 {
        u64::MAX
    } else {
        bps.round() as u64
    }
}

/// Extract add outcome metadata from an `AddTorrentResponse`.
fn response_to_outcome(response: AddTorrentResponse) -> anyhow::Result<AddTorrentOutcome> {
    match response {
        AddTorrentResponse::Added(id, _) => Ok(AddTorrentOutcome {
            id,
            already_managed: false,
        }),
        AddTorrentResponse::AlreadyManaged(id, _) => Ok(AddTorrentOutcome {
            id,
            already_managed: true,
        }),
        AddTorrentResponse::ListOnly(_) => {
            anyhow::bail!("Torrent was added in list-only mode, not started")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_mib_to_bps_correctly() {
        assert_eq!(mib_per_sec_to_bps(0.0), 0);
        assert_eq!(mib_per_sec_to_bps(1.0), 1_048_576);
        assert_eq!(mib_per_sec_to_bps(1.5), 1_572_864);
    }

    #[test]
    fn kbps_to_nonzero_bps_limits_correctly() {
        assert_eq!(kbps_to_nonzero_bps(0), None);
        assert_eq!(kbps_to_nonzero_bps(1).unwrap().get(), 1024);
        assert_eq!(
            kbps_to_nonzero_bps(u64::MAX).unwrap().get(),
            u32::MAX
        );
    }

    #[test]
    fn accepts_raw_info_hash_as_source() {
        let hash = "0123456789abcdef0123456789abcdef01234567";
        let normalized = normalize_torrent_source(hash).unwrap();
        assert_eq!(normalized, format!("magnet:?xt=urn:btih:{hash}"));
    }

    #[test]
    fn rejects_magnet_without_btih() {
        let err = normalize_torrent_source("magnet:?dn=ubuntu").unwrap_err();
        assert!(err.to_string().contains("xt=urn:btih"));
    }

    #[test]
    fn relative_save_path_blocks_parent_traversal() {
        let root = std::env::temp_dir().join("hyperstream_torrent_tests");
        let err = normalize_save_path(Some("..\\outside".to_string()), &root).unwrap_err();
        assert!(err.to_string().contains("must not contain '..'"));
    }

    #[test]
    fn normalize_included_file_ids_rejects_empty_selection() {
        let err = normalize_included_file_ids(Vec::new(), 3).unwrap_err();
        assert!(err
            .to_string()
            .contains("at least one file must remain selected"));
    }

    #[test]
    fn normalize_included_file_ids_rejects_out_of_range_ids() {
        let err = normalize_included_file_ids(vec![0, 4], 2).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn normalize_included_file_ids_deduplicates_valid_ids() {
        let set = normalize_included_file_ids(vec![1, 1, 0], 3).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&0));
        assert!(set.contains(&1));
    }

    #[test]
    fn queue_state_helpers_match_expected_rules() {
        let mut status = TorrentStatus {
            id: 1,
            name: "test".to_string(),
            info_hash: "abcd".to_string(),
            total_size: 10,
            downloaded: 5,
            uploaded: 0,
            progress_percent: 50.0,
            speed_download: 1,
            speed_upload: 0,
            peers_live: 1,
            peers_total: 1,
            state: "live".to_string(),
            eta_secs: Some(1),
            ratio: 0.0,
            priority: "normal".to_string(),
            pinned: false,
            auto_pause_reason: None,
            save_path: "x".to_string(),
            error: None,
            finished: false,
        };

        assert!(is_queue_active(&status));
        assert!(!is_auto_resume_candidate(&status));

        status.state = "paused".to_string();
        assert!(!is_queue_active(&status));
        assert!(is_auto_resume_candidate(&status));

        status.error = Some("oops".to_string());
        assert!(!is_auto_resume_candidate(&status));
    }

    #[test]
    fn seed_threshold_helpers_work() {
        assert_eq!(normalized_seed_ratio_limit(0.0), None);
        assert_eq!(normalized_seed_ratio_limit(-1.0), None);
        assert_eq!(normalized_seed_ratio_limit(1.5), Some(1.5));

        assert_eq!(seed_time_limit_duration(0), None);
        assert_eq!(
            seed_time_limit_duration(5).unwrap(),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn priority_sort_key_prefers_high_then_normal_then_low() {
        let mut overrides = HashMap::new();
        overrides.insert("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(), "low".into());
        overrides.insert("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(), "high".into());

        let a = TorrentStatus {
            id: 2,
            name: "a".into(),
            info_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            total_size: 0,
            downloaded: 0,
            uploaded: 0,
            progress_percent: 0.0,
            speed_download: 0,
            speed_upload: 0,
            peers_live: 0,
            peers_total: 0,
            state: "paused".into(),
            eta_secs: None,
            ratio: 0.0,
            priority: "low".into(),
            pinned: false,
            auto_pause_reason: None,
            save_path: String::new(),
            error: None,
            finished: false,
        };
        let b = TorrentStatus {
            id: 1,
            name: "b".into(),
            info_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            total_size: 0,
            downloaded: 0,
            uploaded: 0,
            progress_percent: 0.0,
            speed_download: 0,
            speed_upload: 0,
            peers_live: 0,
            peers_total: 0,
            state: "paused".into(),
            eta_secs: None,
            ratio: 0.0,
            priority: "high".into(),
            pinned: false,
            auto_pause_reason: None,
            save_path: String::new(),
            error: None,
            finished: false,
        };
        let pinned_hashes = HashSet::new();

        assert!(
            queue_priority_sort_key(&b, &overrides, &pinned_hashes)
                < queue_priority_sort_key(&a, &overrides, &pinned_hashes)
        );
    }

    #[test]
    fn priority_sort_key_prefers_pinned_torrents() {
        let overrides = HashMap::new();
        let mut pinned_hashes = HashSet::new();
        pinned_hashes.insert("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());

        let pinned = TorrentStatus {
            id: 2,
            name: "pinned".into(),
            info_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            total_size: 0,
            downloaded: 0,
            uploaded: 0,
            progress_percent: 0.0,
            speed_download: 0,
            speed_upload: 0,
            peers_live: 0,
            peers_total: 0,
            state: "paused".into(),
            eta_secs: None,
            ratio: 0.0,
            priority: "normal".into(),
            pinned: true,
            auto_pause_reason: None,
            save_path: String::new(),
            error: None,
            finished: false,
        };
        let unpinned = TorrentStatus {
            id: 1,
            name: "unpinned".into(),
            info_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            total_size: 0,
            downloaded: 0,
            uploaded: 0,
            progress_percent: 0.0,
            speed_download: 0,
            speed_upload: 0,
            peers_live: 0,
            peers_total: 0,
            state: "paused".into(),
            eta_secs: None,
            ratio: 0.0,
            priority: "normal".into(),
            pinned: false,
            auto_pause_reason: None,
            save_path: String::new(),
            error: None,
            finished: false,
        };

        assert!(
            queue_priority_sort_key(&pinned, &overrides, &pinned_hashes)
                < queue_priority_sort_key(&unpinned, &overrides, &pinned_hashes)
        );
    }

    #[test]
    fn pinned_resume_candidates_require_auto_paused_and_resume_eligible() {
        let mut auto_paused = HashSet::new();
        auto_paused.insert(1);
        auto_paused.insert(2);
        auto_paused.insert(3);
        auto_paused.insert(4);

        let mut pinned_hashes = HashSet::new();
        pinned_hashes.insert("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());
        pinned_hashes.insert("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
        pinned_hashes.insert("dddddddddddddddddddddddddddddddddddddddd".into());

        let statuses = vec![
            TorrentStatus {
                id: 1,
                name: "eligible".into(),
                info_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: true,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 2,
                name: "not-paused".into(),
                info_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "live".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: true,
                auto_pause_reason: None,
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 3,
                name: "unpinned".into(),
                info_hash: "cccccccccccccccccccccccccccccccccccccccc".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: false,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 4,
                name: "errored".into(),
                info_hash: "dddddddddddddddddddddddddddddddddddddddd".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: true,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: Some("boom".into()),
                finished: false,
            },
        ];

        let ids = collect_pinned_resume_ids(&statuses, &auto_paused, &pinned_hashes);
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn resume_candidates_follow_priority_order() {
        let mut auto_paused = HashSet::new();
        auto_paused.insert(1);
        auto_paused.insert(2);
        auto_paused.insert(3);
        auto_paused.insert(4);

        let mut overrides = HashMap::new();
        overrides.insert("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(), "low".into());
        overrides.insert("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(), "high".into());

        let pinned_hashes = HashSet::new();
        let statuses = vec![
            TorrentStatus {
                id: 1,
                name: "low".into(),
                info_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "low".into(),
                pinned: false,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 2,
                name: "high".into(),
                info_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "high".into(),
                pinned: false,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 3,
                name: "normal".into(),
                info_hash: "cccccccccccccccccccccccccccccccccccccccc".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: false,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: None,
                finished: false,
            },
            TorrentStatus {
                id: 4,
                name: "errored".into(),
                info_hash: "dddddddddddddddddddddddddddddddddddddddd".into(),
                total_size: 0,
                downloaded: 0,
                uploaded: 0,
                progress_percent: 0.0,
                speed_download: 0,
                speed_upload: 0,
                peers_live: 0,
                peers_total: 0,
                state: "paused".into(),
                eta_secs: None,
                ratio: 0.0,
                priority: "normal".into(),
                pinned: false,
                auto_pause_reason: Some(AUTO_PAUSE_REASON_QUEUE.into()),
                save_path: String::new(),
                error: Some("skip".into()),
                finished: false,
            },
        ];

        let ids = collect_resume_candidate_ids(&statuses, &auto_paused, &overrides, &pinned_hashes);
        assert_eq!(ids, vec![2, 3, 1]);
    }
}
