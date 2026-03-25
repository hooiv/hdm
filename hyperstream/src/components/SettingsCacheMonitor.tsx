/**
 * Production-grade Settings Cache Monitor Component
 * 
 * Displays:
 * - Real-time cache metrics (hit ratio, performance)
 * - Cache health status with visual indicators
 * - Lock contention and recovery statistics
 * - Emergency fallback recovery controls
 * - Degraded mode indication
 * 
 * This is a production-critical component for monitoring app state integrity.
 */

import React, { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { AlertTriangle, TrendingUp, Activity, RotateCw, Zap } from 'lucide-react';
import { useSettingsCache } from '../hooks/useSettingsCache';

interface MetricsData {
  hits: number;
  misses: number;
  hit_ratio: number;
  invalidations: number;
  saves: number;
  validation_errors: number;
  poisoned_lock_recoveries: number;
  avg_read_time_ms: number;
  avg_write_time_ms: number;
  last_save_duration_ms: number;
  is_degraded: boolean;
}

interface HealthStatus {
  is_healthy: boolean;
  is_degraded: boolean;
  is_fresh: boolean;
  age_seconds: number | null;
  can_read: boolean;
  can_write: boolean;
}

export const SettingsCacheMonitor: React.FC = () => {
  const cacheHook = useSettingsCache();
  const [metrics, setMetrics] = useState<MetricsData | null>(null);
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [isMonitoring, setIsMonitoring] = useState(true);
  const [expandedPanel, setExpandedPanel] = useState<'metrics' | 'health' | 'actions' | null>(null);

  // Refresh metrics and health every 2 seconds
  useEffect(() => {
    if (!isMonitoring) return;

    const refreshData = async () => {
      try {
        const [metricsData, healthData] = await Promise.all([
          cacheHook.getCacheMetrics(),
          cacheHook.checkCacheHealth(),
        ]);
        setMetrics(metricsData);
        setHealth(healthData);
      } catch (err) {
        console.error('Failed to refresh cache monitor data:', err);
      }
    };

    refreshData(); // Initial call
    const interval = setInterval(refreshData, 2000);
    return () => clearInterval(interval);
  }, [isMonitoring, cacheHook]);

  if (!metrics || !health) {
    return (
      <div className="p-4 text-center text-gray-400">
        <Activity className="w-4 h-4 inline animate-spin mr-2" />
        Loading cache metrics...
      </div>
    );
  }

  // Calculate hit ratio color
  const getHitRatioColor = (ratio: number) => {
    if (ratio >= 0.8) return 'bg-green-500/20 text-green-400';
    if (ratio >= 0.6) return 'bg-blue-500/20 text-blue-400';
    if (ratio >= 0.4) return 'bg-yellow-500/20 text-yellow-400';
    return 'bg-red-500/20 text-red-400';
  };

  const handleRecovery = async () => {
    try {
      await cacheHook.recoverFromFallback();
      // Refresh immediately after recovery
      const healthData = await cacheHook.checkCacheHealth();
      setHealth(healthData);
    } catch (err) {
      console.error('Recovery failed:', err);
    }
  };

  const handleForceRefresh = async () => {
    try {
      await cacheHook.forceCacheRefresh();
      const healthData = await cacheHook.checkCacheHealth();
      setHealth(healthData);
    } catch (err) {
      console.error('Force refresh failed:', err);
    }
  };

  return (
    <div className="space-y-3 text-sm">
      {/* Health Status Band */}
      <motion.div
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        className={`p-3 rounded-lg border ${
          health.is_healthy
            ? 'bg-green-500/10 border-green-500/30'
            : health.is_degraded
            ? 'bg-yellow-500/10 border-yellow-500/30'
            : 'bg-red-500/10 border-red-500/30'
        }`}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div
              className={`w-2 h-2 rounded-full ${
                health.is_healthy
                  ? 'bg-green-400'
                  : health.is_degraded
                  ? 'bg-yellow-400'
                  : 'bg-red-400'
              }`}
            />
            <span className="font-semibold">
              {health.is_healthy
                ? 'Cache Healthy'
                : health.is_degraded
                ? 'Degraded Mode'
                : 'Cache Issues'}
            </span>
          </div>
          <button
            onClick={() => setExpandedPanel(expandedPanel === 'health' ? null : 'health')}
            className="text-xs px-2 py-1 bg-white/5 hover:bg-white/10 rounded transition"
          >
            {expandedPanel === 'health' ? 'Hide' : 'Details'}
          </button>
        </div>

        <AnimatePresence>
          {expandedPanel === 'health' && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className="mt-3 pt-3 border-t border-white/10 space-y-2 text-xs"
            >
              <div className="grid grid-cols-2 gap-2">
                <div>Can Read: {health.can_read ? '✓' : '✗'}</div>
                <div>Can Write: {health.can_write ? '✓' : '✗'}</div>
                <div>Cache Fresh: {health.is_fresh ? '✓' : '✗'}</div>
                <div>Age: {health.age_seconds ?? 'N/A'}s</div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>

      {/* Metrics Card */}
      <motion.div
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="p-3 bg-white/5 border border-white/10 rounded-lg"
      >
        <button
          onClick={() => setExpandedPanel(expandedPanel === 'metrics' ? null : 'metrics')}
          className="w-full text-left flex items-center justify-between group"
        >
          <div className="flex items-center gap-2">
            <TrendingUp className="w-4 h-4 text-cyan-400" />
            <span className="font-semibold">Performance Metrics</span>
          </div>
          <span className={`text-xs px-2 py-1 rounded ${getHitRatioColor(metrics.hit_ratio)}`}>
            {(metrics.hit_ratio * 100).toFixed(1)}% hit ratio
          </span>
        </button>

        <AnimatePresence>
          {expandedPanel === 'metrics' && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className="mt-3 pt-3 border-t border-white/10 space-y-2 text-xs"
            >
              <div className="grid grid-cols-2 gap-2">
                <div>Cache Hits: {metrics.hits}</div>
                <div>Cache Misses: {metrics.misses}</div>
                <div>Saves: {metrics.saves}</div>
                <div>Invalidations: {metrics.invalidations}</div>
                <div>Validation Errors: {metrics.validation_errors}</div>
                <div>Lock Recoveries: {metrics.poisoned_lock_recoveries}</div>
              </div>
              <div className="pt-2 border-t border-white/10 space-y-1">
                <div>Avg Read: {metrics.avg_read_time_ms.toFixed(2)}ms</div>
                <div>Avg Write: {metrics.avg_write_time_ms.toFixed(2)}ms</div>
                <div>Last Save: {metrics.last_save_duration_ms}ms</div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>

      {/* Actions Card */}
      {(metrics.poisoned_lock_recoveries > 0 || metrics.is_degraded) && (
        <motion.div
          initial={{ opacity: 0, y: -10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="p-3 bg-warning-500/10 border border-warning-500/30 rounded-lg"
        >
          <button
            onClick={() => setExpandedPanel(expandedPanel === 'actions' ? null : 'actions')}
            className="w-full text-left flex items-center justify-between"
          >
            <div className="flex items-center gap-2">
              <AlertTriangle className="w-4 h-4 text-warning-400" />
              <span className="font-semibold">Production Issues Detected</span>
            </div>
            <span className="text-xs">
              {metrics.poisoned_lock_recoveries} lock recoveries
            </span>
          </button>

          <AnimatePresence>
            {expandedPanel === 'actions' && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
                className="mt-3 pt-3 border-t border-white/10 space-y-2"
              >
                <button
                  onClick={handleRecovery}
                  disabled={cacheHook.isLoading}
                  className="w-full px-3 py-2 bg-yellow-500/20 hover:bg-yellow-500/30 text-yellow-400 rounded transition text-xs font-semibold flex items-center justify-center gap-2 disabled:opacity-50"
                >
                  <Zap className="w-3 h-3" />
                  Recover from Fallback
                </button>
                <button
                  onClick={handleForceRefresh}
                  disabled={cacheHook.isLoading}
                  className="w-full px-3 py-2 bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-400 rounded transition text-xs font-semibold flex items-center justify-center gap-2 disabled:opacity-50"
                >
                  <RotateCw className="w-3 h-3" />
                  Force Cache Refresh
                </button>
                <p className="text-xs text-gray-400 mt-2">
                  Use these options if cache is corrupted or in degraded state.
                </p>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      )}

      {/* Toggle Button */}
      <button
        onClick={() => setIsMonitoring(!isMonitoring)}
        className="w-full text-xs px-2 py-1 bg-white/5 hover:bg-white/10 rounded transition text-gray-400"
      >
        {isMonitoring ? 'Stop Monitoring' : 'Resume Monitoring'}
      </button>
    </div>
  );
};

export default SettingsCacheMonitor;
