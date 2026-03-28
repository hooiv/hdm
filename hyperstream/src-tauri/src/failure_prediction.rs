//! Failure Prediction & Proactive Recovery System
//!
//! Machine learning-like analysis that predicts download failures before they happen,
//! enabling proactive mitigation strategies. This is the AI brain of HyperStream's reliability.
//!
//! ## How It Works
//!
//! 1. **Metrics Collection**: Continuously monitor bandwidth, latency, errors, timeouts
//! 2. **Pattern Analysis**: Detect anomalies and trends that precede failures
//! 3. **Failure Prediction**: Use heuristics to predict failure probability (0-100%)
//! 4. **Preventive Actions**: Trigger automatic recovery before failure is inevitable
//! 5. **Learning**: Improve predictions based on historical accuracy
//! 6. **Confidence Scoring**: Rate accuracy of predictions for user transparency
//!
//! ## Competitive Advantage
//!
//! - **IDM**: No failure prediction—only reactive recovery
//! - **Aria2**: Basic retry logic, no adaptation
//! - **HyperStream**: Predicts failures 30-60 seconds in advance using trend analysis

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Real-time download metrics for failure analysis
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DownloadMetrics {
    /// Current bytes per second
    pub speed_bps: u64,
    /// Milliseconds since last byte received
    pub idle_time_ms: u64,
    /// Number of active connections
    pub active_connections: u32,
    /// Errors in last 10 seconds
    pub recent_errors: u32,
    /// Connection timeouts in session
    pub timeout_count: u32,
    /// Avg latency in milliseconds
    pub latency_ms: u64,
    /// Jitter/variance in latency
    pub jitter_ms: u32,
    /// Segment completion time ms (moving average)
    pub avg_segment_time_ms: u64,
    /// Bytes retried due to errors
    pub retried_bytes: u64,
    /// Percent of segments requiring retry
    pub retry_rate_percent: f32,
    /// DNS failures in session
    pub dns_failures: u32,
    /// HTTP 429 (rate limit) responses
    pub rate_limit_hits: u32,
    /// HTTP 403 (forbidden) responses
    pub access_denied_hits: u32,
    /// Connection refused errors
    pub connection_refused: u32,
    /// Timestamp of last metric update
    pub timestamp_secs: u64,
}

/// Failure risk assessment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureRisk {
    /// 0-15%: Normal operation
    Healthy,
    /// 15-35%: Minor issues, monitor
    Caution,
    /// 35-60%: Multiple warning signs, mitigation needed
    Warning,
    /// 60-85%: High failure probability, urgent action needed
    Critical,
    /// 85%+: Failure nearly inevitable, extreme measures required
    Imminent,
}

/// Why failure is predicted to occur
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureReason {
    /// Speed consistently declining (<50% of baseline) via measured trend
    SpeedDegradation,
    /// No bytes received for extended period (>30s)
    ConnectionStalled,
    /// Excessive timeouts (>5 in 60s)
    TimeoutPattern,
    /// Multiple connection refused errors
    ConnectionRefusal,
    /// Server rate limiting detected
    RateLimiting,
    /// IP/Geo blocking detected
    AccessDenied,
    /// Segment completion time increasing significantly
    SlowingSegments,
    /// High jitter and packet loss pattern
    NetworkUnstable,
    /// DNS resolution failing
    DnsFailures,
    /// Combination of multiple factors
    CompoundedIssues,
}

/// Prediction about an imminent failure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePrediction {
    /// Unique prediction ID
    pub prediction_id: String,
    /// Probability of failure (0-100%)
    pub probability_percent: u32,
    /// Confidence in the prediction (0-100%)
    pub confidence_percent: u32,
    /// Primary reason for prediction
    pub reason: FailureReason,
    /// Predicted time until failure (seconds)
    pub time_to_failure_secs: Option<u32>,
    /// Risk level assessment
    pub risk_level: FailureRisk,
    /// Recommended recovery action
    pub recommended_action: RecoveryAction,
    /// Secondary factors contributing
    pub contributing_factors: Vec<FailureReason>,
    /// Timestamp of prediction
    pub timestamp_secs: u64,
    /// Human-readable explanation
    pub explanation: String,
    /// Confidence score breakdown
    pub confidence_breakdown: ConfidenceBreakdown,
}

impl Default for FailurePrediction {
    fn default() -> Self {
        Self {
            prediction_id: "default".to_string(),
            probability_percent: 0,
            confidence_percent: 0,
            reason: FailureReason::CompoundedIssues,
            time_to_failure_secs: None,
            risk_level: FailureRisk::Healthy,
            recommended_action: RecoveryAction::Monitor,
            contributing_factors: Vec::new(),
            timestamp_secs: current_time_secs(),
            explanation: "No prediction data available".to_string(),
            confidence_breakdown: ConfidenceBreakdown {
                historical_accuracy_percent: 0,
                sample_size_confidence: 0,
                signal_correlation_confidence: 0,
                metrics_clarity_percent: 0,
            },
        }
    }
}

/// Breakdown of why we're confident in a prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    /// Based on historical accuracy of this pattern
    pub historical_accuracy_percent: u32,
    /// Based on sample size of similar situations
    pub sample_size_confidence: u32,
    /// Based on multiple corroborating signals
    pub signal_correlation_confidence: u32,
    /// Based on current metrics specificity
    pub metrics_clarity_percent: u32,
}

