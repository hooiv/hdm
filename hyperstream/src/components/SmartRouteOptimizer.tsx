import React, { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { AlertCircle, Activity, Zap, TrendingUp, Globe, Shield, Clock } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import '../App.css';

export interface RouteStatus {
  download_id: string;
  current_mirror: string;
  mirrors_in_use: Array<{
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
  }>;
  total_bandwidth_bps: number;
  primary_bandwidth_bps: number;
  secondary_bandwidth_bps: number;
  failover_count: number;
  predicted_completion_secs: number;
  risk_assessment: {
    level: string;
    confidence_percent: number;
    factors: Array<{
      name: string;
      severity: string;
      description: string;
    }>;
    recommended_action: string;
  };
  last_mirror_switch_reason?: string;
  last_mirror_switch_ms?: number;
  is_pooling_bandwidth: boolean;
}

interface SmartRouteOptimizerProps {
  downloadId: string;
  isVisible: boolean;
}

/**
 * Smart Route Optimizer Dashboard
 * 
 * Real-time visualization of intelligent mirror selection, bandwidth pooling,
 * and proactive failover decisions. Shows:
 * - Mirror health heatmap
 * - Bandwidth allocation across mirrors
 * - Failure risk predictions
 * - Route decision history
 */
export const SmartRouteOptimizer: React.FC<SmartRouteOptimizerProps> = ({
  downloadId,
  isVisible,
}) => {
  const [routeStatus, setRouteStatus] = useState<RouteStatus | null>(null);
  const [history, setHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);

  // Format bandwidth for display
  const formatBandwidth = (bps: number): string => {
    if (bps >= 1_000_000_000) return `${(bps / 1_000_000_000).toFixed(2)} GB/s`;
    if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} MB/s`;
    if (bps >= 1_000) return `${(bps / 1_000).toFixed(2)} KB/s`;
    return `${bps} B/s`;
  };

  // Fetch route status
  const fetchRouteStatus = useCallback(async () => {
    if (!isVisible || !downloadId) return;

    setLoading(true);
    try {
      const status = await invoke<RouteStatus | null>('get_route_status', {
        download_id: downloadId,
      });
      setRouteStatus(status);
      setError(null);

      // Also fetch recent history
      const historyData = await invoke<any[]>('get_route_decision_history', {
        download_id: downloadId,
        limit: 10,
      });
      setHistory(historyData);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [downloadId, isVisible]);

  // Auto-refresh on interval
  useEffect(() => {
    if (!autoRefresh || !isVisible) return;

    fetchRouteStatus();
    const interval = setInterval(fetchRouteStatus, 3000);
    return () => clearInterval(interval);
  }, [autoRefresh, isVisible, fetchRouteStatus]);

  if (!isVisible || !routeStatus) {
    return null;
  }

  const getRiskColor = (level: string) => {
    switch (level.toLowerCase()) {
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
  };

  const getRiskBgColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'healthy':
      case 'safe':
        return 'bg-green-900/20 border-green-500/30';
      case 'caution':
        return 'bg-yellow-900/20 border-yellow-500/30';
      case 'warning':
        return 'bg-orange-900/20 border-orange-500/30';
      case 'critical':
        return 'bg-red-900/20 border-red-500/30';
      default:
        return 'bg-cyan-900/20 border-cyan-500/30';
    }
  };

  const primaryMirror = routeStatus.mirrors_in_use[0];
  const secondaryMirrors = routeStatus.mirrors_in_use.slice(1);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: 20 }}
      className="p-6 bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 rounded-xl border border-cyan-500/20 shadow-2xl"
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Globe className="w-6 h-6 text-cyan-400" />
          <div>
            <h3 className="text-lg font-bold text-white">Smart Route Optimizer</h3>
            <p className="text-sm text-slate-400">Intelligent mirror selection & bandwidth pooling</p>
          </div>
        </div>
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={() => setAutoRefresh(!autoRefresh)}
          className={`px-4 py-2 rounded-lg font-mono text-sm ${
            autoRefresh
              ? 'bg-cyan-500/20 border-cyan-500/50 text-cyan-400'
              : 'bg-slate-700/50 border-slate-600/50 text-slate-400'
          } border transition-colors`}
        >
          {autoRefresh ? '⟳ Live' : 'Paused'}
        </motion.button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
        {/* Primary Mirror Status */}
        {primaryMirror && (
          <motion.div
            className={`p-4 rounded-lg border ${getRiskBgColor(primaryMirror.risk_level)}`}
            whileHover={{ scale: 1.02 }}
          >
            <div className="flex items-start justify-between mb-3">
              <div>
                <p className="text-xs text-slate-400 uppercase tracking-wide">Primary Mirror</p>
                <p className="text-sm font-mono text-cyan-300 break-all">
                  {primaryMirror.url.replace('https://', '').split('/')[0]}
                </p>
              </div>
              <span className={`px-2 py-1 rounded text-xs font-bold ${getRiskColor(primaryMirror.risk_level)}`}>
                {primaryMirror.risk_level}
              </span>
            </div>

            <div className="grid grid-cols-2 gap-3 text-xs">
              <div className="bg-slate-900/50 rounded p-2">
                <p className="text-slate-400">Reliability</p>
                <p className="text-cyan-300 font-bold">{primaryMirror.reliability_score.toFixed(1)}%</p>
              </div>
              <div className="bg-slate-900/50 rounded p-2">
                <p className="text-slate-400">Speed</p>
                <p className="text-cyan-300 font-bold">{primaryMirror.speed_score.toFixed(1)}%</p>
              </div>
              <div className="bg-slate-900/50 rounded p-2">
                <p className="text-slate-400">Current Speed</p>
                <p className="text-green-400 font-bold">{formatBandwidth(primaryMirror.last_segment_speed_bps)}</p>
              </div>
              <div className="bg-slate-900/50 rounded p-2">
                <p className="text-slate-400">Latency</p>
                <p className="text-cyan-300 font-bold">{primaryMirror.avg_latency_ms.toFixed(0)}ms</p>
              </div>
            </div>
          </motion.div>
        )}

        {/* Bandwidth & Pooling Status */}
        <motion.div
          className="p-4 rounded-lg border bg-cyan-900/10 border-cyan-500/30"
          whileHover={{ scale: 1.02 }}
        >
          <div className="flex items-start justify-between mb-3">
            <div>
              <p className="text-xs text-slate-400 uppercase tracking-wide">Bandwidth Status</p>
              <p className="text-lg font-bold text-green-400">{formatBandwidth(routeStatus.total_bandwidth_bps)}</p>
            </div>
            {routeStatus.is_pooling_bandwidth && (
              <span className="px-2 py-1 rounded text-xs font-bold bg-green-500/20 text-green-400 border border-green-500/30">
                Pooling ({secondaryMirrors.length} mirrors)
              </span>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3 text-xs">
            <div className="bg-slate-900/50 rounded p-2">
              <p className="text-slate-400">Primary</p>
              <p className="text-cyan-300 font-bold">{formatBandwidth(routeStatus.primary_bandwidth_bps)}</p>
            </div>
            {routeStatus.is_pooling_bandwidth && (
              <div className="bg-slate-900/50 rounded p-2">
                <p className="text-slate-400">Secondary</p>
                <p className="text-green-300 font-bold">{formatBandwidth(routeStatus.secondary_bandwidth_bps)}</p>
              </div>
            )}
            <div className="bg-slate-900/50 rounded p-2">
              <p className="text-slate-400">ETA</p>
              <p className="text-cyan-300 font-bold">
                {routeStatus.predicted_completion_secs < 3600
                  ? `${Math.round(routeStatus.predicted_completion_secs / 60)}m`
                  : `${(routeStatus.predicted_completion_secs / 3600).toFixed(1)}h`}
              </p>
            </div>
            <div className="bg-slate-900/50 rounded p-2">
              <p className="text-slate-400">Failovers</p>
              <p className="text-cyan-300 font-bold">{routeStatus.failover_count}</p>
            </div>
          </div>
        </motion.div>
      </div>

      {/* Risk Assessment */}
      <motion.div
        className={`p-4 rounded-lg border mb-6 ${getRiskBgColor(routeStatus.risk_assessment.level)}`}
        whileHover={{ scale: 1.01 }}
      >
        <div className="flex items-start gap-3 mb-3">
          <Shield className={`w-5 h-5 flex-shrink-0 ${getRiskColor(routeStatus.risk_assessment.level)}`} />
          <div className="flex-1">
            <div className="flex items-center justify-between mb-1">
              <h4 className="font-semibold text-white">Risk Assessment</h4>
              <span className="text-xs font-mono text-slate-400">
                {routeStatus.risk_assessment.confidence_percent}% confidence
              </span>
            </div>
            <p className="text-sm text-slate-200">{routeStatus.risk_assessment.recommended_action}</p>
          </div>
        </div>

        {routeStatus.risk_assessment.factors.length > 0 && (
          <div className="mt-3 pt-3 border-t border-slate-700/50">
            <p className="text-xs text-slate-400 uppercase tracking-wide mb-2">Factors</p>
            <div className="flex flex-wrap gap-2">
              {routeStatus.risk_assessment.factors.map((factor, idx) => (
                <span
                  key={idx}
                  className="text-xs px-2 py-1 rounded bg-slate-900/50 border border-slate-600/30 text-slate-300"
                >
                  {factor.name}
                </span>
              ))}
            </div>
          </div>
        )}
      </motion.div>

      {/* Secondary Mirrors (if pooling) */}
      {secondaryMirrors.length > 0 && (
        <div className="mb-6">
          <h4 className="text-sm font-semibold text-cyan-400 mb-3 flex items-center gap-2">
            <Zap className="w-4 h-4" />
            Secondary Mirrors ({secondaryMirrors.length})
          </h4>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {secondaryMirrors.map((mirror, idx) => (
              <motion.div
                key={idx}
                className={`p-3 rounded-lg border ${getRiskBgColor(mirror.risk_level)} text-sm`}
                initial={{ opacity: 0, x: -20 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ delay: idx * 0.1 }}
              >
                <p className="text-xs text-slate-400 mb-2 break-all">
                  {mirror.url.replace('https://', '').split('/')[0]}
                </p>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div>
                    <p className="text-slate-500">Score</p>
                    <p className="text-cyan-300 font-bold">{mirror.reliability_score.toFixed(0)}%</p>
                  </div>
                  <div>
                    <p className="text-slate-500">Speed</p>
                    <p className="text-green-300 text-xs">
                      {formatBandwidth(mirror.last_segment_speed_bps)}
                    </p>
                  </div>
                  <div>
                    <p className="text-slate-500">Uptime</p>
                    <p className="text-cyan-300 font-bold">{mirror.uptime_percent.toFixed(0)}%</p>
                  </div>
                </div>
              </motion.div>
            ))}
          </div>
        </div>
      )}

      {/* Decision History */}
      {history.length > 0 && (
        <div>
          <h4 className="text-sm font-semibold text-cyan-400 mb-3 flex items-center gap-2">
            <Clock className="w-4 h-4" />
            Recent Decisions
          </h4>
          <div className="space-y-2 max-h-48 overflow-y-auto">
            {history.map((entry, idx) => (
              <motion.div
                key={idx}
                className="p-3 rounded-lg bg-slate-900/30 border border-slate-700/30 text-xs"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: idx * 0.05 }}
              >
                <div className="flex items-start justify-between mb-1">
                  <span className="font-mono text-cyan-400">{entry.action}</span>
                  <span className="text-slate-500 text-xs">
                    {new Date(entry.timestamp_ms).toLocaleTimeString()}
                  </span>
                </div>
                <p className="text-slate-400">{entry.reason}</p>
              </motion.div>
            ))}
          </div>
        </div>
      )}

      {/* Error State */}
      {error && (
        <motion.div
          className="p-3 rounded-lg bg-red-900/20 border border-red-500/30 flex items-start gap-3"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
        >
          <AlertCircle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-sm font-semibold text-red-300">Error loading route status</p>
            <p className="text-xs text-red-400/80">{error}</p>
          </div>
        </motion.div>
      )}
    </motion.div>
  );
};

export default SmartRouteOptimizer;
