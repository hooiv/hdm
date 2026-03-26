/**
 * Failure Prediction API
 *
 * Frontend wrapper for the Failure Prediction & Proactive Recovery System.
 * Predicts download failures before they happen, enabling proactive recovery.
 *
 * This is a major competitive advantage—predicting failures 30-60 seconds in advance
 * so the app can automatically take preventive action before the user even notices.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Real-time download metrics for failure analysis
 */
export interface DownloadMetrics {
  /** Current bytes per second */
  speed_bps: u64,
  /** Milliseconds since last byte received */
  idle_time_ms: u64,
  /** Number of active connections */
  active_connections: u32,
  /** Errors in last 10 seconds */
  recent_errors: u32,
  /** Connection timeouts in session */
  timeout_count: u32,
  /** Avg latency in milliseconds */
  latency_ms: u64,
  /** Jitter/variance in latency */
  jitter_ms: u32,
  /** Segment completion time ms (moving average) */
  avg_segment_time_ms: u64,
  /** Bytes retried due to errors */
  retried_bytes: u64,
  /** Percent of segments requiring retry */
  retry_rate_percent: f32,
  /** DNS failures in session */
  dns_failures: u32,
  /** HTTP 429 (rate limit) responses */
  rate_limit_hits: u32,
  /** HTTP 403 (forbidden) responses */
  access_denied_hits: u32,
  /** Connection refused errors */
  connection_refused: u32,
}

/**
 * Failure risk assessment
 */
export type FailureRisk = 
  | "Healthy"      // 0-15%
  | "Caution"      // 15-35%
  | "Warning"      // 35-60%
  | "Critical"     // 60-85%
  | "Imminent";    // 85%+

/**
 * Reason for failure prediction
 */
export type FailureReason =
  | "SpeedDegradation"
  | "ConnectionStalled"
  | "TimeoutPattern"
  | "ConnectionRefusal"
  | "RateLimiting"
  | "AccessDenied"
  | "DnsFailures"
  | "NetworkUnstable"
  | "SlowingSegments"
  | "CompoundedIssues";

/**
 * Recommended recovery action
 */
export type RecoveryAction =
  | "Monitor"
  | "ReduceSegmentSize"
  | "SequentialMode"
  | "SwitchMirror"
  | "ReduceSpeedLimit"
  | "WaitAndRetry"
  | "UseProxy"
  | "SwitchDns"
  | "IncreaseTimeout"
  | "PauseAndResume"
  | "SwitchUrl"
  | "InitiateRecovery";

/**
 * Confidence score breakdown
 */
export interface ConfidenceBreakdown {
  /** Based on historical accuracy of this pattern */
  historical_accuracy_percent: u32,
  /** Based on sample size of similar situations */
  sample_size_confidence: u32,
  /** Based on multiple corroborating signals */
  signal_correlation_confidence: u32,
  /** Based on current metrics specificity */
  metrics_clarity_percent: u32,
}

/**
 * Prediction about an imminent failure
 */
export interface FailurePrediction {
  /** Unique prediction ID */
  prediction_id: string;
  /** Probability of failure (0-100%) */
  probability_percent: u32;
  /** Confidence in the prediction (0-100%) */
  confidence_percent: u32;
  /** Primary reason for prediction */
  reason: FailureReason;
  /** Predicted time until failure (seconds) */
  time_to_failure_secs?: u32;
  /** Risk level assessment */
  risk_level: FailureRisk;
  /** Recommended recovery action */
  recommended_action: RecoveryAction;
  /** Secondary factors contributing */
  contributing_factors: FailureReason[];
  /** Timestamp of prediction */
  timestamp_secs: u64;
  /** Human-readable explanation */
  explanation: string;
  /** Confidence score breakdown */
  confidence_breakdown: ConfidenceBreakdown;
}

/**
 * Historical prediction accuracy
 */
export interface PredictionAccuracy {
  /** Number of correct predictions */
  correct_predictions: u32;
  /** Number of false alarms */
  false_alarms: u32;
  /** Number of missed failures */
  missed_failures: u32;
  /** Accuracy percentage (0-100) */
  accuracy_percent: u32;
  /** False alarm rate */
  false_alarm_rate: f32;
  /** Detection rate (sensitivity) */
  detection_rate: f32;
  /** Last updated timestamp */
  updated_secs: u64;
}