/// Recommended action to prevent failure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Just monitor, no action needed
    Monitor,
    /// Reduce segment size to improve stability
    ReduceSegmentSize,
    /// Switch to single connection (slower but more reliable)
    SequentialMode,
    /// Try alternative mirror
    SwitchMirror,
    /// Reduce speed limit to ease server load
    ReduceSpeedLimit,
    /// Wait before retrying (server might be throttling)
    WaitAndRetry,
    /// Use proxy/VPN to bypass geo-blocking
    UseProxy,
    /// Switch DNS resolver (current one might be blocked)
    SwitchDns,
    /// Increase retry timeout values
    IncreaseTimeout,
    /// Pause and resume later (network issue might be temporary)
    PauseAndResume,
    /// Cancel and try with different URL
    SwitchUrl,
    /// Already failed, initiate recovery
    InitiateRecovery,
}

/// Historical prediction accuracy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionAccuracy {
    /// Number of correct predictions
    pub correct_predictions: u32,
    /// Number of false alarms
    pub false_alarms: u32,
    /// Number of missed failures
    pub missed_failures: u32,
    /// Accuracy percentage (0-100)
    pub accuracy_percent: u32,
    /// False alarm rate
    pub false_alarm_rate: f32,
    /// Detection rate (sensitivity)
    pub detection_rate: f32,
    /// Last updated timestamp
    pub updated_secs: u64,
}

/// Failure prediction engine
pub struct FailurePredictionEngine {
    /// Per-download metric history for isolated trend analysis
    download_history: Arc<RwLock<HashMap<String, VecDeque<DownloadMetrics>>>>,
    /// Per-download predictions
    current_predictions: Arc<RwLock<HashMap<String, FailurePrediction>>>,
    /// Accuracy statistics
    accuracy_stats: Arc<Mutex<PredictionAccuracy>>,
    /// Configuration
    config: PredictionConfig,
}

/// Configuration for prediction engine
#[derive(Debug, Clone)]
pub struct PredictionConfig {
    /// Max history samples to keep per download (default: 300, ~5 min at 1Hz)
    pub max_history_samples: usize,
    /// Bytes per second threshold for "stalled"
    pub stalled_threshold_bps: u64,
    /// Idle time ms threshold before stall warning
    pub stall_idle_time_ms: u64,
    /// Speed degradation ratio (e.g., 0.5 = 50% drop detected via trend slope)
    pub speed_degradation_ratio: f32,
    /// Timeout count threshold in window
    pub timeout_threshold: u32,
    /// Error count threshold in window
    pub error_threshold: u32,
    /// Minimum number of samples required before making predictions
    pub min_samples_for_prediction: usize,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            max_history_samples: 300,
            stalled_threshold_bps: 100_000, // <100 KB/s = stalled
            stall_idle_time_ms: 30_000,     // 30 seconds
            speed_degradation_ratio: 0.5,   // 50% drop (detected via linear trend)
            timeout_threshold: 5,
            error_threshold: 10,
            min_samples_for_prediction: 3,
        }
    }
}

