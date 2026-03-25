import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MirrorScore, FailurePrediction } from '../types';

/**
 * Hook for mirror scoring operations
 * Provides methods to query and update mirror scores and predictions
 */
export function useMirrorScoring() {
  const [mirrors, setMirrors] = useState<MirrorScore[]>([]);
  const [loading, setLoading] = useState(false);

  /**
   * Get score for a specific mirror URL
   */
  const getMirrorScore = useCallback(async (url: string): Promise<MirrorScore> => {
    setLoading(true);
    try {
      const score = await invoke<MirrorScore>('get_mirror_score', { url });
      return score;
    } catch (err) {
      console.error('Failed to get mirror score:', err);
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Record a successful download from a mirror
   */
  const recordSuccess = useCallback(
    async (url: string, bytes: number, latency_ms: number): Promise<void> => {
      try {
        await invoke('record_mirror_success', { url, bytes, latency_ms });
      } catch (err) {
        console.error('Failed to record mirror success:', err);
        throw err;
      }
    },
    []
  );

  /**
   * Record a failed download attempt from a mirror
   */
  const recordFailure = useCallback(async (url: string, reason: string): Promise<void> => {
    try {
      await invoke('record_mirror_failure', { url, reason });
    } catch (err) {
      console.error('Failed to record mirror failure:', err);
      throw err;
    }
  }, []);

  /**
   * Get all mirrors ranked by their current score
   */
  const getRankedMirrors = useCallback(async (): Promise<MirrorScore[]> => {
    setLoading(true);
    try {
      const ranked = await invoke<MirrorScore[]>('get_ranked_mirrors', {});
      setMirrors(ranked);
      return ranked;
    } catch (err) {
      console.error('Failed to get ranked mirrors:', err);
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Predict failure risk for a mirror given download parameters
   */
  const predictFailureRisk = useCallback(
    async (url: string, segment_size: number, is_resume: boolean): Promise<FailurePrediction> => {
      try {
        const prediction = await invoke<FailurePrediction>('predict_mirror_failure_risk', {
          url,
          segment_size,
          is_resume,
        });
        return prediction;
      } catch (err) {
        console.error('Failed to predict failure risk:', err);
        throw err;
      }
    },
    []
  );

  return {
    mirrors,
    loading,
    getMirrorScore,
    recordSuccess,
    recordFailure,
    getRankedMirrors,
    predictFailureRisk,
  };
}

/**
 * Hook for auto-refreshing mirror metrics
 * Automatically fetches and updates mirror scores on a 5-second interval
 */
export function useMirrorMetrics() {
  const [metrics, setMetrics] = useState<MirrorScore[]>([]);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [loading, setLoading] = useState(false);

  /**
   * Fetch all mirror metrics from the backend
   */
  const fetchMetrics = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<MirrorScore[]>('get_all_mirror_metrics', {});
      setMetrics(data);
    } catch (err) {
      console.error('Failed to fetch mirror metrics:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Effect: Setup auto-refresh interval on mount, cleanup on unmount
   */
  useEffect(() => {
    if (!autoRefresh) return;

    // Fetch immediately on mount
    fetchMetrics();

    // Set up interval for subsequent fetches
    const interval = setInterval(() => {
      fetchMetrics();
    }, 5000); // 5 second interval

    // Cleanup interval on unmount or when autoRefresh changes
    return () => clearInterval(interval);
  }, [autoRefresh, fetchMetrics]);

  return {
    metrics,
    autoRefresh,
    setAutoRefresh,
    loading,
    fetchMetrics,
  };
}
