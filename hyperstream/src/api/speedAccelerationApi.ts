//! Speed Acceleration API
//!
//! TypeScript bindings for the download speed acceleration engine.
//! Provides bandwidth monitoring, condition detection, and optimization strategies.

import { invoke } from "@tauri-apps/api/core";

export interface AccelerationStats {
  avg_speed_bps: number;
  max_speed_bps: number;
  min_speed_bps: number;
  speed_variance: number;
  network_condition: string;
  health_score: number;
  predicted_improvement: boolean;
  predicted_degradation: boolean;
  measurements_count: number;
}

export interface DownloadTimeEstimate {
  file_size_bytes: number;
  estimated_time_secs: number;
  estimated_time_formatted: string;
  confidence_percent: number;
}

export interface BandwidthDataPoint {
  timestamp: number;
  speed_bps: number;
}

/**
 * Get current acceleration statistics with network condition.
 *
 * @returns Statistics including average speed, health score, and predictions
 */
export async function getAccelerationStats(): Promise<AccelerationStats> {
  return invoke("get_acceleration_stats");
}

/**
 * Record a bandwidth measurement.
 *
 * @param bytesTransferred - Bytes transferred in this measurement window
 * @param durationMs - Duration of the measurement in milliseconds
 * @param qualityScore - Quality score (0-100, higher = more stable)
 * @returns Confirmation message
 */
export async function recordBandwidthMeasurement(
  bytesTransferred: number,
  durationMs: number,
  qualityScore: number = 80
): Promise<string> {
  return invoke("record_bandwidth_measurement", {
    bytes_transferred: bytesTransferred,
    duration_ms: durationMs,
    quality_score: Math.min(Math.max(qualityScore, 0), 100),
  });
}

/**
 * Estimate download time for a file based on current bandwidth.
 *
 * @param fileSizeBytes - Size of the file to download
 * @returns Estimated download time with confidence level
 */
export async function estimateDownloadTime(
  fileSizeBytes: number
): Promise<DownloadTimeEstimate> {
  return invoke("estimate_download_time", { file_size_bytes: fileSizeBytes });
}

/**
 * Get the optimal segment strategy for current network conditions.
 *
 * @returns Strategy recommendations (segment size, parallel connections, etc.)
 */
export async function getOptimalSegmentStrategy(): Promise<string> {
  return invoke("get_optimal_segment_strategy");
}

/**
 * Predict future network changes.
 *
 * @returns Prediction of improvement, degradation, or stability
 */
export async function predictNetworkChanges(): Promise<string> {
  return invoke("predict_network_changes");
}

/**
 * Get bandwidth history for visualization and analysis.
 *
 * @returns Array of (timestamp, speed_bps) tuples
 */
export async function getBandwidthHistory(): Promise<[number, number][]> {
  return invoke("get_bandwidth_history");
}

/**
 * Format speed in human-readable format.
 *
 * @param bps - Speed in bytes per second
 * @returns Formatted speed string
 */
export function formatSpeed(bps: number): string {
  if (bps >= 1_000_000_000) {
    return `${(bps / 1_000_000_000).toFixed(2)} GB/s`;
  } else if (bps >= 1_000_000) {
    return `${(bps / 1_000_000).toFixed(2)} MB/s`;
  } else if (bps >= 1_000) {
    return `${(bps / 1_000).toFixed(2)} KB/s`;
  } else {
    return `${bps} B/s`;
  }
}

/**
 * Format bytes in human-readable format.
 *
 * @param bytes - Number of bytes
 * @returns Formatted bytes string
 */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) {
    return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
  } else if (bytes >= 1_000_000) {
    return `${(bytes / 1_000_000).toFixed(2)} MB`;
  } else if (bytes >= 1_000) {
    return `${(bytes / 1_000).toFixed(2)} KB`;
  } else {
    return `${bytes} B`;
  }
}

/**
 * Format duration in human-readable format.
 *
 * @param seconds - Duration in seconds
 * @returns Formatted duration string
 */
export function formatDuration(seconds: number): string {
  if (seconds < 60) {
    return `${Math.floor(seconds)}s`;
  } else if (seconds < 3600) {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}m ${secs}s`;
  } else if (seconds < 86400) {
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${mins}m`;
  } else {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    return `${days}d ${hours}h`;
  }
}

/**
 * Calculate download time for a file given a speed.
 *
 * @param fileSizeBytes - File size in bytes
 * @param speedBps - Download speed in bytes per second
 * @returns Duration in seconds
 */
