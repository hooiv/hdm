import { useState, useCallback, useEffect, useRef } from 'react';
import * as SmartRouteAPI from '../api/smartRouteApi';

/**
 * Hook for accessing smart route optimization data
 */
export function useSmartRoute(downloadId: string | null) {
  const [routeStatus, setRouteStatus] = useState<SmartRouteAPI.RouteStatus | null>(null);
  const [mirrorRankings, setMirrorRankings] = useState<SmartRouteAPI.MirrorHealthMetrics[]>([]);
  const [history, setHistory] = useState<SmartRouteAPI.RouteHistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<number>(0);

  const fetchData = useCallback(async () => {
    if (!downloadId) return;

    setLoading(true);
    try {
      const [status, rankings, historyData] = await Promise.all([
        SmartRouteAPI.getRouteStatus(downloadId).catch(() => null),
        SmartRouteAPI.getMirrorHealthRankings().catch(() => []),
        SmartRouteAPI.getRouteDecisionHistory(downloadId, 20).catch(() => []),
      ]);

      setRouteStatus(status);
      setMirrorRankings(rankings);
      setHistory(historyData);
      setLastUpdate(Date.now());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [downloadId]);

  // Auto-refresh every 3 seconds
  useEffect(() => {
    if (!downloadId) return;

    fetchData();
    const interval = setInterval(fetchData, 3000);
    return () => clearInterval(interval);
  }, [downloadId, fetchData]);

  return {
    routeStatus,
    mirrorRankings,
    history,
    loading,
    error,
    lastUpdate,
    refetch: fetchData,
  };
}

/**
 * Hook for optimizing a route
 */
export function useRouteOptimization(downloadId: string | null) {
  const [decision, setDecision] = useState<SmartRouteAPI.RouteDecision | null>(null);
  const [optimizing, setOptimizing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const optimize = useCallback(
    async (
      availableMirrors: Array<[string, number]>,
      currentSpeedBps: number,
      remainingBytes: number
    ) => {
      if (!downloadId) return null;

      setOptimizing(true);
      try {
        const result = await SmartRouteAPI.optimizeDownloadRoute(
          downloadId,
          availableMirrors,
          currentSpeedBps,
          remainingBytes
        );
        setDecision(result);
        setError(null);
        return result;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        return null;
      } finally {
        setOptimizing(false);
      }
    },
    [downloadId]
  );

  return {
    decision,
    optimizing,
    error,
    optimize,
  };
}

/**
 * Hook for smart route configuration
 */
export function useSmartRouteConfig() {
  const [config, setConfig] = useState<SmartRouteAPI.SmartRouteConfig | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    setLoading(true);
    try {
      const cfg = await SmartRouteAPI.getSmartRouteConfig();
      setConfig(cfg);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const updateConfig = useCallback(async (newConfig: SmartRouteAPI.SmartRouteConfig) => {
    try {
      await SmartRouteAPI.updateSmartRouteConfig(newConfig);
      setConfig(newConfig);
      setError(null);
      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      return false;
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  return {
    config,
    loading,
    error,
    updateConfig,
    reload: loadConfig,
  };
}

/**
 * Hook for polling route status efficiently
 */
export function useRouteStatusPolling(
  downloadId: string | null,
  pollingInterval: number = 3000
) {
  const [routeStatus, setRouteStatus] = useState<SmartRouteAPI.RouteStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pollingRef = useRef<NodeJS.Timeout | null>(null);

  const pollStatus = useCallback(async () => {
    if (!downloadId) return;

    setLoading(true);
    try {
      const status = await SmartRouteAPI.getRouteStatus(downloadId);
      setRouteStatus(status);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [downloadId]);

  useEffect(() => {
    if (!downloadId) return;

    // Initial fetch
    pollStatus();

    // Set up polling
    pollingRef.current = setInterval(pollStatus, pollingInterval);

    return () => {
      if (pollingRef.current) {
        clearInterval(pollingRef.current);
      }
    };
  }, [downloadId, pollingInterval, pollStatus]);

  return {
    routeStatus,
    loading,
    error,
    statusLoaded: routeStatus !== null,
  };
}

/**
 * Hook for recording route decision telemetry
 */
export function useRouteDecisionTelemetry(downloadId: string | null) {
  const [isSending, setIsSending] = useState(false);

  const recordOutcome = useCallback(
    async (
      decisionId: string,
      mirrorUrl: string,
      success: boolean,
      durationMs: number,
      bytesTransferred: number
    ) => {
      if (!downloadId) return false;

      setIsSending(true);
      try {
        await SmartRouteAPI.recordRouteDecisionOutcome(
          downloadId,
          decisionId,
          mirrorUrl,
          success,
          durationMs,
          bytesTransferred
        );
        return true;
      } catch (err) {
        console.error('Failed to record route telemetry:', err);
        return false;
      } finally {
        setIsSending(false);
      }
    },
    [downloadId]
  );

  return {
    recordOutcome,
    isSending,
  };
}

export default {
  useSmartRoute,
  useRouteOptimization,
  useSmartRouteConfig,
  useRouteStatusPolling,
};