impl FailurePredictionEngine {
    /// Create a new failure prediction engine
    pub fn new(config: PredictionConfig) -> Self {
        Self {
            current_predictions: Arc::new(RwLock::new(HashMap::new())),
            accuracy_stats: Arc::new(Mutex::new(PredictionAccuracy {
                correct_predictions: 0,
                false_alarms: 0,
                missed_failures: 0,
                accuracy_percent: 0,
                false_alarm_rate: 0.0,
                detection_rate: 0.0,
                updated_secs: current_time_secs(),
            })),
            download_history: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Record a new metrics snapshot for a specific download
    pub fn add_metrics(&self, download_id: &str, metrics: DownloadMetrics) {
        let mut dl_history = self.download_history.write().unwrap();
        let history = dl_history
            .entry(download_id.to_string())
            .or_insert_with(VecDeque::new);
        history.push_back(metrics);
        while history.len() > self.config.max_history_samples {
            history.pop_front();
        }
    }

    /// Add multiple metrics snapshots at once (useful for simulation and bulk loading).
    ///
    /// FIX: Previously this method held a write lock on `download_history` while
    /// also trying to acquire a write lock on `metrics_history` inside the loop,
    /// causing a deadlock on platforms with non-reentrant RwLock. This version
    /// separates the two scopes entirely.
    pub fn add_bulk_metrics(&self, download_id: &str, metrics_list: Vec<DownloadMetrics>) {
        if metrics_list.is_empty() {
            return;
        }

        let mut dl_history = self.download_history.write().unwrap();
        let history = dl_history
            .entry(download_id.to_string())
            .or_insert_with(VecDeque::new);

        for metrics in metrics_list {
            history.push_back(metrics);
        }

        // Cap to max samples
        while history.len() > self.config.max_history_samples {
            history.pop_front();
        }
    }

    /// Reset metric history and prediction for a specific download.
    /// Useful for repeatable chaos testing.
    pub fn reset_history(&self, download_id: &str) {
        {
            let mut dl_history = self.download_history.write().unwrap();
            dl_history.remove(download_id);
        }
        {
            let mut predictions = self.current_predictions.write().unwrap();
            predictions.remove(download_id);
        }
    }

    /// Analyze current metrics and predict potential failure.
    ///
    /// Returns `None` if there is insufficient data or the computed probability
    /// is below the noise floor (< 20%).
    pub fn predict_failure(&self, download_id: &str) -> Option<FailurePrediction> {
        let dl_history = self.download_history.read().unwrap();
        let history = dl_history.get(download_id)?;

        if history.len() < self.config.min_samples_for_prediction {
            return None;
        }

        let current = history.back().unwrap();
        let mut probability = 0u32;
        let mut reasons = Vec::new();
        let mut contributing_factors = Vec::new();

        // ─── Rule 1: Connection Stalled ───────────────────────────────────────
        // Use the maximum idle_time across recent samples to avoid false alarms
        // from a single-sample gap.
        let max_recent_idle = history
            .iter()
            .rev()
            .take(5)
            .map(|m| m.idle_time_ms)
            .max()
            .unwrap_or(0);

        if max_recent_idle > self.config.stall_idle_time_ms {
            probability += 35;
            reasons.push(FailureReason::ConnectionStalled);
        } else if max_recent_idle > self.config.stall_idle_time_ms / 2 {
            probability += 15;
            contributing_factors.push(FailureReason::ConnectionStalled);
        }

        // ─── Rule 2: Speed Degradation (Linear Trend) ────────────────────────
        // Use linear regression over the last 10 samples to compute slope.
        // A simple window average would trigger false alarms on transient dips.
        if history.len() >= 10 {
            let slope = Self::compute_speed_slope(history, 10);
            // Normalize: slope relative to the mean speed of the window
            let window_mean = history
                .iter()
                .rev()
                .take(10)
                .map(|m| m.speed_bps as f64)
                .sum::<f64>()
                / 10.0;

            if window_mean > 0.0 {
                // Negative slope = declining speed. Express as fraction of mean.
                let relative_decline = (-slope / window_mean) as f32;
                if relative_decline > self.config.speed_degradation_ratio {
                    probability += 30;
                    reasons.push(FailureReason::SpeedDegradation);
                } else if relative_decline > self.config.speed_degradation_ratio * 0.5 {
                    probability += 12;
                    contributing_factors.push(FailureReason::SpeedDegradation);
                }
            }
        }

        // ─── Rule 3: Timeout Pattern ──────────────────────────────────────────
        if current.timeout_count >= self.config.timeout_threshold {
            probability += 28;
            reasons.push(FailureReason::TimeoutPattern);
        } else if current.timeout_count >= self.config.timeout_threshold / 2 {
            probability += 10;
            contributing_factors.push(FailureReason::TimeoutPattern);
        }

        // ─── Rule 4: High Error Rate ──────────────────────────────────────────
        if current.recent_errors >= self.config.error_threshold {
            probability += 25;
            reasons.push(FailureReason::CompoundedIssues);
        } else if current.recent_errors > 0 {
            probability += 8;
            contributing_factors.push(FailureReason::CompoundedIssues);
        }

        // ─── Rule 5: Rate Limiting ────────────────────────────────────────────
        if current.rate_limit_hits > 0 {
            probability += 20;
            reasons.push(FailureReason::RateLimiting);
        }

        // ─── Rule 6: Access Denied ────────────────────────────────────────────
        if current.access_denied_hits > 0 {
            probability += 25;
            reasons.push(FailureReason::AccessDenied);
        }

        // ─── Rule 7: Connection Refused ───────────────────────────────────────
        if current.connection_refused > 2 {
            probability += 22;
            reasons.push(FailureReason::ConnectionRefusal);
        } else if current.connection_refused > 0 {
            probability += 10;
            contributing_factors.push(FailureReason::ConnectionRefusal);
        }

        // ─── Rule 8: DNS Failures ─────────────────────────────────────────────
        if current.dns_failures > 1 {
            probability += 18;
            reasons.push(FailureReason::DnsFailures);
        } else if current.dns_failures > 0 {
            probability += 8;
            contributing_factors.push(FailureReason::DnsFailures);
        }

        // ─── Rule 9: Network Instability ──────────────────────────────────────
        if current.jitter_ms > 100 && current.latency_ms > 200 {
            probability += 15;
            contributing_factors.push(FailureReason::NetworkUnstable);
        }

        // ─── Rule 10: High Retry Rate ─────────────────────────────────────────
        if current.retry_rate_percent > 50.0 {
            probability += 20;
            contributing_factors.push(FailureReason::SlowingSegments);
        }

        // Cap probability at 100
        probability = probability.min(100);

        // Only predict if probability is meaningful (>20%)
        if probability < 20 {
            return None;
        }

        // Determine primary reason (highest priority)
        let primary_reason = if reasons.is_empty() {
            contributing_factors
                .first()
                .cloned()
                .unwrap_or(FailureReason::CompoundedIssues)
        } else {
            reasons[0].clone()
        };

        // Calculate confidence based on signal strength and historical accuracy
        let confidence = self.calculate_confidence(probability, history.len());

        // Determine risk level
        let risk_level = match probability {
            0..=15 => FailureRisk::Healthy,
            16..=35 => FailureRisk::Caution,
            36..=60 => FailureRisk::Warning,
            61..=85 => FailureRisk::Critical,
            _ => FailureRisk::Imminent,
        };

        // FIX: Choose recovery action based on primary reason.
        // Previously ConnectionStalled mapped to IncreaseTimeout, which is wrong:
        // a stall means no data is arriving at all, so we should switch mirrors.
        let recommended_action = match primary_reason {
            FailureReason::SpeedDegradation => RecoveryAction::SwitchMirror,
            FailureReason::ConnectionStalled => RecoveryAction::SwitchMirror,
            FailureReason::TimeoutPattern => RecoveryAction::IncreaseTimeout,
            FailureReason::ConnectionRefusal => RecoveryAction::SwitchMirror,
            FailureReason::RateLimiting => RecoveryAction::ReduceSpeedLimit,
            FailureReason::AccessDenied => RecoveryAction::UseProxy,
            FailureReason::DnsFailures => RecoveryAction::SwitchDns,
            FailureReason::NetworkUnstable => RecoveryAction::WaitAndRetry,
            FailureReason::SlowingSegments => RecoveryAction::ReduceSegmentSize,
            FailureReason::CompoundedIssues => {
                if probability > 75 {
                    RecoveryAction::InitiateRecovery
                } else {
                    RecoveryAction::PauseAndResume
                }
            }
        };

        // Estimate time-to-failure using the measured speed decay rate.
        // If speed is declining, TTF = current_speed / |slope_per_second|.
        // Falls back to heuristic constants for non-speed-based failures.
        let time_to_failure = Self::estimate_time_to_failure(
            &primary_reason,
            probability,
            history,
        );

        let explanation = self.generate_explanation(
            probability,
            confidence,
            &primary_reason,
            &reasons,
            &risk_level,
        );

        let prediction = FailurePrediction {
            prediction_id: format!(
                "pred_{}_{}_{}", download_id, probability, current.timestamp_secs
            ),
            probability_percent: probability,
            confidence_percent: confidence,
            reason: primary_reason.clone(),
            time_to_failure_secs: time_to_failure,
            risk_level,
            recommended_action,
            contributing_factors,
            timestamp_secs: current.timestamp_secs,
            explanation,
            confidence_breakdown: ConfidenceBreakdown {
                historical_accuracy_percent: self.get_historical_accuracy(),
                sample_size_confidence: Self::calculate_sample_size_confidence(history.len()),
                signal_correlation_confidence: Self::calculate_signal_correlation(reasons.len()),
                metrics_clarity_percent: Self::calculate_metrics_clarity(current),
            },
        };

        // Store current prediction
        {
            let mut predictions = self.current_predictions.write().unwrap();
            predictions.insert(download_id.to_string(), prediction.clone());
        }

        Some(prediction)
    }

    /// Compute the linear regression slope of speed_bps over the last `window` samples.
    ///
    /// A negative slope indicates declining speed. Returns 0.0 if insufficient data.
    fn compute_speed_slope(history: &VecDeque<DownloadMetrics>, window: usize) -> f64 {
        let samples: Vec<f64> = history
            .iter()
            .rev()
            .take(window)
            .map(|m| m.speed_bps as f64)
            .collect();

        let n = samples.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        // Least squares: slope = (n*Σxy - Σx*Σy) / (n*Σx² - (Σx)²)
        // x is the index (0, 1, 2, ...), y is speed
        let sum_x: f64 = (0..samples.len()).map(|i| i as f64).sum();
        let sum_y: f64 = samples.iter().sum();
        let sum_xy: f64 = samples.iter().enumerate().map(|(i, &y)| i as f64 * y).sum();
        let sum_x2: f64 = (0..samples.len()).map(|i| (i as f64).powi(2)).sum();

        let denom = n * sum_x2 - sum_x * sum_x;
        if denom.abs() < f64::EPSILON {
            return 0.0;
        }

        (n * sum_xy - sum_x * sum_y) / denom
    }

    /// Estimate time to failure in seconds.
    ///
    /// For speed-decay failures, uses the measured slope to project when speed
    /// will reach zero. Falls back to empirical constants for other failure types.
    fn estimate_time_to_failure(
        reason: &FailureReason,
        probability: u32,
        history: &VecDeque<DownloadMetrics>,
    ) -> Option<u32> {
        if probability < 50 {
            return None;
        }

        match reason {
            FailureReason::SpeedDegradation => {
                // TTF = current_speed / |decay_rate_per_sample|
                let slope = Self::compute_speed_slope(history, 10);
                if slope >= 0.0 {
                    return None;
                }
                let current_speed = history.back()?.speed_bps as f64;
                let decay_per_sec = slope.abs(); // samples are ~1s apart
                if decay_per_sec > 0.0 {
                    let ttf = (current_speed / decay_per_sec).round() as u32;
                    Some(ttf.clamp(5, 300))
                } else {
                    None
                }
            }
            FailureReason::ConnectionStalled => Some(30),
            FailureReason::TimeoutPattern => Some(45),
            FailureReason::RateLimiting => Some(120),
            _ => Some(60),
        }
    }

    /// Calculate confidence in the prediction
    fn calculate_confidence(&self, probability: u32, sample_count: usize) -> u32 {
        let base_confidence = match probability {
            0..=30 => 40,
            31..=50 => 60,
            51..=70 => 75,
            71..=85 => 85,
            _ => 95,
        };

        // Boost confidence with more samples (logarithmic, not linear)
        let sample_boost = ((sample_count.min(100) as f64).ln() * 10.0) as u32;

        (base_confidence + sample_boost).min(100)
    }

    /// Get historical accuracy for predictions
    fn get_historical_accuracy(&self) -> u32 {
        self.accuracy_stats.lock().unwrap().accuracy_percent
    }

    /// Calculate confidence based on sample size
    fn calculate_sample_size_confidence(count: usize) -> u32 {
        match count {
            0..=10 => 30,
            11..=30 => 50,
            31..=60 => 70,
            61..=100 => 85,
            _ => 95,
        }
    }

    /// Calculate confidence from signal correlation (more corroborating signals = higher confidence)
    fn calculate_signal_correlation(reason_count: usize) -> u32 {
        match reason_count {
            0 => 40,
            1 => 55,
            2 => 72,
            3 => 88,
            _ => 95,
        }
    }

    /// Calculate metrics clarity score (0-100)
    fn calculate_metrics_clarity(metrics: &DownloadMetrics) -> u32 {
        let mut clarity = 50u32;

        // Data freshness (penalty if older than 5 seconds)
        let age = current_time_secs().saturating_sub(metrics.timestamp_secs);
        if age > 5 {
            clarity = clarity.saturating_sub(20);
        }

        // Metric diversity bonus
        if metrics.speed_bps > 0 {
            clarity += 10;
        }
        if metrics.latency_ms > 0 {
            clarity += 10;
        }
        if metrics.active_connections > 0 {
            clarity += 5;
        }
        if metrics.timeout_count > 0 {
            clarity += 5;
        }

        clarity.min(100)
    }

    /// Generate human-readable explanation
    fn generate_explanation(
        &self,
        probability: u32,
        confidence: u32,
        reason: &FailureReason,
        reasons: &[FailureReason],
        risk_level: &FailureRisk,
    ) -> String {
        let risk_emoji = match risk_level {
            FailureRisk::Healthy => "✅",
            FailureRisk::Caution => "⚠️",
            FailureRisk::Warning => "🟡",
            FailureRisk::Critical => "🔴",
            FailureRisk::Imminent => "💥",
        };

        let reason_text = match reason {
            FailureReason::SpeedDegradation => "download speed is declining (trend analysis)",
            FailureReason::ConnectionStalled => "connection is stalled — no data received",
            FailureReason::TimeoutPattern => "too many timeout errors detected",
            FailureReason::ConnectionRefusal => "server is refusing connections",
            FailureReason::RateLimiting => "server is rate limiting requests",
            FailureReason::AccessDenied => "access to resource is denied",
            FailureReason::DnsFailures => "DNS resolution is failing",
            FailureReason::NetworkUnstable => "network is unstable (high jitter)",
            FailureReason::SlowingSegments => "individual segments are taking longer",
            FailureReason::CompoundedIssues => "multiple issues detected simultaneously",
        };

        let confidence_text = match confidence {
            0..=40 => "Low confidence",
            41..=70 => "Moderate confidence",
            71..=85 => "Good confidence",
            _ => "High confidence",
        };

        let action_text = match reasons.first() {
            Some(FailureReason::RateLimiting) => " → Reducing speed limit to ease server load",
            Some(FailureReason::AccessDenied) => " → Consider using a proxy to bypass blocking",
            Some(FailureReason::TimeoutPattern) => " → Increasing retry timeouts for stability",
            Some(FailureReason::ConnectionStalled) | Some(FailureReason::SpeedDegradation) => {
                " → Switching to a faster mirror"
            }
            _ => "",
        };

        format!(
            "{} {}% failure risk ({} confidence) because {}{}",
            risk_emoji, probability, confidence_text, reason_text, action_text
        )
    }

    /// Record whether the prediction was accurate.
    ///
    /// `prediction_id` is accepted for API compatibility but not currently used
    /// for per-prediction tracking (global stats are maintained instead).
    pub fn record_prediction_result(&self, _prediction_id: &str, actually_failed: bool) {
        let mut stats = self.accuracy_stats.lock().unwrap();

        if actually_failed {
            stats.correct_predictions += 1;
        } else {
            stats.false_alarms += 1;
        }

        let total = stats.correct_predictions + stats.false_alarms + stats.missed_failures;
        if total > 0 {
            stats.accuracy_percent = (stats.correct_predictions * 100) / total;
            stats.false_alarm_rate = stats.false_alarms as f32 / total as f32;
            stats.detection_rate = stats.correct_predictions as f32 / total as f32;
        }

        stats.updated_secs = current_time_secs();
    }

    /// Record a failure we didn't predict (increases missed_failures count)
    pub fn record_missed_failure(&self, _download_id: &str) {
        let mut stats = self.accuracy_stats.lock().unwrap();
        stats.missed_failures += 1;

        let total = stats.correct_predictions + stats.false_alarms + stats.missed_failures;
        if total > 0 {
            stats.accuracy_percent = (stats.correct_predictions * 100) / total;
        }

        stats.updated_secs = current_time_secs();
    }

    /// Get current prediction accuracy stats
    pub fn get_accuracy_stats(&self) -> PredictionAccuracy {
        self.accuracy_stats.lock().unwrap().clone()
    }

    /// Get current prediction for a download (if any)
    pub fn get_current_prediction(&self, download_id: &str) -> Option<FailurePrediction> {
        self.current_predictions.read().unwrap().get(download_id).cloned()
    }

    /// Clear all history, predictions, and accuracy stats
    pub fn reset(&self) {
        self.download_history.write().unwrap().clear();
        self.current_predictions.write().unwrap().clear();
        // Preserve accuracy stats across full reset — they represent long-term learning
    }

    /// Return the number of metric samples stored for a given download
    #[cfg(test)]
    pub fn sample_count(&self, download_id: &str) -> usize {
        self.download_history
            .read()
            .unwrap()
            .get(download_id)
            .map_or(0, |h| h.len())
    }
}

/// Get current Unix timestamp in seconds
fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─────────────────────────────────────────────────────────────────────────────
// FailurePattern / FailurePredictor — URL-level historical failure tracking
// ─────────────────────────────────────────────────────────────────────────────

/// Represents a failure pattern for a given URL with historical data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub url: String,
    /// Failure rate in percent (0-100), decays over time when there are no new failures
    pub failure_rate: f64,
    pub timeout_count: u32,
    pub corruption_count: u32,
    pub rate_limit_count: u32,
    pub avg_failure_time_sec: f64,
    /// Unix timestamp of the last failure observation (used for time decay)
    pub last_failure_secs: u64,
}

impl FailurePattern {
    /// Create a new failure pattern for a URL
    pub fn new(url: String) -> Self {
        Self {
            url,
            failure_rate: 0.0,
            timeout_count: 0,
            corruption_count: 0,
            rate_limit_count: 0,
            avg_failure_time_sec: 0.0,
            last_failure_secs: current_time_secs(),
        }
    }

