import React, { useState, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { ChevronDown, RefreshCw } from 'lucide-react';
import { useMirrorMetrics } from '../hooks/useMirrorScoring';
import { MirrorScore } from '../types';

/**
 * Real-time mirror scoring dashboard component
 * Displays mirror health metrics with auto-refresh and expandable details
 */
export const MirrorScoringDashboard: React.FC = () => {
  const { metrics, autoRefresh, setAutoRefresh, loading, fetchMetrics } = useMirrorMetrics();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  /**
   * Calculate summary statistics from metrics
   */
  const stats = useMemo(() => {
    if (!metrics || metrics.length === 0) {
      return { totalMirrors: 0, healthy: 0, atRisk: 0 };
    }

    const healthy = metrics.filter((m) => m.risk_level === 'healthy').length;
    const atRisk = metrics.filter((m) =>
      ['caution', 'warning', 'critical'].includes(m.risk_level)
    ).length;

    return {
      totalMirrors: metrics.length,
      healthy,
      atRisk,
    };
  }, [metrics]);

  /**
   * Handle mirror expansion toggle
   */
  const toggleExpanded = (url: string) => {
    const newExpanded = new Set(expanded);
    if (newExpanded.has(url)) {
      newExpanded.delete(url);
    } else {
      newExpanded.add(url);
    }
    setExpanded(newExpanded);
  };

  /**
   * Get color classes based on risk level
   */
  const getRiskColors = (
    riskLevel: 'healthy' | 'caution' | 'warning' | 'critical'
  ): { bg: string; border: string; text: string; badge: string } => {
    switch (riskLevel) {
      case 'healthy':
        return {
          bg: 'bg-emerald-500/5',
          border: 'border-emerald-500/30',
          text: 'text-emerald-400',
          badge: 'bg-emerald-500/20 text-emerald-300',
        };
      case 'caution':
        return {
          bg: 'bg-amber-500/5',
          border: 'border-amber-500/30',
          text: 'text-amber-400',
          badge: 'bg-amber-500/20 text-amber-300',
        };
      case 'warning':
        return {
          bg: 'bg-orange-500/5',
          border: 'border-orange-500/30',
          text: 'text-orange-400',
          badge: 'bg-orange-500/20 text-orange-300',
        };
      case 'critical':
        return {
          bg: 'bg-rose-500/5',
          border: 'border-rose-500/30',
          text: 'text-rose-400',
          badge: 'bg-rose-500/20 text-rose-300',
        };
    }
  };

  /**
   * Render stat card
   */
  const StatCard = ({
    title,
    value,
    color,
  }: {
    title: string;
    value: number;
    color: 'emerald' | 'blue' | 'orange';
  }) => {
    const colorMap = {
      emerald: { bg: 'bg-emerald-500/10', border: 'border-emerald-500/30', text: 'text-emerald-400' },
      blue: { bg: 'bg-cyan-500/10', border: 'border-cyan-500/30', text: 'text-cyan-400' },
      orange: { bg: 'bg-orange-500/10', border: 'border-orange-500/30', text: 'text-orange-400' },
    };

    return (
      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        className={`${colorMap[color].bg} border ${colorMap[color].border} rounded-lg p-4 backdrop-blur-xl`}
      >
        <div className="text-sm text-gray-400 mb-2">{title}</div>
        <div className={`text-3xl font-bold ${colorMap[color].text}`}>{value}</div>
      </motion.div>
    );
  };

  /**
   * Render mirror list item
   */
  const MirrorListItem = ({ mirror }: { mirror: MirrorScore }) => {
    const isExpanded = expanded.has(mirror.url);
    const colors = getRiskColors(mirror.risk_level);

    return (
      <motion.div
        key={mirror.url}
        initial={{ opacity: 0, x: -20 }}
        animate={{ opacity: 1, x: 0 }}
        exit={{ opacity: 0, x: 20 }}
        transition={{ duration: 0.2 }}
        className={`${colors.bg} border ${colors.border} rounded-lg overflow-hidden transition-all`}
      >
        {/* Header */}
        <button
          onClick={() => toggleExpanded(mirror.url)}
          className="w-full px-4 py-3 flex items-center justify-between hover:bg-white/5 transition-colors"
        >
          <div className="flex-1 text-left min-w-0">
            <div className="text-sm font-medium text-gray-200 truncate">{mirror.url}</div>
            <div className="text-xs text-gray-500 mt-1">
              Reliability: {mirror.reliability_score.toFixed(1)}% | Speed: {mirror.speed_score.toFixed(1)}%
            </div>
          </div>
          <div className="flex items-center gap-3 ml-4">
            <span className={`px-2 py-1 rounded text-xs font-medium ${colors.badge}`}>
              {mirror.risk_level}
            </span>
            <motion.div
              animate={{ rotate: isExpanded ? 180 : 0 }}
              transition={{ duration: 0.2 }}
              className={colors.text}
            >
              <ChevronDown size={18} />
            </motion.div>
          </div>
        </button>

        {/* Expandable Details */}
        <AnimatePresence>
          {isExpanded && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.2 }}
              className="border-t border-white/10 bg-white/2 px-4 py-3 space-y-2"
            >
              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <div className="text-gray-500 text-xs uppercase tracking-wide">Reliability Score</div>
                  <div className={`text-lg font-semibold ${colors.text} mt-1`}>
                    {mirror.reliability_score.toFixed(1)}%
                  </div>
                  <div className="w-full bg-white/10 rounded-full h-2 mt-2 overflow-hidden">
                    <motion.div
                      className={`h-full bg-gradient-to-r from-cyan-400 to-blue-500`}
                      initial={{ width: 0 }}
                      animate={{ width: `${mirror.reliability_score}%` }}
                      transition={{ duration: 0.5, delay: 0.1 }}
                    />
                  </div>
                </div>

                <div>
                  <div className="text-gray-500 text-xs uppercase tracking-wide">Speed Score</div>
                  <div className={`text-lg font-semibold ${colors.text} mt-1`}>
                    {mirror.speed_score.toFixed(1)}%
                  </div>
                  <div className="w-full bg-white/10 rounded-full h-2 mt-2 overflow-hidden">
                    <motion.div
                      className={`h-full bg-gradient-to-r from-purple-400 to-pink-500`}
                      initial={{ width: 0 }}
                      animate={{ width: `${mirror.speed_score}%` }}
                      transition={{ duration: 0.5, delay: 0.1 }}
                    />
                  </div>
                </div>
              </div>

              <div className="pt-2 border-t border-white/5">
                <div className="text-gray-500 text-xs uppercase tracking-wide mb-2">Uptime</div>
                <div className={`text-lg font-semibold ${colors.text}`}>
                  {mirror.uptime_percentage.toFixed(1)}%
                </div>
                <div className="w-full bg-white/10 rounded-full h-2 mt-2 overflow-hidden">
                  <motion.div
                    className={`h-full bg-gradient-to-r from-green-400 to-emerald-500`}
                    initial={{ width: 0 }}
                    animate={{ width: `${mirror.uptime_percentage}%` }}
                    transition={{ duration: 0.5, delay: 0.1 }}
                  />
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    );
  };

  return (
    <div className="w-full h-full bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-6 overflow-y-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-white">Mirror Scoring Dashboard</h1>
          <p className="text-sm text-gray-400 mt-1">Real-time mirror health and reliability metrics</p>
        </div>
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={() => setAutoRefresh(!autoRefresh)}
          className={`p-2 rounded-lg transition-all ${
            autoRefresh
              ? 'bg-cyan-500/20 border border-cyan-500/50 text-cyan-400'
              : 'bg-gray-500/20 border border-gray-500/50 text-gray-400'
          }`}
          title={autoRefresh ? 'Auto-refresh enabled' : 'Auto-refresh disabled'}
        >
          <motion.div animate={{ rotate: autoRefresh ? 360 : 0 }} transition={{ duration: 2, repeat: Infinity }}>
            <RefreshCw size={20} />
          </motion.div>
        </motion.button>
      </div>

      {/* Summary Stats */}
      <div className="grid grid-cols-3 gap-4 mb-6">
        <StatCard title="Total Mirrors" value={stats.totalMirrors} color="blue" />
        <StatCard title="Healthy Mirrors" value={stats.healthy} color="emerald" />
        <StatCard title="At Risk" value={stats.atRisk} color="orange" />
      </div>

      {/* Loading State */}
      {loading && (
        <div className="text-center py-8">
          <div className="inline-flex items-center gap-2 text-cyan-400">
            <motion.div animate={{ rotate: 360 }} transition={{ duration: 1, repeat: Infinity }}>
              <RefreshCw size={20} />
            </motion.div>
            <span>Loading metrics...</span>
          </div>
        </div>
      )}

      {/* Empty State */}
      {!loading && metrics.length === 0 && (
        <div className="text-center py-12">
          <div className="text-gray-400 mb-2">No mirrors available</div>
          <p className="text-sm text-gray-500">Mirror metrics will appear here when mirrors are discovered</p>
        </div>
      )}

      {/* Mirror List */}
      <AnimatePresence mode="popLayout">
        <div className="space-y-3">
          {metrics.map((mirror) => (
            <MirrorListItem key={mirror.url} mirror={mirror} />
          ))}
        </div>
      </AnimatePresence>
    </div>
  );
};

export default MirrorScoringDashboard;