/**
 * Record download metrics for failure analysis
 *
 * Call this continuously as the download progresses to feed the prediction engine
 * with real-time data. Call roughly every 1-2 seconds for best accuracy.
 *
 * @example
 * ```typescript
 * await recordDownloadMetrics("download123", {
 *   speed_bps: 5_000_000,
 *   idle_time_ms: 100,
 *   active_connections: 4,
 *   recent_errors: 0,
 *   timeout_count: 0,
 *   latency_ms: 50,
 *   jitter_ms: 5,
 *   avg_segment_time_ms: 1000,
 *   retried_bytes: 0,
 *   retry_rate_percent: 0.0,
 *   dns_failures: 0,
 *   rate_limit_hits: 0,
 *   access_denied_hits: 0,
 *   connection_refused: 0,
 * });
 * ```
 */
export async function recordDownloadMetrics(
  downloadId: string,
  speed_bps: u64,
  idle_time_ms: u64,
  active_connections: u32,
  recent_errors: u32,
  timeout_count: u32,
  latency_ms: u64,
  jitter_ms: u32,
  avg_segment_time_ms: u64,
  retried_bytes: u64,
  retry_rate_percent: f32,
  dns_failures: u32,
  rate_limit_hits: u32,
  access_denied_hits: u32,
  connection_refused: u32,
): Promise<string> {
  return invoke("record_download_metrics", {
    download_id: downloadId,
    speed_bps,
    idle_time_ms,
    active_connections,
    recent_errors,
    timeout_count,
    latency_ms,
    jitter_ms,
    avg_segment_time_ms,
    retried_bytes,
    retry_rate_percent,
    dns_failures,
    rate_limit_hits,
    access_denied_hits,
    connection_refused,
  });
}

/**
 * Analyze failure risk for a download
 *
 * Returns the current failure prediction (if any) for the download.
 * The prediction includes probability, confidence, recommended actions, and explanation.
 *
 * @example
 * ```typescript
 * const prediction = await analyzeFailureRisk("download123");
 * if (prediction) {
 *   console.log(`${prediction.probability_percent}% failure risk`);
 *   console.log(`Recommended: ${prediction.recommended_action}`);
 *   console.log(`${prediction.explanation}`);
 * }
 * ```
 */
export async function analyzeFailureRisk(
  downloadId: string,
): Promise<FailurePrediction | null> {
  const result = await invoke<{
    success: boolean;
    prediction?: FailurePrediction;
    error?: string;
  }>("analyze_failure_risk", {
    download_id: downloadId,
  });

  if (result.success && result.prediction) {
    return result.prediction;
  }

  return null;
}

/**
 * Record whether a prediction was accurate
 *
 * Call this after a download completes to help train the prediction engine.
 * The engine learns from its predictions to improve accuracy over time.
 *
 * @example
 * ```typescript
 * // Download completed successfully - was the failure prediction wrong?
 * await recordPredictionAccuracy("pred_xyz_70_1234567", false);
 *
 * // Download failed - was the prediction correct?
 * await recordPredictionAccuracy("pred_xyz_70_1234567", true);
 * ```
 */
export async function recordPredictionAccuracy(
  predictionId: string,
  actuallyFailed: boolean,
): Promise<string> {
  return invoke("record_prediction_accuracy", {
    prediction_id: predictionId,
    actually_failed: actuallyFailed,
  });
}

/**
 * Record a failure we didn't predict
 *
 * Call this if a download failed but we didn't predict it.
 * Helps the engine improve its detection accuracy.
 *
 * @example
 * ```typescript
 * if (downloadFailed && !prediction) {
 *   await recordMissedFailure("download123");
 * }
 * ```
 */
export async function recordMissedFailure(
  downloadId: string,
): Promise<string> {
  return invoke("record_missed_failure", {
    download_id: downloadId,
  });
}

/**
 * Get prediction accuracy statistics
 *
 * Returns metrics about how accurate the prediction engine's predictions have been.
 * Use this to understand the engine's performance characteristics.
 *
 * @example
 * ```typescript
 * const stats = await getPredictionAccuracyStats();
 * console.log(`${stats.accuracy_percent}% accurate`);
 * console.log(`${stats.detection_rate * 100}% detection rate`);
 * console.log(`${(stats.false_alarm_rate * 100).toFixed(1)}% false alarm rate`);
 * ```
 */
export async function getPredictionAccuracyStats(): Promise<PredictionAccuracy> {
  return invoke("get_prediction_accuracy_stats");
}

/**
 * Get current failure prediction
 *
 * Returns the most recent failure prediction (if any).
 * Useful for checking if there's an active prediction without calling analyze_failure_risk.
 *
 * @example
 * ```typescript
 * const current = await getCurrentFailurePrediction();
 * if (current && current.risk_level === "Critical") {
 *   showWarningToUser(current.explanation);
 * }
 * ```
 */
export async function getCurrentFailurePrediction(): Promise<FailurePrediction | null> {
  return invoke("get_current_failure_prediction");
}

/**
 * Reset failure prediction engine
 *
 * Clears all history and predictions. Useful for testing or starting a fresh session.
 *
 * @example
 * ```typescript
 * await resetFailurePrediction();
 * ```
 */