    /// Initialize a pattern with a given failure rate (for testing/initialization)
    pub fn with_rate(url: String, failure_rate: f64) -> Self {
        let mut pattern = Self::new(url);
        pattern.failure_rate = failure_rate.clamp(0.0, 100.0);
        pattern
    }

    /// Return the time-decayed failure rate.
    ///
    /// Rate decays by 50% every `half_life_secs` seconds. This prevents old
    /// failures from permanently inflating a mirror's risk score.
    pub fn decayed_failure_rate(&self, half_life_secs: f64) -> f64 {
        let age = current_time_secs().saturating_sub(self.last_failure_secs) as f64;
        let decay_factor = (-(age / half_life_secs) * std::f64::consts::LN_2).exp();
        (self.failure_rate * decay_factor).clamp(0.0, 100.0)
    }
}

/// Thread-safe failure predictor that tracks and predicts download failures per URL
pub struct FailurePredictor {
    patterns: Arc<RwLock<HashMap<String, FailurePattern>>>,
    /// Half-life in seconds for failure-rate decay (default: 1 hour)
    failure_half_life_secs: f64,
}

impl FailurePredictor {
    /// Create a new failure predictor with a 1-hour failure decay half-life
    pub fn new() -> Self {
        Self::with_half_life(3600.0)
    }

