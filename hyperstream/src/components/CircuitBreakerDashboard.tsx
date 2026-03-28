import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { AlertTriangle, CheckCircle2, RefreshCw, Activity, TrendingDown, Shield } from 'lucide-react';
import { motion } from 'framer-motion';

interface MirrorHealthReport {
  mirror_host: string;
  state: 'Closed' | 'Open' | 'HalfOpen';
  is_healthy: boolean;
  success_rate_percent: number;
  failure_count: number;
  success_count: number;
  health_score: number;
  last_failure_time: number | null;
  last_success_time: number | null;
}

interface FailoverMetrics {
  total_mirrors: number;
  healthy_mirrors: number;
  average_success_rate: number;
  mirror_details: MirrorHealthReport[];
}

interface CircuitBreakerDashboardProps {
  isOpen: boolean;
  onClose: () => void;
}

export const CircuitBreakerDashboard: React.FC<CircuitBreakerDashboardProps> = ({
  isOpen,
  onClose,
}) => {
  const [metrics, setMetrics] = useState<FailoverMetrics | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadMetrics = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await invoke<FailoverMetrics>('get_failover_metrics');
      setMetrics(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load circuit breaker metrics');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      loadMetrics();
      const interval = setInterval(loadMetrics, 5000); // Refresh every 5 seconds
      return () => clearInterval(interval);
    }
  }, [isOpen]);

  useEffect(() => {
    const subscription = listen('circuit_breaker_health', (event: any) => {
      if (event.payload && event.payload.mirrors_health) {
        const healthReports = event.payload.mirrors_health;
        // Reconstruct metrics from event
        const reconstructed: FailoverMetrics = {
          total_mirrors: healthReports.length,
          healthy_mirrors: healthReports.filter((m: MirrorHealthReport) => m.is_healthy).length,
          average_success_rate: healthReports.length > 0
            ? healthReports.reduce((sum: number, m: MirrorHealthReport) => sum + m.success_rate_percent, 0) / healthReports.length
            : 0,
          mirror_details: healthReports,
        };
        setMetrics(reconstructed);
      }
    });

    return () => {
      subscription.then(unsub => unsub());
    };
  }, []);

  const resetMirror = async (mirror: string) => {
    try {
      await invoke('reset_mirror_circuit_breaker', { mirror });
      // Reload metrics after reset
      await loadMetrics();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to reset mirror');
    }
  };

  if (!isOpen) return null;

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center"
      onClick={onClose}
    >
      <motion.div
        initial={{ scale: 0.95 }}
        animate={{ scale: 1 }}
        exit={{ scale: 0.95 }}
        onClick={(e) => e.stopPropagation()}
        className="bg-gradient-to-br from-cyan-900/20 to-blue-900/20 border border-cyan-500/30 rounded-xl p-6 max-w-4xl max-h-96 overflow-y-auto shadow-2xl"
      >
        <div className="flex items-center justify-between mb-6">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-cyan-400" />
            <h2 className="text-2xl font-bold text-white">Circuit Breaker Dashboard</h2>
          </div>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-white p-2 rounded-lg hover:bg-white/10"
          >
            ✕
          </button>
        </div>

        {/* Summary Stats */}
        {metrics && (
          <div className="grid grid-cols-3 gap-4 mb-6">
            <div className="bg-emerald-500/10 border border-emerald-500/30 rounded-lg p-4">
              <div className="text-emerald-400 text-sm font-semibold mb-1">Healthy Mirrors</div>
              <div className="text-2xl font-bold text-white">
                {metrics.healthy_mirrors}/{metrics.total_mirrors}
              </div>
            </div>

            <div className="bg-yellow-500/10 border border-yellow-500/30 rounded-lg p-4">
              <div className="text-yellow-400 text-sm font-semibold mb-1">Avg Success Rate</div>
              <div className="text-2xl font-bold text-white">
                {metrics.average_success_rate.toFixed(1)}%
              </div>
            </div>

            <div className="bg-blue-500/10 border border-blue-500/30 rounded-lg p-4">
              <div className="text-blue-400 text-sm font-semibold mb-1">Total Mirrors</div>
              <div className="text-2xl font-bold text-white">{metrics.total_mirrors}</div>
            </div>
          </div>
        )}

        {/* Error Display */}
        {error && (
          <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-4 mb-6 flex items-start gap-3">
            <AlertTriangle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
            <div>
              <div className="text-red-400 font-semibold">Error</div>
              <div className="text-red-300 text-sm">{error}</div>
            </div>
          </div>
        )}

        {/* Mirror Details */}
        {metrics && metrics.mirror_details.length > 0 ? (
          <div className="space-y-3">
            <h3 className="text-sm font-semibold text-cyan-300 uppercase tracking-wider mb-3">
              Mirror Status
            </h3>
            {metrics.mirror_details.map((mirror) => (
              <motion.div
                key={mirror.mirror_host}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                className={`p-3 rounded-lg border transition-all ${
                  mirror.is_healthy
                    ? 'bg-emerald-500/5 border-emerald-500/30'
                    : 'bg-red-500/5 border-red-500/30'
                }`}
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      {mirror.state === 'Open' ? (
                        <AlertTriangle className="w-4 h-4 text-red-400" />
                      ) : mirror.state === 'HalfOpen' ? (
                        <Activity className="w-4 h-4 text-yellow-400" />
                      ) : (
                        <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                      )}
                      <span className="font-mono text-sm font-semibold text-white">
                        {mirror.mirror_host}
                      </span>
                      <span className={`text-xs font-bold px-2 py-1 rounded ${
                        mirror.state === 'Closed'
                          ? 'bg-emerald-500/30 text-emerald-300'
                          : mirror.state === 'Open'
                          ? 'bg-red-500/30 text-red-300'
                          : 'bg-yellow-500/30 text-yellow-300'
                      }`}>
                        {mirror.state}
                      </span>
                    </div>
                    <div className="text-xs text-gray-400 ml-6 space-y-1">
                      <div>Success Rate: <span className="text-cyan-300 font-mono">{mirror.success_rate_percent.toFixed(1)}%</span></div>
                      <div>Successes: <span className="text-emerald-300 font-mono">{mirror.success_count}</span> | Failures: <span className="text-red-300 font-mono">{mirror.failure_count}</span></div>
                      <div>Health Score: <span className="text-blue-300 font-mono">{mirror.health_score.toFixed(2)}</span></div>
                    </div>
                  </div>
                  <button
                    onClick={() => resetMirror(mirror.mirror_host)}
                    className="ml-2 p-2 rounded-lg bg-blue-500/20 hover:bg-blue-500/40 text-blue-400 hover:text-blue-300 transition-colors"
                    title="Reset this mirror's circuit breaker"
                  >
                    <RefreshCw className="w-4 h-4" />
                  </button>
                </div>
              </motion.div>
            ))}
          </div>
        ) : loading ? (
          <div className="flex items-center justify-center py-8">
            <Activity className="w-6 h-6 text-cyan-400 animate-spin" />
            <span className="ml-2 text-gray-400">Loading metrics...</span>
          </div>
        ) : (
          <div className="text-center py-8 text-gray-400">
            <TrendingDown className="w-8 h-8 mx-auto mb-2 opacity-50" />
            No mirror data available
          </div>
        )}

        {/* Refresh Button */}
        <button
          onClick={loadMetrics}
          disabled={loading}
          className="mt-6 w-full px-4 py-2 bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 rounded-lg font-semibold transition-colors disabled:opacity-50"
        >
          {loading ? 'Refreshing...' : 'Refresh Now'}
        </button>
      </motion.div>
    </motion.div>
  );
};