export function calculateDownloadTime(
  fileSizeBytes: number,
  speedBps: number
): number {
  if (speedBps === 0) return 0;
  return fileSizeBytes / speedBps;
}

/**
 * Get network health status as emoji + description.
 *
 * @param healthScore - Score from 0-100
 * @returns Status emoji and description
 */
export function getHealthStatus(healthScore: number): string {
  if (healthScore >= 80) {
    return `✅ Excellent (${healthScore}/100)`;
  } else if (healthScore >= 60) {
    return `✓ Good (${healthScore}/100)`;
  } else if (healthScore >= 40) {
    return `⚠️ Fair (${healthScore}/100)`;
  } else if (healthScore >= 20) {
    return `⚠️ Poor (${healthScore}/100)`;
  } else {
    return `❌ Critical (${healthScore}/100)`;
  }
}

/**
 * Estimate speed improvement from parallel downloads.
 *
 * @param baseSpeed - Current single connection speed
 * @param parallelConnections - Number of parallel connections
 * @param conservativeFactor - Loss factor (0.7 = 30% loss, default)
 * @returns Estimated aggregate speed
 */
export function estimateParallelSpeedup(
  baseSpeed: number,
  parallelConnections: number,
  conservativeFactor: number = 0.7
): number {
  return baseSpeed * parallelConnections * conservativeFactor;
}

/**
 * Calculate speedup factor.
 *
 * @param originalSpeed - Original speed in bytes/sec
 * @param acceleratedSpeed - Accelerated speed in bytes/sec
 * @returns Speedup factor (e.g., 2.5x)
 */
export function calculateSpeedup(
  originalSpeed: number,
  acceleratedSpeed: number
): number {
  if (originalSpeed === 0) return 0;
  return acceleratedSpeed / originalSpeed;
}

/**
 * Analyze bandwidth trend from measurements.
 *
 * @param measurements - Array of speed measurements
 * @returns Trend direction: "improving" | "stable" | "degrading"
 */
export function analyzeTrend(measurements: number[]): string {
  if (measurements.length < 2) return "insufficient_data";

  const first_half_avg =
    measurements.slice(0, Math.floor(measurements.length / 2)).reduce((a, b) => a + b, 0) /
    Math.floor(measurements.length / 2);

  const second_half_avg =
    measurements.slice(Math.floor(measurements.length / 2)).reduce((a, b) => a + b, 0) /
    Math.ceil(measurements.length / 2);

  if (second_half_avg > first_half_avg * 1.05) {
    return "improving";
  } else if (second_half_avg < first_half_avg * 0.95) {
    return "degrading";
  } else {
    return "stable";
  }
}

/**
 * Create acceleration report for display.
 *
 * @param stats - Acceleration statistics
 * @returns Formatted report
 */
export function createAccelerationReport(stats: AccelerationStats): string {
  return `Acceleration Report:
    
Average Speed: ${formatSpeed(stats.avg_speed_bps)}
Max Speed: ${formatSpeed(stats.max_speed_bps)}
Min Speed: ${formatSpeed(stats.min_speed_bps)}
Variance: ${stats.speed_variance.toFixed(2)} B/s

Network Condition: ${stats.network_condition}
Health Score: ${getHealthStatus(stats.health_score)}
Measurements: ${stats.measurements_count}

Predictions:
- Improvement likely: ${stats.predicted_improvement ? "Yes ✓" : "No"}
- Degradation likely: ${stats.predicted_degradation ? "Yes ⚠️" : "No"}`;
}

/**
 * Estimate savings in download time with acceleration.
 *
 * @param fileSizeBytes - File size
 * @param currentSpeed - Current/baseline speed
 * @param acceleratedSpeed - Accelerated speed
 * @returns Object with original time, accelerated time, and savings
 */
export function estimateTimeSavings(
  fileSizeBytes: number,
  currentSpeed: number,
  acceleratedSpeed: number
): {
  original_time_secs: number;
  accelerated_time_secs: number;
  savings_secs: number;
  speedup_factor: number;
} {
  const originalTime = currentSpeed > 0 ? fileSizeBytes / currentSpeed : 0;
  const acceleratedTime = acceleratedSpeed > 0 ? fileSizeBytes / acceleratedSpeed : 0;

  return {
    original_time_secs: originalTime,
    accelerated_time_secs: acceleratedTime,
    savings_secs: originalTime - acceleratedTime,
    speedup_factor: originalTime > 0 ? originalTime / acceleratedTime : 0,
  };
}
