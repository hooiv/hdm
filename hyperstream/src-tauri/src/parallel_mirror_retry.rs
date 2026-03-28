//! Parallel Mirror Retry: Proactive segment shadowing and bandwidth arbitrage
//!
//! Monitors download progress to identify stalled or "straggler" segments,
//! forcing mid-segment splits and reassigning work to superior mirrors in real-time.
//!
//! ## How It Works
//!
//! 1. **Straggler Detection**: Compares each segment's speed against the download average.
//! 2. **AI-Driven Prediction**: Uses `FailurePredictionEngine` to trigger splits *before* a
//!    segment officially fails.
//! 3. **Shadow Worker**: The new split segment is assigned to the fastest *alternative* mirror,
//!    not the one already struggling.

use crate::core_state::AppState;
use crate::downloader::structures::{Segment, SegmentState};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParallelRetryConfig {
    /// Speed threshold relative to average (e.g., 0.3 means <30% of average)
    pub straggler_speed_ratio: f64,
    /// Minimum time a segment must be in `Downloading` state before being judged
    pub min_evaluation_time_ms: u64,
    /// Stall timeout: no progress for this many seconds triggers intervention
    pub stall_timeout_s: u64,
    /// Max segments per download (prevents unbounded splitting)
    pub max_segments_per_download: u32,
}

impl Default for ParallelRetryConfig {
    fn default() -> Self {
        Self {
            straggler_speed_ratio: 0.4,     // <40% of average = straggling
            min_evaluation_time_ms: 10_000, // 10s before judging a new segment
            stall_timeout_s: 15,
            max_segments_per_download: 32,
        }
    }
}

pub struct ParallelMirrorRetryManager {
    config: ParallelRetryConfig,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum SplitReason {
    Straggler,
    Predictive,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SplitAction {
    pub segment: Segment,
    pub mirror_url: String,
    pub reason: SplitReason,
}

impl ParallelMirrorRetryManager {
    pub fn new(config: ParallelRetryConfig) -> Self {
        Self { config }
    }

    /// Evaluates a single download for stragglers and AI-predicted failures,
    /// triggering proactive segment splits as needed.
    ///
    /// Returns a list of `SplitAction`s, each describing the new shadow segment
    /// and the mirror it should use.
    pub async fn evaluate_and_split(&self, download_id: &str, state: &AppState) -> Vec<SplitAction> {
        let session = {
            let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            match downloads.get(download_id) {
                Some(s) => s.clone(),
                None => return Vec::new(),
            }
        };

        let manager = session.manager.clone();
        let stats = manager.lock().unwrap().get_stats();

        if stats.active_segments <= 1 || stats.total_speed_bps == 0 {
            return Vec::new();
        }

        let avg_speed = stats.total_speed_bps / stats.active_segments as u64;
        let slow_threshold = (avg_speed as f64 * self.config.straggler_speed_ratio) as u64;

        let segments = manager.lock().unwrap().get_segments_snapshot();

        // ── Resolve mirrors once — before taking any per-segment locks ──────
        // Fetching mirrors is async and must not occur while holding a Mutex.
        let all_mirrors = state.mirror_aggregator.get_active_mirrors(download_id).await;

        // FIX: Select best alternative mirror — skip the session's primary URL
        // so that the shadow worker actually uses a different server.
        let best_mirror = all_mirrors
            .iter()
            .find(|m| m.url != session.url)
            .map(|m| m.url.clone())
            .unwrap_or_else(|| session.url.clone());

        let mut results: Vec<SplitAction> = Vec::new();

        // ── AI-Driven Predictive Failover ─────────────────────────────────────
        let prediction = state
            .failure_prediction_engine
            .lock()
            .ok()
            .and_then(|engine| engine.predict_failure(download_id));

        if let Some(ref prediction) = prediction {
            use crate::failure_prediction::FailureRisk;
            if matches!(
                prediction.risk_level,
                FailureRisk::Critical | FailureRisk::Imminent
            ) {
                println!(
                    "[PredictiveRetry] AI risk {}% ({:?}) for '{}' — triggering shadow split.",
                    prediction.probability_percent, prediction.risk_level, download_id
                );

                // Shadow the slowest active segment
                if let Some(slowest) = segments
                    .iter()
                    .filter(|s| s.state == SegmentState::Downloading)
                    .min_by_key(|s| s.speed_bps)
                {
                    let new_seg_opt = {
                        let m = manager.lock().unwrap_or_else(|e| e.into_inner());
                        let min_split = m.config.min_split_size;
                        if slowest.remaining() > min_split * 2 {
                            m.trigger_proactive_split(slowest.id)
                        } else {
                            None
                        }
                    };

                    if let Some(new_seg) = new_seg_opt {
                        results.push(SplitAction {
                            segment: new_seg,
                            mirror_url: best_mirror.clone(),
                            reason: SplitReason::Predictive,
                        });
                    }
                }
            }
        }

        // ── Reactive Straggler Detection ──────────────────────────────────────
        for seg in &segments {
            if seg.state != SegmentState::Downloading {
                continue;
            }

            // Skip segments already handled in the predictive pass
            if results.iter().any(|r| r.segment.id == seg.id) {
                continue;
            }

            let is_straggler = seg.speed_bps < slow_threshold;
            let has_enough_remaining = {
                let m = manager.lock().unwrap_or_else(|e| e.into_inner());
                seg.remaining() > m.config.min_split_size * 2
            };

            if is_straggler && has_enough_remaining {
                println!(
                    "[ParallelRetry] Seg {} straggling ({} bps < threshold {} bps) — splitting.",
                    seg.id, seg.speed_bps, slow_threshold
                );

                let new_seg_opt = {
                    let m = manager.lock().unwrap_or_else(|e| e.into_inner());
                    m.trigger_proactive_split(seg.id)
                };

                if let Some(new_seg) = new_seg_opt {
                    results.push(SplitAction {
                        segment: new_seg,
                        mirror_url: best_mirror.clone(),
                        reason: SplitReason::Straggler,
                    });
                }
            }
        }

        results
    }
}
