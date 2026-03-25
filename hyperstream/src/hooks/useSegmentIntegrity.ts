import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback, useEffect } from 'react';

export interface SegmentIntegrityInfo {
  segment_id: number;
  start_byte: number;
  end_byte: number;
  expected_size: number;
  actual_size: number;
  size_valid: boolean;
  checksum: string | null;
  expected_checksum: string | null;
  checksum_valid: boolean;
  entropy: number;
  appears_corrupted: boolean;
  integrity_score: number;
  verified_at_ms: number;
  verification_duration_ms: number;
}

export interface IntegrityReport {
  download_id: string;
  file_path: string;
  total_size: number;
  segments: SegmentIntegrityInfo[];
  failed_segments: number[];
  overall_score: number;
  risk_level: 'Healthy' | 'Caution' | 'Warning' | 'Critical';
  at_risk_percentage: number;
  recommendations: string[];
  generated_at_ms: number;
  total_duration_ms: number;
  parallel_degree: number;
}

export interface IntegrityMetrics {
  total_segments_verified: number;
  total_corruptions_detected: number;
  auto_recovery_attempts: number;
  auto_recovery_success: number;
  average_verification_time_ms: number;
  average_integrity_score: number;
}

export interface RecoveryStrategy {
  segment_id: number;
  action: 'Redownload' | 'SwitchMirror' | 'ReduceSize' | 'ManualIntervention' | 'TruncateAndRestart';
  priority: number;
  reason: string;
}

/**
 * Hook for verifying download segment integrity
 */
export const useSegmentIntegrity = (downloadId: string) => {
  const [report, setReport] = useState<IntegrityReport | null>(null);
  const [isVerifying, setIsVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Verify all segments in a download
  const verifyDownload = useCallback(async () => {
    setIsVerifying(true);
    setError(null);

    try {
      const result = await invoke<IntegrityReport>('verify_download_integrity', { downloadId });
      setReport(result);
      return result;
    } catch (err) {
      const errorMsg = String(err);
      setError(errorMsg);
      console.error('Segment verification failed:', errorMsg);
      return null;
    } finally {
      setIsVerifying(false);
    }
  }, [downloadId]);

  // Verify specific segments
  const verifySegments = useCallback(
    async (segmentIndices: number[]) => {
      setIsVerifying(true);
      setError(null);

      try {
        const results = await invoke<SegmentIntegrityInfo[]>('verify_segments', {
          downloadId,
          segmentIndices,
        });
        return results;
      } catch (err) {
        const errorMsg = String(err);
        setError(errorMsg);
        console.error('Segment verification failed:', errorMsg);
        return null;
      } finally {
        setIsVerifying(false);
      }
    },
    [downloadId]
  );

  // Get cached report
  const getCachedReport = useCallback(async () => {
    try {
      const result = await invoke<IntegrityReport | null>('get_cached_integrity_report', { downloadId });
      if (result) {
        setReport(result);
      }
      return result;
    } catch (err) {
      console.error('Failed to get cached report:', err);
      return null;
    }
  }, [downloadId]);

  // Get summary
  const getSummary = useCallback(async () => {
    try {
      const result = await invoke<any>('get_integrity_summary', { downloadId });
      return result;
    } catch (err) {
      console.error('Failed to get summary:', err);
      return null;
    }
  }, [downloadId]);

  // Generate recovery strategies
  const generateRecoveryStrategies = useCallback(async () => {
    try {
      const result = await invoke<RecoveryStrategy[]>('generate_recovery_strategies', { downloadId });
      return result;
    } catch (err) {
      console.error('Failed to generate recovery strategies:', err);
      return null;
    }
  }, [downloadId]);

  return {
    report,
    isVerifying,
    error,
    verifyDownload,
    verifySegments,
    getCachedReport,
    getSummary,
    generateRecoveryStrategies,
  };
};

/**
 * Hook for monitoring system-wide integrity metrics
 */
export const useIntegrityMetrics = () => {
  const [metrics, setMetrics] = useState<IntegrityMetrics | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const fetchMetrics = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await invoke<IntegrityMetrics>('get_integrity_monitoring_metrics');
      setMetrics(result);
      return result;
    } catch (err) {
      console.error('Failed to fetch metrics:', err);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchMetrics();
    const interval = setInterval(fetchMetrics, 10000); // Update every 10s
    return () => clearInterval(interval);
  }, [fetchMetrics]);

  return { metrics, isLoading, fetchMetrics };
};

