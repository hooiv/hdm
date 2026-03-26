//! Mirror Analytics API
//! 
//! TypeScript bindings for the mirror analytics engine.
//! Provides statistical analysis, trend detection, and recommendations.

import { invoke } from "@tauri-apps/api/core";

export interface MirrorStatistics {
  url: string;
  success_count: number;
  failure_count: number;
  success_rate_percent: number;
  average_speed_bps: number;
  max_speed_bps: number;
  min_speed_bps: number;
  speed_std_dev_bps: number;
  corruption_count: number;
  corruption_rate_percent: number;
  avg_response_time_ms: number;
  median_response_time_ms: number;
  p95_response_time_ms: number;
  time_since_last_success_secs: number;
  reliability_score: number;
  trend: string;
  confidence_percent: number;
}

export interface CompareMirrorsRequest {
  mirror_a_url: string;
  mirror_a_success: number;
  mirror_a_failures: number;
  mirror_a_speeds: number[];
  mirror_a_corruptions: number;
  mirror_b_url: string;
  mirror_b_success: number;
  mirror_b_failures: number;
  mirror_b_speeds: number[];
  mirror_b_corruptions: number;
}

export interface AnalyticsReport {
  title: string;
  summary: string;
  metrics: Record<string, any>;
  recommendations: string[];
  timestamp: string;
}

/**
 * Analyze comprehensive statistics for a mirror.
 *
 * @param mirrorUrl - The mirror URL to analyze
 * @param successCount - Number of successful downloads
 * @param failureCount - Number of failed downloads
 * @param speeds - Array of observed download speeds (bytes per second)
 * @param corruptionCount - Number of corruption detections
 * @param responseTimes - Array of response times (milliseconds)
 * @returns Mirror statistics with reliability score and trend analysis
 */
export async function analyzeMirrorStatistics(
  mirrorUrl: string,
  successCount: number,
  failureCount: number,
  speeds: number[],
  corruptionCount: number = 0,
  responseTimes: number[] = []
): Promise<MirrorStatistics> {
  return invoke("analyze_mirror_statistics", {
    request: {
      mirror_url: mirrorUrl,
      success_count: successCount,
      failure_count: failureCount,
      speeds_bps: speeds,
      corruption_count: corruptionCount,
      response_times_ms: responseTimes,
    },
  });
}

/**
 * Compare two mirrors side-by-side.
 *
 * @param mirrorA - First mirror statistics
 * @param mirrorB - Second mirror statistics
 * @returns Detailed comparison report
 */
export async function compareTwoMirrors(
  mirrorAUrl: string,
  mirrorASuccess: number,
  mirrorAFailures: number,
  mirrorASpeeds: number[],
  mirrorACorruptions: number,
  mirrorBUrl: string,
  mirrorBSuccess: number,
  mirrorBFailures: number,
  mirrorBSpeeds: number[],
  mirrorBCorruptions: number
): Promise<string> {
  return invoke("compare_two_mirrors", {
    request: {
      mirror_a_url: mirrorAUrl,
      mirror_a_success: mirrorASuccess,
      mirror_a_failures: mirrorAFailures,
      mirror_a_speeds: mirrorASpeeds,
      mirror_a_corruptions: mirrorACorruptions,
      mirror_b_url: mirrorBUrl,
      mirror_b_success: mirrorBSuccess,
      mirror_b_failures: mirrorBFailures,
      mirror_b_speeds: mirrorBSpeeds,
      mirror_b_corruptions: mirrorBCorruptions,
    },
  });
}

/**
 * Get performance trend for a mirror over time.
 *
 * @param mirrorUrl - The mirror URL
 * @param successCount - Recent successful downloads
 * @param failureCount - Recent failed downloads
 * @returns Trend analysis (improving, stable, or degrading)
 */
export async function getMirrorTrend(
  mirrorUrl: string,
  successCount: number,
  failureCount: number
): Promise<string> {
  return invoke("get_mirror_trend", {
    mirror_url: mirrorUrl,
    success_count: successCount,
    failure_count: failureCount,
  });
}

/**
 * Get recommendation for which mirror to use.
 *
 * @param mirrorUrls - List of mirror URLs
 * @param successRates - Success rate for each mirror (0-100)
 * @param speedsBps - Average speed for each mirror (bytes per second)
 * @returns Recommended mirror URL with rationale
 */
export async function getMirrorRecommendation(
  mirrorUrls: string[],
  successRates: number[],
  speedsBps: number[]
): Promise<string> {
  return invoke("get_mirror_recommendation", {
    mirror_urls: mirrorUrls,
    success_rates: successRates,
    speeds_bps: speedsBps,
  });
}

/**
 * Health check for all mirrors.
 *
 * @param mirrors - Array of (URL, successes, failures) tuples
 * @returns Health status report categorized by tier
 */
export async function healthCheckMirrors(
  mirrors: Array<{ url: string; successes: number; failures: number }>
): Promise<string> {
  const formatted = mirrors.map((m) => [m.url, m.successes, m.failures]);
  return invoke("health_check_mirrors", { mirrors: formatted });
}

/**
 * Calculate percentile metrics for a dataset.
 *
 * @param values - Array of numeric values
 * @returns Percentile breakdown (p50, p95, p99, min, max, avg)
 */
export async function calculatePercentiles(values: number[]): Promise<string> {
  return invoke("calculate_percentiles", { request: { values } });
}

/**
 * Format speed in human-readable format.
 *
 * @param bps - Speed in bytes per second
 * @returns Formatted speed string (MB/s, KB/s, etc.)
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
 * Format time duration in human-readable format.
 *
 * @param milliseconds - Duration in milliseconds
 * @returns Formatted time string
 */
