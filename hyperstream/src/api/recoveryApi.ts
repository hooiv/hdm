import { invoke } from '@tauri-apps/api/core';

export interface CorruptionEvidence {
  segment_id: number;
  segment_start: number;
  segment_end: number;
  corruption_type: CorruptionType;
  confidence: number;
  detected_at_ms: number;
  evidence_data: string;
}

export type CorruptionType =
  | { SizeMismatch: { expected: number; actual: number } }
  | { ChecksumMismatch: { expected: string; computed: string; algorithm: string } }
  | { ZeroEntropy: null }
  | { LowEntropy: { entropy: number; threshold: number } }
  | { IncompleteTransfer: { bytes_received: number; bytes_claimed: number } }
  | { HTTPStatusMismatch: { status: number; reason: string } }
  | { SegmentHashMismatch: { expected_hash: string; computed_hash: string } };

export type RecoveryStrategy =
  | {
      RetryOriginal: {
        attempt: number;
        max_attempts: number;
        backoff_ms: number;
      };
    }
  | {
      SwitchMirror: {
        current_mirror_url: string;
        fallback_mirror_url: string;
      };
    }
  | {
      ResumeFromOffset: {
        byte_offset: number;
        previous_downloaded: number;
      };
    }
  | {
      SkipSegmentResumeAfter: {
        segment_index: number;
        next_segment_offset: number;
      };
    }
  | {
      PauseForUserInput: {
        reason: string;
        suggested_action: string;
      };
    };

export interface MirrorReliability {
  url: string;
  success_count: number;
  failure_count: number;
  corruption_count: number;
  average_speed_bps: number;
  last_used_ms: number;
  score: number;
}

export interface RecoveryAttempt {
  segment_id: number;
  strategy: RecoveryStrategy;
  succeeded: boolean;
  reason: string;
  duration_ms: number;
  attempted_at_ms: number;
}

/**
 * Detect potential corruption in downloaded segment data
 */
export async function detectCorruption(
  downloadId: string,
  segmentId: number,
  segmentStart: number,
  segmentEnd: number,
  dataSample: Uint8Array,
  expectedChecksum?: string,
  expectedSize?: number,
  algorithm?: string
): Promise<CorruptionEvidence | null> {
  return invoke<CorruptionEvidence | null>('detect_corruption', {
    download_id: downloadId,
    segment_id: segmentId,
    segment_start: segmentStart,
    segment_end: segmentEnd,
    data_sample: Array.from(dataSample),
    expected_checksum: expectedChecksum,
    expected_size: expectedSize,
    algorithm,
  });
}

/**
 * Get recommended recovery strategy for a corrupted segment
 */
export async function getRecoveryStrategy(
  downloadId: string,
  segmentId: number,
  segmentStart: number,
  segmentEnd: number,
  originalUrl: string,
  alternativeMirrors: string[]
): Promise<RecoveryStrategy> {
  return invoke<RecoveryStrategy>('get_recovery_strategy', {
    download_id: downloadId,
    segment_id: segmentId,
    segment_start: segmentStart,
    segment_end: segmentEnd,
    original_url: originalUrl,
    alternative_mirrors: alternativeMirrors,
  });
}

/**
 * Execute a recovery strategy
 */
export async function executeRecovery(
  downloadId: string,
  segmentId: number,
  strategy: RecoveryStrategy
): Promise<string> {
  return invoke<string>('execute_recovery', {
    download_id: downloadId,
    segment_id: segmentId,
    strategy,
  });
}

/**
 * Get full corruption report for a download
 */
export async function getCorruptionReport(downloadId: string): Promise<CorruptionEvidence[]> {
  return invoke<CorruptionEvidence[]>('get_corruption_report', {
    download_id: downloadId,
  });
}

/**
 * Get mirror reliability rankings
 */
export async function getMirrorRankings(): Promise<MirrorReliability[]> {
  return invoke<MirrorReliability[]>('get_mirror_rankings', {});
}

/**
 * Update mirror reliability score after a download attempt
 */
export async function updateMirrorReliability(
  url: string,
  success: boolean,
  hadCorruption: boolean,
  avgSpeedBps: number
): Promise<void> {
  return invoke<void>('update_mirror_reliability', {
    url,
    success,
    had_corruption: hadCorruption,
    avg_speed_bps: avgSpeedBps,
  });
}

/**
 * Clean up old recovery data (>7 days)
 */
export async function cleanupRecoveryData(): Promise<void> {
  return invoke<void>('cleanup_recovery_data', {});
}

/**
 * Parse corruption type for display
 */
export function formatCorruptionType(corrupted: CorruptionType): string {
  if ('SizeMismatch' in corrupted) {
    const m = corrupted.SizeMismatch;
    return `Size mismatch: expected ${m.expected}, got ${m.actual}`;
  }
  if ('ChecksumMismatch' in corrupted) {
    const m = corrupted.ChecksumMismatch;
    return `${m.algorithm} mismatch: expected ${m.expected.substring(0, 16)}..., computed ${m.computed.substring(0, 16)}...`;
  }
  if ('ZeroEntropy' in corrupted) {
    return 'Zero entropy (all bytes identical)';
  }
  if ('LowEntropy' in corrupted) {
    const m = corrupted.LowEntropy;
    return `Low entropy ${m.entropy.toFixed(4)} (threshold: ${m.threshold.toFixed(4)})`;
  }
  if ('IncompleteTransfer' in corrupted) {
    const m = corrupted.IncompleteTransfer;
    return `Incomplete transfer: received ${m.bytes_received}, claimed ${m.bytes_claimed}`;
  }
  if ('HTTPStatusMismatch' in corrupted) {
    const m = corrupted.HTTPStatusMismatch;
    return `HTTP ${m.status}: ${m.reason}`;
  }
  if ('SegmentHashMismatch' in corrupted) {
    const m = corrupted.SegmentHashMismatch;
    return `Segment hash mismatch: expected ${m.expected_hash.substring(0, 16)}..., computed ${m.computed_hash.substring(0, 16)}...`;
  }
  return 'Unknown corruption type';
}

/**
 * Parse recovery strategy for display
 */
export function formatRecoveryStrategy(strategy: RecoveryStrategy): string {
  if ('RetryOriginal' in strategy) {
    const s = strategy.RetryOriginal;
    return `Retry original source (attempt ${s.attempt}/${s.max_attempts}, backoff ${s.backoff_ms}ms)`;
  }
  if ('SwitchMirror' in strategy) {
    const s = strategy.SwitchMirror;
    return `Switch mirror: ${s.fallback_mirror_url}`;
  }
  if ('ResumeFromOffset' in strategy) {
    const s = strategy.ResumeFromOffset;
    return `Resume from byte offset ${s.byte_offset}`;
  }
  if ('SkipSegmentResumeAfter' in strategy) {
    const s = strategy.SkipSegmentResumeAfter;
    return `Skip segment, resume at offset ${s.next_segment_offset}`;
  }
  if ('PauseForUserInput' in strategy) {
    const s = strategy.PauseForUserInput;
    return `Paused: ${s.reason} (Suggested: ${s.suggested_action})`;
  }
  return 'Unknown recovery strategy';
}
