//! Smart Route Manager API
//! 
//! TypeScript bindings for the smart route optimization engine.
//! Provides type-safe access to intelligent mirror selection and routing.

import { invoke } from '@tauri-apps/api/core';

export interface MirrorHealthMetrics {
  url: string;
  reliability_score: number;
  speed_score: number;
  uptime_percent: number;
  risk_level: string;
  success_count: number;
  failure_count: number;
  avg_latency_ms: number;
  last_segment_speed_bps: number;
  last_update_ms: number;
}

export interface RouteRiskAssessment {
  level: string;
  confidence_percent: number;
  factors: Array<{
    name: string;
    severity: string;
    description: string;
  }>;
  recommended_action: string;
}

export interface RouteStatus {
  download_id: string;
  current_mirror: string;
  mirrors_in_use: MirrorHealthMetrics[];
  total_bandwidth_bps: number;
  primary_bandwidth_bps: number;
  secondary_bandwidth_bps: number;
  failover_count: number;
  predicted_completion_secs: number;
  risk_assessment: RouteRiskAssessment;
  last_mirror_switch_reason?: string;
  last_mirror_switch_ms?: number;
  is_pooling_bandwidth: boolean;
}

export interface RouteDecision {
  decision_id: string;
  download_id: string;
  primary_mirror: string;
  fallback_mirrors: string[];
  parallel_mirrors: string[];
  failure_risk_percent: number;
  reason: string;
  created_at_ms: number;
  is_active: boolean;
}

export interface RouteHistoryEntry {
  decision_id: string;
  download_id: string;
  timestamp_ms: number;
  action: string;
  from_mirror?: string;
  to_mirror?: string;
  reason: string;
  speed_before_bps: number;
  speed_after_bps: number;
}

export interface SmartRouteConfig {
  enabled: boolean;
  max_parallel_mirrors: number;
  min_mirror_score_threshold: number;
  reevaluation_interval_secs: number;
  failover_prediction_threshold: number;
  bandwidth_pooling_threshold_bps: number;
  decision_change_smoothing: number;
}

export interface RouteDashboardSnapshot {
  download_id: string;
  route_status: RouteStatus | null;
  mirror_rankings: MirrorHealthMetrics[];
  recent_decisions: RouteHistoryEntry[];
}

/**
 * Get current route status for a download
 */
export async function getRouteStatus(downloadId: string): Promise<RouteStatus | null> {
  return invoke<RouteStatus | null>('get_route_status', { download_id: downloadId });
}

/**
 * Optimize the route for a download given available mirrors
 */
export async function optimizeDownloadRoute(
  downloadId: string,
  availableMirrors: Array<[string, number]>,
  currentSpeedBps: number,
  remainingBytes: number
): Promise<RouteDecision> {
  return invoke<RouteDecision>('optimize_download_route', {
    download_id: downloadId,
    available_mirrors: availableMirrors,
    current_speed_bps: currentSpeedBps,
    remaining_bytes: remainingBytes,
  });
}

/**
 * Get all mirrors ranked by current health
 */
export async function getMirrorHealthRankings(): Promise<MirrorHealthMetrics[]> {
  return invoke<MirrorHealthMetrics[]>('get_mirror_health_rankings');
}

/**
 * Get route decision history for a download
 */
export async function getRouteDecisionHistory(
  downloadId: string,
  limit: number = 20
): Promise<RouteHistoryEntry[]> {
  return invoke<RouteHistoryEntry[]>('get_route_decision_history', {
    download_id: downloadId,
    limit,
  });
}

/**
 * Get current smart route configuration
 */
export async function getSmartRouteConfig(): Promise<SmartRouteConfig> {
  return invoke<SmartRouteConfig>('get_smart_route_config');
}

/**
 * Update smart route configuration
 */
export async function updateSmartRouteConfig(config: SmartRouteConfig): Promise<void> {
  return invoke<void>('update_smart_route_config', { config });
}

/**
 * Get all route metrics for a download in a single call (efficient for dashboards)
 */
export async function getRouteDashboardSnapshot(
  downloadId: string,
  historyLimit: number = 20
): Promise<RouteDashboardSnapshot> {
  return invoke<RouteDashboardSnapshot>('get_route_dashboard_snapshot', {
    download_id: downloadId,
    history_limit: historyLimit,
  });
}

/**
 * Record route decision telemetry for analysis and continuous improvement
 */
export async function recordRouteDecisionOutcome(
  downloadId: string,
  decisionId: string,
  mirrorUrl: string,
  success: boolean,
  durationMs: number,
  bytesTransferred: number
): Promise<void> {
  return invoke<void>('record_route_decision_outcome', {
    download_id: downloadId,
    decision_id: decisionId,
    mirror_url: mirrorUrl,
    success,
    duration_ms: durationMs,
    bytes_transferred: bytesTransferred,
  });
}

// Utility functions

/**
 * Format bandwidth in human-readable format
 */
export function formatBandwidth(bps: number): string {
  if (bps >= 1_000_000_000) return `${(bps / 1_000_000_000).toFixed(2)} GB/s`;
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} MB/s`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(2)} KB/s`;
  return `${bps} B/s`;
}

/**
 * Get risk level color for UI
 */
export function getRiskColor(level: string): string {
  switch (level.toLowerCase()) {
    case 'healthy':
    case 'safe':
      return 'text-green-400';
    case 'caution':
      return 'text-yellow-400';
    case 'warning':
      return 'text-orange-400';
    case 'critical':
      return 'text-red-400';
    default:
      return 'text-cyan-400';
  }
}

/**
 * Get bandwidth pooling efficiency percentage
 * Accounts for overhead from parallel mirror connections
 */
export function calculatePoolingEfficiency(
  primarySpeed: number,
  secondarySpeed: number,
  parallelCount: number
): number {
  if (primarySpeed === 0) return 0;
  // Conservative efficiency: secondary contributes 70% of its speed
  // Reduces by 5% per additional parallel mirror due to coordination overhead
  const overheadFactor = Math.max(0.5, 1 - (parallelCount - 1) * 0.05);
  const efficiency = (secondarySpeed * 0.7 * overheadFactor) / primarySpeed;
  return Math.min(efficiency * 100, 100);
}

/**
 * Estimate time to completion
 */
export function estimateCompletion(
  remainingBytes: number,
  currentSpeedBps: number
): string {
  if (currentSpeedBps === 0) return 'Unknown';

  const seconds = remainingBytes / currentSpeedBps;

  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    const secs = Math.round(seconds % 60);
    return `${minutes}m ${secs}s`;
  }

  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

/**
 * Determine if a route change would be beneficial
 */
export function shouldSwitchMirror(
  currentFailureRisk: number,
  alternativeFailureRisk: number,
  failoverThreshold: number = 60
): boolean {
  // Switch if current is risky and alternative is safer
  if (currentFailureRisk > failoverThreshold && alternativeFailureRisk < currentFailureRisk - 20) {
    return true;
  }
  return false;
}

export default {
  getRouteStatus,
  optimizeDownloadRoute,
  getMirrorHealthRankings,
  getRouteDecisionHistory,
  getSmartRouteConfig,
  updateSmartRouteConfig,
  getRouteDashboardSnapshot,
  formatBandwidth,
  getRiskColor,
  calculatePoolingEfficiency,
  estimateCompletion,
  shouldSwitchMirror,
};