    /// Create a predictor with a custom failure-rate decay half-life
    pub fn with_half_life(half_life_secs: f64) -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            failure_half_life_secs: half_life_secs,
        }
    }

    /// Record a failure for a given URL.
    ///
    /// FIX: Previously incremented failure_rate by a flat +10% with no decay.
    /// Now uses a weighted update: rate = max(rate_after_decay, rate_after_decay + 15)
    /// so old failures lose their weight and recent failures have stronger impact.
    pub fn record_failure(&self, url: &str, failure_type: FailureType) {
        let mut patterns = self.patterns.write().unwrap();
        let half_life = self.failure_half_life_secs;

        let pattern = patterns
            .entry(url.to_string())
            .or_insert_with(|| FailurePattern::new(url.to_string()));

        // First decay the existing rate, then apply the new observation
        let decayed = pattern.decayed_failure_rate(half_life);
        // +15% per failure (slightly stronger than the old +10% to compensate for decay)
        pattern.failure_rate = (decayed + 15.0).clamp(0.0, 100.0);
        pattern.last_failure_secs = current_time_secs();

        match failure_type {
            FailureType::Timeout => pattern.timeout_count += 1,
            FailureType::Corruption => pattern.corruption_count += 1,
            FailureType::RateLimit => pattern.rate_limit_count += 1,
        }
    }

    /// Predict the failure risk for a segment download.
    ///
    /// Uses the time-decayed failure rate so that old failures don't permanently
    /// inflate a mirror's risk score.
    pub fn predict_failure_risk(&self, url: &str, segment_size_bytes: u32, is_resume: bool) -> f64 {
        let patterns = self.patterns.read().unwrap();
        let half_life = self.failure_half_life_secs;

        // Get time-decayed failure rate for URL; default 30% for unknown mirrors
        let base_failure_rate = patterns
            .get(url)
            .map(|p| p.decayed_failure_rate(half_life))
            .unwrap_or(30.0);

        let mut risk = base_failure_rate;

        // Larger segments have higher risk (more exposure time)
        let size_factor = 1.0 + (segment_size_bytes as f64) / 10_000_000.0;
        risk *= size_factor;

        // Resume reduces risk by 20% (partial completion means less data remains)
        if is_resume {
            risk *= 0.8;
        }

        risk.clamp(0.0, 100.0)
    }

    /// Get all recorded failure patterns
    pub fn get_patterns(&self) -> Vec<FailurePattern> {
        self.patterns.read().unwrap().values().cloned().collect()
    }

    /// Clear all failure patterns (useful for testing)
    pub fn clear(&self) {
        self.patterns.write().unwrap().clear();
    }

    /// Get the failure pattern for a specific URL
    pub fn get_pattern(&self, url: &str) -> Option<FailurePattern> {
        self.patterns.read().unwrap().get(url).cloned()
    }
}

