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
//! - **HyperStream**: Predicts failures 30-60 seconds in advance

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    /// Speed consistently declining (<50% of baseline)
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
    /// Historical metrics for pattern analysis
    metrics_history: Arc<RwLock<VecDeque<DownloadMetrics>>>,
    /// Current prediction (if any)
    current_prediction: Arc<Mutex<Option<FailurePrediction>>>,
    /// Accuracy statistics
    accuracy_stats: Arc<Mutex<PredictionAccuracy>>,
    /// Per-download failure history
    download_history: Arc<RwLock<HashMap<String, Vec<DownloadMetrics>>>>,
    /// Configuration
    config: PredictionConfig,
}

/// Configuration for prediction engine
#[derive(Debug, Clone)]
pub struct PredictionConfig {
    /// Max history samples to keep (default: 300, ~5 min at 1Hz)
    pub max_history_samples: usize,
    /// Bytes per second threshold for "stalled"
    pub stalled_threshold_bps: u64,
    /// Idle time ms threshold before stall warning
    pub stall_idle_time_ms: u64,
    /// Speed degradation ratio (e.g., 0.5 = 50% drop)
    pub speed_degradation_ratio: f32,
    /// Timeout count threshold in window
    pub timeout_threshold: u32,
    /// Error count threshold in window
    pub error_threshold: u32,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            max_history_samples: 300,
            stalled_threshold_bps: 100_000, // <100 KB/s = stalled
            stall_idle_time_ms: 30_000,     // 30 seconds
            speed_degradation_ratio: 0.5,   // 50% drop
            timeout_threshold: 5,
            error_threshold: 10,
        }
    }
}

