import { invoke } from '@tauri-apps/api/core';

/**
 * Configuration for parallel mirror retry strategy
 */
export interface ParallelRetryConfig {
  /** Maximum number of concurrent mirror attempts (2-10) */
  max_concurrent_mirrors: number;
  /** Timeout for each mirror attempt in seconds */
  attempt_timeout_secs: number;
  /** Only use mirrors with score >= this threshold */
  min_mirror_score_threshold: number;
  /** Enable bandwidth aggregation across mirrors */
  enable_bandwidth_aggregation: boolean;
  /** Backoff multiplier when all mirrors fail */
  failure_backoff_multiplier: number;
}

/**
 * Individual mirror result in simulation
 */
export interface MirrorSimulationResult {
  /** Which mirror (0-indexed) */
  mirror_index: number;
  /** Speed of this mirror (bytes per second) */
  speed_bps: number;
  /** Time for this mirror to complete segment */
  duration_ms: number;
  /** Whether this mirror succeeded */
  succeeded: boolean;
}

/**
 * Simulation result showing expected outcomes
 */
export interface ParallelRetrySimulation {
  /** Total number of mirrors available */
  num_mirrors: number;
  /** Number of mirrors that succeeded */
  num_successful: number;
  /** Size of segment being downloaded */
  segment_size_bytes: number;
  /** Individual mirror results */
  individual_results: MirrorSimulationResult[];
  /** Combined speed from all successful mirrors */
  aggregated_speed_bps: number;
  /** Expected time to complete the segment */
  expected_completion_ms: number;
  /** Speedup percentage vs. single fastest mirror */
  improvement_vs_single: number;
}

/**
 * Get current parallel retry configuration
 */
export async function getParallelRetryConfig(): Promise<ParallelRetryConfig> {
  return invoke<ParallelRetryConfig>('get_parallel_retry_config', {});
}

/**
 * Update parallel retry configuration
 */
export async function updateParallelRetryConfig(
  config: ParallelRetryConfig
): Promise<void> {
  return invoke<void>('update_parallel_retry_config', { new_config: config });
}

/**
 * Select optimal mirrors based on scores
 * 
 * @param availableMirrors Array of [url, score] pairs
 * @param maxConcurrent Maximum mirrors to use
 * @param minScore Minimum reliability score (0-100)
 * @returns Selected mirror URLs
 */
export async function selectOptimalMirrors(
  availableMirrors: [string, number][],
  maxConcurrent: number,
  minScore: number
): Promise<string[]> {
  return invoke<string[]>('select_optimal_mirrors', {
    available_mirrors: availableMirrors,
    max_concurrent: maxConcurrent,
    min_score: minScore,
  });
}

/**
 * Estimate combined throughput from multiple mirrors
 * 
 * @param individualSpeedsBps Array of speeds in bytes/second
 * @param conservative If true: max + (second_max/2). If false: sum of all
 * @returns Estimated aggregated speed (bytes per second)
 */
export async function estimateAggregatedThroughput(
  individualSpeedsBps: number[],
  conservative: boolean = true
): Promise<number> {
  return invoke<number>('estimate_aggregated_throughput', {
    individual_speeds_bps: individualSpeedsBps,
    conservative,
  });
}

/**
 * Simulate a parallel mirror retry scenario
 * 
 * Shows how much faster parallel downloads would be with a given set of mirrors.
 * Useful for users to decide whether to enable parallel retry.
 * 
 * @param mirrorSpeedsBps Speed of each mirror (bytes/second)
 * @param segmentSizeBytes Amount of data to download
 * @param numSuccessful How many mirrors succeed (others fail/timeout)
 * @returns Simulation showing speedup and completion time
 */
export async function simulateParallelRetry(
  mirrorSpeedsBps: number[],
  segmentSizeBytes: number,
  numSuccessful: number
): Promise<ParallelRetrySimulation> {
  return invoke<ParallelRetrySimulation>('simulate_parallel_retry', {
    mirror_speeds_bps: mirrorSpeedsBps,
    segment_size_bytes: segmentSizeBytes,
    num_successful: numSuccessful,
  });
}

/**
 * Format a configuration forreadable display
 */
export function formatRetryConfig(config: ParallelRetryConfig): string {
  return `Parallel Retry: ${config.max_concurrent_mirrors} mirrors max, ${config.attempt_timeout_secs}s timeout, min score ${config.min_mirror_score_threshold}`;
}

/**
 * Format simulation result for display
 */
export function formatSimulationResult(sim: ParallelRetrySimulation): string {
  const speedupPercent = sim.improvement_vs_single;
  const completionSecs = (sim.expected_completion_ms / 1000).toFixed(2);
  const speedMbps = (sim.aggregated_speed_bps / 1_000_000).toFixed(2);
  return `${sim.num_successful}/${sim.num_mirrors} mirrors | ${speedMbps}MB/s (+${speedupPercent}%) | ~${completionSecs}s completion`;
}

/**
 * Estimate cost savings from parallel downloads
 * 
 * Shows time saved vs. single fastest mirror
 */
export function estimateTimeSavings(sim: ParallelRetrySimulation): {
  singleMirrorMs: number;
  parallelMs: number;
  savedMs: number;
  percentFaster: number;
} {
  const singleMirrorMs = sim.expected_completion_ms * (100 / sim.improvement_vs_single);
  const savedMs = singleMirrorMs - sim.expected_completion_ms;
  const percentFaster = ((savedMs / singleMirrorMs) * 100).toFixed(1);

  return {
    singleMirrorMs: Math.round(singleMirrorMs),
    parallelMs: sim.expected_completion_ms,
    savedMs: Math.round(savedMs),
    percentFaster: parseFloat(percentFaster),
  };
}