/**
 * Hook for batch verification of multiple downloads
 */
export const useBatchIntegrityVerification = () => {
  const [results, setResults] = useState<Array<[string, IntegrityReport]>>([]);
  const [isVerifying, setIsVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const batchVerify = useCallback(async (downloadIds: string[]) => {
    if (downloadIds.length === 0) return;

    setIsVerifying(true);
    setError(null);

    try {
      const result = await invoke<Array<[string, IntegrityReport]>>('batch_verify_downloads', {
        downloadIds,
      });
      setResults(result);
      return result;
    } catch (err) {
      const errorMsg = String(err);
      setError(errorMsg);
      console.error('Batch verification failed:', errorMsg);
      return null;
    } finally {
      setIsVerifying(false);
    }
  }, []);

  return { results, isVerifying, error, batchVerify };
};

/**
 * Utility functions for integrity scoring and severity
 */
export const integrityUtils = {
  /**
   * Get human-readable risk level
   */
  getRiskLevelLabel: (level: string): string => {
    switch (level) {
      case 'Healthy':
        return '✅ Healthy';
      case 'Caution':
        return '⚠️ Caution';
      case 'Warning':
        return '⚠️ Warning';
      case 'Critical':
        return '🚫 Critical';
      default:
        return 'Unknown';
    }
  },

  /**
   * Get color class for risk level
   */
  getRiskColor: (level: string): string => {
    switch (level) {
      case 'Healthy':
        return 'text-green-400';
      case 'Caution':
        return 'text-yellow-400';
      case 'Warning':
        return 'text-orange-400';
      case 'Critical':
        return 'text-red-400';
      default:
        return 'text-slate-400';
    }
  },

  /**
   * Get background class for risk level
   */
  getRiskBackground: (level: string): string => {
    switch (level) {
      case 'Healthy':
        return 'bg-green-500/10 border-green-500/20';
      case 'Caution':
        return 'bg-yellow-500/10 border-yellow-500/20';
      case 'Warning':
        return 'bg-orange-500/10 border-orange-500/20';
      case 'Critical':
        return 'bg-red-500/10 border-red-500/20';
      default:
        return 'bg-slate-500/10 border-slate-500/20';
    }
  },

  /**
   * Get color based on integrity score
   */
  getScoreColor: (score: number): string => {
    if (score >= 90) return 'text-green-400';
    if (score >= 70) return 'text-yellow-400';
    if (score >= 50) return 'text-orange-400';
    return 'text-red-400';
  },

  /**
   * Check if download can be safely resumed
   */
  canResume: (report: IntegrityReport): boolean => {
    return report.overall_score >= 70 && report.at_risk_percentage < 0.1;
  },

  /**
   * Check if restart is recommended
   */
  shouldRestart: (report: IntegrityReport): boolean => {
    return report.overall_score < 60 || report.at_risk_percentage > 0.3;
  },

  /**
   * Get entropy as percentage string
   */
  getEntropyLabel: (entropy: number): string => {
    const percent = entropy * 100;
    if (percent < 20) return 'Very Low (suspicious)';
    if (percent < 50) return 'Low';
    if (percent < 80) return 'Normal';
    return 'High (acceptable)';
  },

  /**
   * Format bytes to human-readable size
   */
  formatBytes: (bytes: number): string => {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = bytes;
    let unitIndex = 0;

    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }

    return `${size.toFixed(2)} ${units[unitIndex]}`;
  },
};