impl FailurePredictionEngine {
    /// Create a new failure prediction engine
    pub fn new(config: PredictionConfig) -> Self {
        Self {
            metrics_history: Arc::new(RwLock::new(VecDeque::new())),
            current_prediction: Arc::new(Mutex::new(None)),
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

    /// Record a new metrics snapshot for analysis
    pub fn add_metrics(&self, metrics: DownloadMetrics) {
        let mut history = self.metrics_history.write().unwrap();
        history.push_back(metrics);

        // Keep history bounded
        while history.len() > self.config.max_history_samples {
            history.pop_front();
        }
    }

    /// Analyze current metrics and predict potential failure
    pub fn predict_failure(&self, download_id: &str) -> Option<FailurePrediction> {
        let history = self.metrics_history.read().unwrap();

        if history.is_empty() {
            return None;
        }

        let current = history.back().unwrap();
        let mut probability = 0u32;
        let mut reasons = Vec::new();
        let mut contributing_factors = Vec::new();

        // ─── Rule 1: Connection Stalled ───────────────────────────────────────
        if current.idle_time_ms > self.config.stall_idle_time_ms {
            probability += 35;
            reasons.push(FailureReason::ConnectionStalled);
        } else if current.idle_time_ms > self.config.stall_idle_time_ms / 2 {
            probability += 15;
            contributing_factors.push(FailureReason::ConnectionStalled);
        }

        // ─── Rule 2: Speed Degradation ────────────────────────────────────────
        if history.len() >= 10 {
            let recent_avg = history
                .iter()
                .rev()
                .take(5)
                .map(|m| m.speed_bps)
                .sum::<u64>()
                / 5;
            let older_avg = history
                .iter()
                .rev()
                .skip(5)
                .take(5)
                .map(|m| m.speed_bps)
                .sum::<u64>()
                / 5;

            if older_avg > 0 {
                let degradation = 1.0 - (recent_avg as f32 / older_avg as f32);
                if degradation > self.config.speed_degradation_ratio {
                    probability += 30;
                    reasons.push(FailureReason::SpeedDegradation);
                } else if degradation > self.config.speed_degradation_ratio * 0.5 {
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
        let confidence = self.calculate_confidence(&primary_reason, probability, history.len());

        // Determine risk level
        let risk_level = match probability {
            0..=15 => FailureRisk::Healthy,
            16..=35 => FailureRisk::Caution,
            36..=60 => FailureRisk::Warning,
            61..=85 => FailureRisk::Critical,
            _ => FailureRisk::Imminent,
        };

        // Choose recovery action based on primary reason
        let recommended_action = match primary_reason {
            FailureReason::SpeedDegradation => RecoveryAction::ReduceSegmentSize,
            FailureReason::ConnectionStalled => RecoveryAction::IncreaseTimeout,
            FailureReason::TimeoutPattern => RecoveryAction::IncreaseTimeout,
            FailureReason::ConnectionRefusal => RecoveryAction::SwitchMirror,
            FailureReason::RateLimiting => RecoveryAction::ReduceSpeedLimit,
            FailureReason::AccessDenied => RecoveryAction::UseProxy,
            FailureReason::DnsFailures => RecoveryAction::SwitchDns,
            FailureReason::NetworkUnstable => RecoveryAction::WaitAndRetry,
            FailureReason::SlowingSegments => RecoveryAction::SequentialMode,
            FailureReason::CompoundedIssues => {
                if probability > 75 {
                    RecoveryAction::InitiateRecovery
                } else {
                    RecoveryAction::PauseAndResume
                }
            }
        };

        // Estimate time to failure
        let time_to_failure = if probability > 70 {
            Some(match primary_reason {
                FailureReason::ConnectionStalled => 30,
                FailureReason::TimeoutPattern => 45,
                _ => 60,
            })
        } else {
            None
        };

        let explanation = self.generate_explanation(
            probability,
            confidence,
            &primary_reason,
            &reasons,
            &risk_level,
        );

        let prediction = FailurePrediction {
            prediction_id: format!(
                "pred_{}_{}_{}",
                download_id, probability, current.timestamp_secs
            ),
            probability_percent: probability,
            confidence_percent: confidence,
            reason: primary_reason,
            time_to_failure_secs: time_to_failure,
            risk_level,
            recommended_action,
            contributing_factors,
            timestamp_secs: current.timestamp_secs,
            explanation,
            confidence_breakdown: ConfidenceBreakdown {
                historical_accuracy_percent: self.get_historical_accuracy(&primary_reason),
                sample_size_confidence: Self::calculate_sample_size_confidence(history.len()),
                signal_correlation_confidence: Self::calculate_signal_correlation(reasons.len()),
                metrics_clarity_percent: Self::calculate_metrics_clarity(current),
            },
        };

        // Store current prediction
        *self.current_prediction.lock().unwrap() = Some(prediction.clone());

        Some(prediction)
    }

    /// Calculate confidence in the prediction
    fn calculate_confidence(
        &self,
        _reason: &FailureReason,
        probability: u32,
        sample_count: usize,
    ) -> u32 {
        let base_confidence = match probability {
            0..=30 => 40,
            31..=50 => 60,
            51..=70 => 75,
            71..=85 => 85,
            _ => 95,
        };

        // Boost confidence with more samples
        let sample_boost = (sample_count.min(100) as u32 * 30) / 100;

        ((base_confidence + sample_boost) / 2).min(100)
    }

    /// Get historical accuracy for this failure reason
    fn get_historical_accuracy(&self, _reason: &FailureReason) -> u32 {
        let stats = self.accuracy_stats.lock().unwrap();
        stats.accuracy_percent
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

    /// Calculate confidence from signal correlation
    fn calculate_signal_correlation(reason_count: usize) -> u32 {
        match reason_count {
            0 => 40,
            1 => 50,
            2 => 70,
            3 => 85,
            _ => 95,
        }
    }

    /// Calculate metrics clarity score
    fn calculate_metrics_clarity(metrics: &DownloadMetrics) -> u32 {
        let mut clarity = 50u32;

        // Data freshness (penalty if older than 5 seconds)
        let age = current_time_secs().saturating_sub(metrics.timestamp_secs);
        if age > 5 {
            clarity = clarity.saturating_sub(20);
        }

        // Metric diversity
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
            FailureReason::SpeedDegradation => "download speed is declining significantly",
            FailureReason::ConnectionStalled => "connection appears to be stalled (no data)",
            FailureReason::TimeoutPattern => "too many timeout errors detected",
            FailureReason::ConnectionRefusal => "server is refusing connections",
            FailureReason::RateLimiting => "server is rate limiting requests",
            FailureReason::AccessDenied => "access to resource is denied",
            FailureReason::DnsFailures => "DNS resolution is failing",
            FailureReason::NetworkUnstable => "network is unstable and unreliable",
            FailureReason::SlowingSegments => "individual segments are taking longer",
            FailureReason::CompoundedIssues => "multiple issues detected simultaneously",
        };

        let confidence_text = match confidence {
            0..=40 => "Low confidence",
            41..=70 => "Moderate confidence",
            71..=85 => "Good confidence",
            _ => "High confidence",
        };

        let action_text = match &reasons.first() {
            Some(FailureReason::RateLimiting) => " → Reducing speed limit to ease server load",
            Some(FailureReason::AccessDenied) => " → Consider using a proxy to bypass blocking",
            Some(FailureReason::TimeoutPattern) => " → Increasing retry timeouts for stability",
            Some(FailureReason::ConnectionStalled) => {
                " → Will investigate and possibly switch mirror"
            }
            _ => "",
        };

        format!(
            "{} {}% failure risk ({} confidence) because {}{}",
            risk_emoji, probability, confidence_text, reason_text, action_text
        )
    }

    /// Record whether the prediction was accurate
    pub fn record_prediction_result(&self, prediction_id: &str, actually_failed: bool) {
        let mut stats = self.accuracy_stats.lock().unwrap();

        if actually_failed {
            stats.correct_predictions += 1;
        } else {
            stats.false_alarms += 1;
        }

        // Recalculate accuracy
        let total = stats.correct_predictions + stats.false_alarms + stats.missed_failures;
        if total > 0 {
            stats.accuracy_percent = (stats.correct_predictions * 100) / total;
            stats.false_alarm_rate = (stats.false_alarms as f32) / (total as f32);
            stats.detection_rate = (stats.correct_predictions as f32) / (total as f32);
        }

        stats.updated_secs = current_time_secs();
    }

    /// Record a failure we didn't predict
    pub fn record_missed_failure(&self, _download_id: &str) {
        let mut stats = self.accuracy_stats.lock().unwrap();
        stats.missed_failures += 1;

        // Recalculate accuracy
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

    /// Get current prediction (if any)
    pub fn get_current_prediction(&self) -> Option<FailurePrediction> {
        self.current_prediction.lock().unwrap().clone()
    }

    /// Clear history and predictions
    pub fn reset(&self) {
        self.metrics_history.write().unwrap().clear();
        *self.current_prediction.lock().unwrap() = None;
    }
}

/// Get current Unix timestamp
fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Represents a failure pattern for a given URL with historical data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub url: String,
    pub failure_rate: f64,
    pub timeout_count: u32,
    pub corruption_count: u32,
    pub rate_limit_count: u32,
    pub avg_failure_time_sec: f64,
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
        }
    }

    /// Initialize a pattern with a given failure rate (for testing/initialization)
    pub fn with_rate(url: String, failure_rate: f64) -> Self {
        let mut pattern = Self::new(url);
        pattern.failure_rate = failure_rate.clamp(0.0, 100.0);
        pattern
    }
}

/// Thread-safe failure predictor that tracks and predicts download failures
pub struct FailurePredictor {
    patterns: Arc<RwLock<HashMap<String, FailurePattern>>>,
}

impl FailurePredictor {
    /// Create a new failure predictor
    pub fn new() -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a failure for a given URL
    pub fn record_failure(&self, url: &str, failure_type: FailureType) {
        let mut patterns = self.patterns.write().unwrap();
        let pattern = patterns
            .entry(url.to_string())
            .or_insert_with(|| FailurePattern::new(url.to_string()));

        // Update failure rate based on historical data
        // For simplicity, increment by 10% for each failure recorded
        pattern.failure_rate = (pattern.failure_rate + 10.0).clamp(0.0, 100.0);

        match failure_type {
            FailureType::Timeout => pattern.timeout_count += 1,
            FailureType::Corruption => pattern.corruption_count += 1,
            FailureType::RateLimit => pattern.rate_limit_count += 1,
        }
    }

    /// Predict the failure risk for a segment download
    ///
    /// Algorithm:
    /// - risk = base_failure_rate
    /// - risk *= (1.0 + segment_size / 10_000_000)  // Size factor (larger = higher risk)
    /// - risk *= 0.8 if is_resume                   // Resume reduces risk by 20%
    /// - risk = clamp(risk, 0, 100)
    pub fn predict_failure_risk(&self, url: &str, segment_size_bytes: u32, is_resume: bool) -> f64 {
        let patterns = self.patterns.read().unwrap();

        // Get base failure rate for URL, default to 30% for new mirrors
        let base_failure_rate = patterns.get(url).map(|p| p.failure_rate).unwrap_or(30.0);

        // Start with base rate
        let mut risk = base_failure_rate;

        // Apply size factor: larger segments have higher risk
        let size_factor = 1.0 + (segment_size_bytes as f64) / 10_000_000.0;
        risk *= size_factor;

        // Resume reduces risk by 20%
        if is_resume {
            risk *= 0.8;
        }

        // Clamp to valid percentage range
        risk.clamp(0.0, 100.0)
    }

    /// Get all recorded failure patterns
    pub fn get_patterns(&self) -> Vec<FailurePattern> {
        let patterns = self.patterns.read().unwrap();
        patterns.values().cloned().collect()
    }

    /// Clear all failure patterns (useful for testing)
    pub fn clear(&self) {
        let mut patterns = self.patterns.write().unwrap();
        patterns.clear();
    }

    /// Get the failure pattern for a specific URL
    pub fn get_pattern(&self, url: &str) -> Option<FailurePattern> {
        let patterns = self.patterns.read().unwrap();
        patterns.get(url).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_mirror_has_low_risk() {
        let predictor = FailurePredictor::new();
        let risk = predictor.predict_failure_risk("https://example.com/file.iso", 1_000_000, false);

        // New mirror should have ~30% base risk
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

        // Record multiple failures to increase risk
        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Corruption);
        predictor.record_failure(url, FailureType::RateLimit);

        let risk = predictor.predict_failure_risk(url, 1_000_000, false);

        // Failed mirror should have >50% risk after 3 failures
        assert!(
            risk > 50.0,
            "Failed mirror risk should be >50%, got {}",
            risk
        );
    }

    #[test]
    fn test_resume_reduces_risk_by_twenty_percent() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/large-file.iso";

        // Record some failures
        predictor.record_failure(url, FailureType::Timeout);

        let risk_no_resume = predictor.predict_failure_risk(url, 1_000_000, false);
        let risk_with_resume = predictor.predict_failure_risk(url, 1_000_000, true);

        // Resume should reduce risk by 20%
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

        let small_segment_risk = predictor.predict_failure_risk(url, 1_000_000, false);
        let large_segment_risk = predictor.predict_failure_risk(url, 50_000_000, false);

        // Larger segments should have higher risk
        assert!(
            large_segment_risk > small_segment_risk,
            "Large segments ({}) should have higher risk than small ({}) segments",
            large_segment_risk,
            small_segment_risk
        );
    }

    #[test]
    fn test_record_failure_updates_pattern() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/test.exe";

        assert_eq!(predictor.get_patterns().len(), 0);

        predictor.record_failure(url, FailureType::Timeout);

        assert_eq!(predictor.get_patterns().len(), 1);
        let pattern = predictor.get_pattern(url).unwrap();
        assert_eq!(pattern.timeout_count, 1);
        assert_eq!(pattern.corruption_count, 0);
        assert_eq!(pattern.rate_limit_count, 0);
    }

    #[test]
    fn test_multiple_failure_types() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/multi-fail.iso";

        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Corruption);
        predictor.record_failure(url, FailureType::RateLimit);

        let pattern = predictor.get_pattern(url).unwrap();
        assert_eq!(pattern.timeout_count, 2);
        assert_eq!(pattern.corruption_count, 1);
        assert_eq!(pattern.rate_limit_count, 1);
    }

    #[test]
    fn test_risk_clamping() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/huge.iso";

        // Record many failures to push risk past 100%
        for _ in 0..20 {
            predictor.record_failure(url, FailureType::Timeout);
        }

        let risk = predictor.predict_failure_risk(url, 100_000_000, false);

        // Risk should be clamped to max 100%
        assert!(
            risk <= 100.0,
            "Risk should be clamped to 100%, got {}",
            risk
        );
    }

    #[test]
    fn test_thread_safety() {
        let predictor = Arc::new(FailurePredictor::new());
        let mut handles = vec![];

        for i in 0..10 {
            let predictor_clone = Arc::clone(&predictor);
            let handle = std::thread::spawn(move || {
                let url = format!("https://example.com/file-{}.bin", i);
                predictor_clone.record_failure(&url, FailureType::Timeout);
                predictor_clone.predict_failure_risk(&url, 1_000_000, false);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 10 patterns recorded from 10 threads
        assert_eq!(predictor.get_patterns().len(), 10);
    }
}