export async function resetFailurePrediction(): Promise<string> {
  return invoke("reset_failure_prediction");
}

/**
 * Listen for failure predictions
 *
 * The backend emits predictions for high-risk failures (>60% probability).
 * Use this to react to predictions in real-time without polling.
 *
 * @example
 * ```typescript
 * const unlisten = await listenForFailurePredictions((prediction) => {
 *   showCriticalWarning(`⚠️ ${prediction.explanation}`);
 *   executeRecoveryAction(prediction.recommended_action);
 * });
 * ```
 */
export async function listenForFailurePredictions(
  callback: (prediction: FailurePrediction) => void,
): Promise<() => void> {
  const unlisten = await listen<FailurePrediction>(
    "failure_prediction",
    (event) => {
      callback(event.payload);
    }
  );

  return unlisten;
}

/**
 * Format failure reason for display
 */
export function formatFailureReason(reason: FailureReason): string {
  const reasons: Record<FailureReason, string> = {
    SpeedDegradation: "Download speed is declining",
    ConnectionStalled: "Connection appears to be stalled",
    TimeoutPattern: "Excessive timeout errors",
    ConnectionRefusal: "Server refusing connections",
    RateLimiting: "Server rate limiting detected",
    AccessDenied: "Access to resource denied",
    DnsFailures: "DNS resolution failing",
    NetworkUnstable: "Network is unstable",
    SlowingSegments: "Individual segments slowing",
    CompoundedIssues: "Multiple issues detected",
  };
  return reasons[reason] || "Unknown issue";
}

/**
 * Format recovery action for display
 */
export function formatRecoveryAction(action: RecoveryAction): string {
  const actions: Record<RecoveryAction, string> = {
    Monitor: "Monitor for now",
    ReduceSegmentSize: "Reduce segment size",
    SequentialMode: "Switch to sequential mode",
    SwitchMirror: "Try alternative mirror",
    ReduceSpeedLimit: "Reduce speed limit",
    WaitAndRetry: "Wait and retry later",
    UseProxy: "Use proxy/VPN",
    SwitchDns: "Switch DNS resolver",
    IncreaseTimeout: "Increase timeout values",
    PauseAndResume: "Pause and resume later",
    SwitchUrl: "Try different URL",
    InitiateRecovery: "Initiate recovery mode",
  };
  return actions[action] || "Unknown action";
}

/**
 * Format risk level with emoji
 */
export function formatRiskLevel(risk: FailureRisk): string {
  const emojis: Record<FailureRisk, string> = {
    Healthy: "✅ Healthy",
    Caution: "⚠️ Caution",
    Warning: "🟡 Warning",
    Critical: "🔴 Critical",
    Imminent: "💥 Imminent",
  };
  return emojis[risk] || "Unknown";
}

/**
 * Get emoji for confidence level
 */
export function getConfidenceEmoji(confidence: u32): string {
  if (confidence >= 80) return "🎯";
  if (confidence >= 60) return "👍";
  if (confidence >= 40) return "🤔";
  return "❓";
}

/**
 * Format confidence as percentage with emoji
 */
export function formatConfidence(confidence: u32): string {
  return `${getConfidenceEmoji(confidence)} ${confidence}% confident`;
}

/**
 * Check if prediction indicates imminent failure
 */
export function isImminentFailure(prediction: FailurePrediction): boolean {
  return prediction.probability_percent > 75 || prediction.risk_level === "Imminent";
}

/**
 * Check if automatic action should be taken
 */
export function shouldTakeAutomaticAction(prediction: FailurePrediction): boolean {
  return isImminentFailure(prediction) ||
    prediction.risk_level === "Critical";
}

/**
 * Get color for risk level (for UI)
 */
export function getRiskColor(risk: FailureRisk): string {
  const colors: Record<FailureRisk, string> = {
    Healthy: "#22c55e",     // green
    Caution: "#eab308",     // yellow
    Warning: "#f97316",     // orange
    Critical: "#ef4444",    // red
    Imminent: "#7c3aed",    // violet
  };
  return colors[risk] || "#gray";
}

/**
 * Create an end-user-friendly message
 */
export function createUserMessage(prediction: FailurePrediction): string {
  if (prediction.probability_percent > 80) {
    return `⚠️ Your download is in danger! ${prediction.explanation}`;
  }
  if (prediction.probability_percent > 60) {
    return `🟡 Potential issue detected: ${prediction.explanation}`;
  }
  return `ℹ️ ${prediction.explanation}`;
}

/**
 * Summary of prediction for status badge
 */
export function getPredictionBadge(prediction: FailurePrediction): string {
  return `${prediction.probability_percent}% risk`;
}