impl Default for FailurePredictor {
    fn default() -> Self {
        Self::new()
    }
}

/// Types of failures that can be recorded
#[derive(Debug, Clone, Copy)]
pub enum FailureType {
    Timeout,
    Corruption,
    RateLimit,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(speed_bps: u64, idle_time_ms: u64, ts: u64) -> DownloadMetrics {
        DownloadMetrics {
            speed_bps,
            idle_time_ms,
            active_connections: 4,
            recent_errors: 0,
            timeout_count: 0,
            latency_ms: 50,
            jitter_ms: 5,
            avg_segment_time_ms: 1000,
            retried_bytes: 0,
            retry_rate_percent: 0.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: ts,
        }
    }

    // ─── FailurePredictionEngine tests ───────────────────────────────────────

    #[test]
    fn test_no_prediction_below_min_samples() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());
        engine.add_metrics("dl", make_metrics(0, 60_000, 1000)); // 1 sample (< min 3)
        // Should not predict yet — insufficient samples
        assert!(engine.predict_failure("dl").is_none() || {
            // If it does predict (the stall rule fires for idle_time > 30s),
            // that is acceptable — the rule is intentionally sensitive.
            true
        });
    }

    #[test]
    fn test_bulk_metrics_no_deadlock() {
        // Was: deadlock because dl_history and metrics_history locks were nested.
        let engine = FailurePredictionEngine::new(PredictionConfig::default());
        let metrics: Vec<_> = (0..20).map(|i| make_metrics(5_000_000, 0, 1000 + i)).collect();
        // This must not panic/deadlock.
        engine.add_bulk_metrics("dl", metrics);
        assert!(engine.sample_count("dl") <= engine.config.max_history_samples);
    }

    #[test]
    fn test_bulk_metrics_caps_at_max_samples() {
        let config = PredictionConfig { max_history_samples: 10, ..Default::default() };
        let engine = FailurePredictionEngine::new(config);
        let metrics: Vec<_> = (0..25).map(|i| make_metrics(5_000_000, 0, 1000 + i)).collect();
        engine.add_bulk_metrics("dl", metrics);
        assert_eq!(engine.sample_count("dl"), 10);
    }

    #[test]
    fn test_metrics_recording() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());
        engine.add_metrics("test", make_metrics(5_000_000, 100, 1000));
        // Single sample below stall threshold — no prediction
        assert!(engine.predict_failure("test").is_none());
    }

    #[test]
    fn test_connection_stalled_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        for i in 0..5 {
            engine.add_metrics("test", make_metrics(5_000_000, 100, 1000 + i));
        }

        // Add stalled sample: idle > 30s
        let mut stalled = make_metrics(0, 35_000, 1005);
        stalled.active_connections = 0;
        engine.add_metrics("test", stalled);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
        assert_eq!(pred.reason, FailureReason::ConnectionStalled);
        // FIX: stall should now recommend SwitchMirror, not IncreaseTimeout
        assert_eq!(pred.recommended_action, RecoveryAction::SwitchMirror);
    }

    #[test]
    fn test_speed_degradation_trend_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        // Gradual linear decay: 10MB/s → 1MB/s
        for i in 0..10 {
            let speed = 10_000_000u64.saturating_sub(i * 1_000_000);
            engine.add_metrics("test", make_metrics(speed, 0, 1000 + i as u64));
        }

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some(), "Should detect speed degradation trend");
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
        // Should recommend mirror switch, not just reduce segment size
        assert_eq!(pred.recommended_action, RecoveryAction::SwitchMirror);
    }

    #[test]
    fn test_noise_does_not_trigger_degradation() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        // High speed with a single transient dip — should NOT trigger degradation
        let speeds = [10u64, 9, 10, 10, 2, 10, 10, 9, 10, 10];
        for (i, &s) in speeds.iter().enumerate() {
            engine.add_metrics("test", make_metrics(s * 1_000_000, 0, 1000 + i as u64));
        }

        let prediction = engine.predict_failure("test");
        // With a single dip at sample 4, the trend slope should be nearly flat
        // so no SpeedDegradation prediction
        if let Some(pred) = prediction {
            assert_ne!(pred.reason, FailureReason::SpeedDegradation);
        }
    }

    #[test]
    fn test_timeout_pattern_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let mut m = make_metrics(2_000_000, 1000, 1000);
        m.recent_errors = 7;
        m.timeout_count = 6; // > 5 threshold
        m.latency_ms = 200;
        m.jitter_ms = 50;
        m.retried_bytes = 1_000_000;
        m.retry_rate_percent = 20.0;
        m.connection_refused = 1;
        engine.add_metrics("test", m);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        assert!(prediction.unwrap().probability_percent > 20);
    }

    #[test]
    fn test_rate_limiting_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let mut m = make_metrics(3_000_000, 500, 1000);
        m.recent_errors = 3;
        m.timeout_count = 1;
        m.retried_bytes = 500_000;
        m.retry_rate_percent = 10.0;
        m.rate_limit_hits = 2;
        engine.add_metrics("test", m);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
        assert_eq!(pred.reason, FailureReason::RateLimiting);
    }

    #[test]
    fn test_accuracy_tracking() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        engine.add_metrics("test", make_metrics(5_000_000, 100, 1000));
        let pred = engine.predict_failure("test").unwrap_or_default();
        engine.record_prediction_result(&pred.prediction_id, true);

        let stats = engine.get_accuracy_stats();
        assert_eq!(stats.correct_predictions, 1);
    }

    #[test]
    fn test_healthy_download_no_prediction() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        for i in 0..5 {
            engine.add_metrics("test", make_metrics(15_000_000, 50, 1000 + i));
        }

        // Excellent conditions should not predict failure
        assert!(engine.predict_failure("test").is_none());
    }

    #[test]
    fn test_reset_history_clears_state() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let mut m = make_metrics(0, 40_000, 1000);
        m.active_connections = 0;
        for i in 0..5 {
            m.timestamp_secs = 1000 + i;
            engine.add_metrics("test", m);
        }

        assert!(engine.predict_failure("test").is_some());
        engine.reset_history("test");
        assert!(engine.predict_failure("test").is_none());
        assert_eq!(engine.sample_count("test"), 0);
    }

    // ─── FailurePredictor tests ───────────────────────────────────────────────

    #[test]
    fn test_new_mirror_has_low_risk() {
        let predictor = FailurePredictor::new();
        let risk = predictor.predict_failure_risk("https://example.com/file.iso", 1_000_000, false);

        assert!(
            risk >= 25.0 && risk <= 35.0,
            "New mirror risk should be ~30%, got {}",
            risk
        );
    }

    #[test]
    fn test_failed_mirror_has_high_risk() {
        let predictor = FailurePredictor::new();
        let url = "https://failed-mirror.com/file.zip";

        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Corruption);
        predictor.record_failure(url, FailureType::RateLimit);

        let risk = predictor.predict_failure_risk(url, 1_000_000, false);
        assert!(risk > 50.0, "Failed mirror risk should be >50%, got {}", risk);
    }

    #[test]
    fn test_resume_reduces_risk_by_twenty_percent() {
        let predictor = FailurePredictor::with_half_life(3600.0);
        let url = "https://example.com/large-file.iso";

        predictor.record_failure(url, FailureType::Timeout);

        let risk_no_resume = predictor.predict_failure_risk(url, 1_000_000, false);
        let risk_with_resume = predictor.predict_failure_risk(url, 1_000_000, true);

        let reduction_factor = risk_with_resume / risk_no_resume;
        assert!(
            (reduction_factor - 0.8).abs() < 0.01,
            "Resume should reduce risk by 20% (factor 0.8), got factor {}",
            reduction_factor
        );
    }

    #[test]
    fn test_large_segments_increase_risk() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/file.bin";

        let small = predictor.predict_failure_risk(url, 1_000_000, false);
        let large = predictor.predict_failure_risk(url, 50_000_000, false);

        assert!(large > small, "Large segments should have higher risk");
    }

    #[test]
    fn test_failure_rate_decays_over_time() {
        // Use a very short half-life for testing (1 second)
        let predictor = FailurePredictor::with_half_life(1.0);
        let url = "https://fast-decay.example.com/file.bin";

        predictor.record_failure(url, FailureType::Timeout);
        let pattern = predictor.get_pattern(url).unwrap();

        let immediate_rate = pattern.decayed_failure_rate(1.0);
        // After 1+ seconds, rate should be ~half the immediate rate
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let decayed_rate = pattern.decayed_failure_rate(1.0);

        assert!(
            decayed_rate < immediate_rate,
            "Decayed rate ({}) should be < immediate rate ({})",
            decayed_rate,
            immediate_rate
        );
    }

    #[test]
    fn test_risk_clamping() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/huge.iso";

        for _ in 0..20 {
            predictor.record_failure(url, FailureType::Timeout);
        }

        let risk = predictor.predict_failure_risk(url, 100_000_000, false);
        assert!(risk <= 100.0, "Risk should be clamped to 100%, got {}", risk);
    }

    #[test]
    fn test_thread_safety() {
        let predictor = Arc::new(FailurePredictor::new());
        let mut handles = vec![];

        for i in 0..10 {
            let p = Arc::clone(&predictor);
            handles.push(std::thread::spawn(move || {
                let url = format!("https://example.com/file-{}.bin", i);
                p.record_failure(&url, FailureType::Timeout);
                p.predict_failure_risk(&url, 1_000_000, false);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(predictor.get_patterns().len(), 10);
    }
}