export function formatDuration(milliseconds: number): string {
  const seconds = milliseconds / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(1)}s`;
  } else if (seconds < 3600) {
    return `${(seconds / 60).toFixed(1)}m`;
  } else if (seconds < 86400) {
    return `${(seconds / 3600).toFixed(1)}h`;
  } else {
    return `${(seconds / 86400).toFixed(1)}d`;
  }
}

/**
 * Extract domain from a full URL.
 *
 * @param url - Full URL
 * @returns Base domain (e.g., "example.com" from "https://cdn.example.com/file")
 */
export function extractDomain(url: string): string {
  try {
    const urlObj = new URL(url);
    const hostname = urlObj.hostname;
    // Remove leading "www." if present
    return hostname.startsWith("www.") ? hostname.slice(4) : hostname;
  } catch {
    return url;
  }
}

/**
 * Calculate success rate percentage.
 *
 * @param successes - Number of successful downloads
 * @param failures - Number of failed downloads
 * @returns Success rate as percentage (0-100)
 */
export function calculateSuccessRate(successes: number, failures: number): number {
  const total = successes + failures;
  if (total === 0) return 0;
  return (successes / total) * 100;
}

/**
 * Determine health status based on success rate.
 *
 * @param successRate - Success rate percentage (0-100)
 * @returns Health status: "healthy", "degraded", or "unhealthy"
 */
export function getHealthStatus(successRate: number): "healthy" | "degraded" | "unhealthy" {
  if (successRate >= 95) {
    return "healthy";
  } else if (successRate >= 80) {
    return "degraded";
  } else {
    return "unhealthy";
  }
}

/**
 * Generate an analytics report for display.
 *
 * @param statistics - Mirror statistics
 * @returns Formatted analytics report
 */
export function generateAnalyticsReport(statistics: MirrorStatistics): AnalyticsReport {
  const healthStatus = getHealthStatus(statistics.success_rate_percent);

  return {
    title: `Analytics Report: ${extractDomain(statistics.url)}`,
    summary: `${healthStatus.toUpperCase()}: ${statistics.success_rate_percent.toFixed(1)}% success rate`,
    metrics: {
      successRate: `${statistics.success_rate_percent.toFixed(1)}%`,
      failureCount: statistics.failure_count,
      corruptionRate: `${statistics.corruption_rate_percent.toFixed(2)}%`,
      avgSpeed: formatSpeed(statistics.average_speed_bps),
      maxSpeed: formatSpeed(statistics.max_speed_bps),
      minSpeed: formatSpeed(statistics.min_speed_bps),
      p95ResponseTime: formatDuration(statistics.p95_response_time_ms),
      reliabilityScore: `${statistics.reliability_score}/100`,
      trend: statistics.trend,
      confidence: `${statistics.confidence_percent}%`,
    },
    recommendations: generateRecommendations(statistics),
    timestamp: new Date().toISOString(),
  };
}

/**
 * Generate recommendations based on statistics.
 *
 * @param statistics - Mirror statistics
 * @returns Array of recommendations
 */
export function generateRecommendations(statistics: MirrorStatistics): string[] {
  const recommendations: string[] = [];

  // Check success rate
  if (statistics.success_rate_percent < 80) {
    recommendations.push("⚠️ Low success rate - consider prioritizing other mirrors");
  } else if (statistics.success_rate_percent > 95) {
    recommendations.push("✓ Excellent reliability - prioritize this mirror");
  }

  // Check corruption rate
  if (statistics.corruption_rate_percent > 1) {
    recommendations.push(
      "⚠️ Corruption detected - verify file integrity after download"
    );
  }

  // Check speed
  if (statistics.average_speed_bps > 10_000_000) {
    recommendations.push("⚡ Fast speeds - ideal for large files");
  } else if (statistics.average_speed_bps < 1_000_000) {
    recommendations.push("🐢 Slow speeds - good for small files only");
  }

  // Check response time
  if (statistics.p95_response_time_ms > 5000) {
    recommendations.push("⏱️ High response time - may have connection issues");
  }

  // Check confidence
  if (statistics.confidence_percent < 50) {
    recommendations.push("ℹ️ Limited data - collect more samples for accurate analysis");
  }

  // Trend check
  if (statistics.trend === "degrading") {
    recommendations.push("📉 Performance degrading - monitor closely");
  } else if (statistics.trend === "improving") {
    recommendations.push("📈 Performance improving - increasing reliability");
  }

  if (recommendations.length === 0) {
    recommendations.push("✓ Mirror is performing well with no issues detected");
  }

  return recommendations;
}

/**
 * Create a comparison snapshot between two mirrors.
 *
 * @param stats1 - First mirror statistics
 * @param stats2 - Second mirror statistics
 * @returns Comparison snapshot object
 */
export function createComparisonSnapshot(
  stats1: MirrorStatistics,
  stats2: MirrorStatistics
): {
  fastestMirror: string;
  speedAdvantage: string;
  mostReliable: string;
  reliabilityAdvantage: string;
} {
  const fastestMirror =
    stats1.average_speed_bps > stats2.average_speed_bps ? stats1.url : stats2.url;
  const speedAdvantage =
    Math.abs(stats1.average_speed_bps - stats2.average_speed_bps) / 1_000_000;

  const mostReliable =
    stats1.reliability_score > stats2.reliability_score ? stats1.url : stats2.url;
  const reliabilityAdvantage = Math.abs(
    stats1.reliability_score - stats2.reliability_score
  );

  return {
    fastestMirror,
    speedAdvantage: `${speedAdvantage.toFixed(2)} MB/s`,
    mostReliable,
    reliabilityAdvantage: `${reliabilityAdvantage} points`,
  };
}
