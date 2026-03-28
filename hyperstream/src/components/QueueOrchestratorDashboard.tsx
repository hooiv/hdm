/**
 * Queue Orchestrator Dashboard
 *
 * Production-grade queue intelligence dashboard showing:
 * - Real-time queue orchestration state
 * - Intelligent bandwidth allocation
 * - ETC (Estimated Time to Completion) prediction
 * - Performance bottleneck detection
 * - Smart recommendations
 * - Speed trends per download
 *
 * This is what makes HyperStream's download queue better than any competitor.
 */

import React, { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Activity,
  TrendingDown,
  TrendingUp,
  AlertTriangle,
  Zap,
  Clock,
  Gauge,
  Lightbulb,
  ChevronDown,
  ChevronUp,
  BarChart3,
} from 'lucide-react';

interface DownloadMetrics {
  id: string;
  url: string;
  bytes_downloaded: number;
  total_bytes: number;
  current_speed_bps: number;
  average_speed_bps: number;
  elapsed_ms: number;
  estimated_remaining_ms: number;
  allocated_bandwidth_bps: number;
  priority: number;
  is_blocked: boolean;
}

interface QueueOrchestrationState {
  total_active_downloads: number;
  total_queued_downloads: number;
  global_bandwidth_available_bps: number;
  global_bandwidth_used_bps: number;
  estimated_queue_completion_ms: number;
  queue_efficiency: number;
  conflict_count: number;
  downloads: DownloadMetrics[];
}

interface QueueAnalysis {
  state: QueueOrchestrationState;
  bottlenecks: string[];
  recommendations: string[];
  estimated_completion_time_ms: number;
  critical_warnings: number;
}

const formatBytes = (bytes: number): string => {
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = bytes;
  let unitIdx = 0;

  while (size >= 1024 && unitIdx < units.length - 1) {
    size /= 1024;
    unitIdx += 1;
  }

  return `${size.toFixed(2)} ${units[unitIdx]}`;
};

const formatBytesPerSecond = (bps: number): string => {
  return `${formatBytes(bps)}/s`;
};

const formatDuration = (ms: number): string => {
  if (ms < 1000) return '<1s';
  if (ms < 60000) return `${(ms / 1000).toFixed(0)}s`;
  if (ms < 3600000) return `${(ms / 60000).toFixed(0)}m`;
  return `${(ms / 3600000).toFixed(1)}h`;
};

const getPriorityLabel = (priority: number): string => {
  switch (priority) {
    case 0:
      return 'Low';
    case 1:
      return 'Normal';
    case 2:
      return 'High';
    default:
      return '?';
  }
};

const getPriorityColor = (priority: number): string => {
  switch (priority) {
    case 0:
      return 'text-blue-400';
    case 1:
      return 'text-amber-400';
    case 2:
      return 'text-red-400';
    default:
      return 'text-slate-400';
  }
};

const QueueOrchestratorDashboard: React.FC = () => {
  const [state, setState] = useState<QueueOrchestrationState | null>(null);
  const [analysis, setAnalysis] = useState<QueueAnalysis | null>(null);
  const [loading, setLoading] = useState(true);
  const [expandedDownloads, setExpandedDownloads] = useState<Set<string>>(new Set());
  const [speedTrends, setSpeedTrends] = useState<Record<string, string>>({});

  // Fetch orchestration state periodically
  useEffect(() => {
    const fetchState = async () => {
      try {
        const orchestrationState = await invoke<QueueOrchestrationState>(
          'get_queue_orchestration_state'
        );
        setState(orchestrationState);

        // Fetch analysis if we have downloads
        if (orchestrationState.downloads.length > 0) {
          const queueAnalysis = await invoke<QueueAnalysis>('analyze_queue_health', {
            total_queued: orchestrationState.total_queued_downloads,
            total_active: orchestrationState.total_active_downloads,
            global_limit: 5,
          });
          setAnalysis(queueAnalysis);
        }

        // Fetch speed trends for each download
        const trends: Record<string, string> = {};
        for (const download of orchestrationState.downloads) {
          try {
            const trend = await invoke<string>('get_download_speed_trend', { id: download.id });
            trends[download.id] = trend;
          } catch (e) {
            trends[download.id] = 'No data';
          }
        }
        setSpeedTrends(trends);

        setLoading(false);
      } catch (error) {
        console.error('Failed to fetch queue state:', error);
      }
    };

    fetchState();
    const interval = setInterval(fetchState, 500); // Update every 500ms for real-time feel
    return () => clearInterval(interval);
  }, []);

  const toggleDownloadExpanded = useCallback((id: string) => {
    setExpandedDownloads((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  if (loading || !state) {
    return (
      <div className="flex items-center justify-center h-96 text-slate-400">
        <Activity className="animate-spin mr-2" size={20} />
        Loading queue orchestration...
      </div>
    );
  }

  const efficiencyColor =
    state.queue_efficiency > 0.8
      ? 'text-emerald-400'
      : state.queue_efficiency > 0.5
        ? 'text-amber-400'
        : 'text-red-400';

  return (
    <div className="space-y-6 pb-8">
      {/* Header */}
      <div className="flex items-center gap-3">
        <BarChart3 size={24} className="text-cyan-400" />
        <div>
          <h2 className="text-2xl font-bold text-white">Queue Orchestrator</h2>
          <p className="text-xs text-slate-400">Intelligent download scheduling with smart bandwidth allocation</p>
        </div>
      </div>

      {/* Critical Warnings */}
      {analysis && analysis.critical_warnings > 0 && (
        <motion.div
          initial={{ opacity: 0, y: -10 }}
          animate={{ opacity: 1, y: 0 }}
          className="p-4 rounded-lg bg-red-500/10 border border-red-500/20"
        >
          <div className="flex items-start gap-3">
            <AlertTriangle size={20} className="text-red-400 mt-0.5 flex-shrink-0" />
            <div>
              <p className="font-semibold text-red-400">{analysis.critical_warnings} Critical Warning(s)</p>
              <ul className="mt-2 space-y-1 text-sm text-slate-300">
                {analysis.bottlenecks.map((bottleneck, idx) => (
                  <li key={idx} className="flex items-center gap-2">
                    <span className="w-1 h-1 rounded-full bg-red-400" />
                    {bottleneck}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </motion.div>
      )}

      {/* Queue Metrics Grid */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {/* Active Downloads */}
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          className="p-4 rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur"
        >
          <div className="flex items-center justify-between mb-2">
            <p className="text-xs uppercase text-slate-400 font-semibold">Active</p>
            <Activity size={16} className="text-emerald-400" />
          </div>
          <p className="text-2xl font-bold text-white">{state.total_active_downloads}</p>
          <p className="text-xs text-slate-400 mt-1">downloading now</p>
        </motion.div>

        {/* Queued Downloads */}
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.05 }}
          className="p-4 rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur"
        >
          <div className="flex items-center justify-between mb-2">
            <p className="text-xs uppercase text-slate-400 font-semibold">Queued</p>
            <Clock size={16} className="text-blue-400" />
          </div>
          <p className="text-2xl font-bold text-white">{state.total_queued_downloads}</p>
          <p className="text-xs text-slate-400 mt-1">waiting to start</p>
        </motion.div>

        {/* Global Bandwidth Used */}
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.1 }}
          className="p-4 rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur"
        >
          <div className="flex items-center justify-between mb-2">
            <p className="text-xs uppercase text-slate-400 font-semibold">Speed</p>
            <Zap size={16} className="text-amber-400" />
          </div>
          <p className="text-2xl font-bold text-white">{formatBytesPerSecond(state.global_bandwidth_used_bps)}</p>
          <p className="text-xs text-slate-400 mt-1">current throughput</p>
        </motion.div>

        {/* Queue Efficiency */}
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.15 }}
          className="p-4 rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur"
        >
          <div className="flex items-center justify-between mb-2">
            <p className="text-xs uppercase text-slate-400 font-semibold">Efficiency</p>
            <Gauge size={16} className={efficiencyColor} />
          </div>
          <p className={`text-2xl font-bold ${efficiencyColor}`}>
            {(state.queue_efficiency * 100).toFixed(0)}%
          </p>
          <p className="text-xs text-slate-400 mt-1">resource utilization</p>
        </motion.div>
      </div>

      {/* ETC & Queue Status */}
      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="p-5 rounded-lg bg-gradient-to-br from-slate-800/50 to-slate-900/50 border border-slate-700/50 backdrop-blur"
      >
        <div className="flex items-center justify-between mb-4">
          <h3 className="font-semibold text-slate-200 flex items-center gap-2">
            <Clock size={18} className="text-cyan-400" />
            Queue Completion Estimate
          </h3>
          <span className="text-2xl font-bold text-cyan-400">
            {formatDuration(state.estimated_queue_completion_ms)}
          </span>
        </div>
        <div className="w-full h-2 rounded-full bg-slate-700/50">
          <motion.div
            className="h-full rounded-full bg-gradient-to-r from-cyan-500 to-blue-500"
            initial={{ width: 0 }}
            animate={{ width: `${Math.min((state.queue_efficiency * 100) / 100 * 100, 100)}%` }}
            transition={{ duration: 0.5 }}
          />
        </div>
        <p className="text-xs text-slate-400 mt-3">
          All downloads will complete in approximately {formatDuration(state.estimated_queue_completion_ms)}
        </p>
      </motion.div>

      {/* Smart Recommendations */}
      {analysis && analysis.recommendations.length > 0 && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.25 }}
          className="p-5 rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur"
        >
          <h3 className="font-semibold text-slate-200 flex items-center gap-2 mb-4">
            <Lightbulb size={18} className="text-yellow-400" />
            Smart Recommendations
          </h3>
          <ul className="space-y-3">
            {analysis.recommendations.map((rec, idx) => (
              <li key={idx} className="flex items-start gap-3 text-sm">
                <span className="text-yellow-400 font-bold mt-1">→</span>
                <span className="text-slate-300">{rec}</span>
              </li>
            ))}
          </ul>
        </motion.div>
      )}

      {/* Downloads List */}
      {state.downloads.length > 0 && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.3 }}
          className="space-y-3"
        >
          <h3 className="font-semibold text-slate-200">Active Downloads</h3>
          <div className="space-y-2">
            <AnimatePresence>
              {state.downloads.map((download) => (
                <motion.div
                  key={download.id}
                  initial={{ opacity: 0, x: -20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: 20 }}
                  className="rounded-lg bg-slate-800/50 border border-slate-700/50 backdrop-blur overflow-hidden"
                >
                  {/* Download Header */}
                  <button
                    onClick={() => toggleDownloadExpanded(download.id)}
                    className="w-full p-4 flex items-center justify-between hover:bg-slate-700/30 transition-colors text-left"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-3 mb-2">
                        <span
                          className={`px-2 py-1 rounded text-xs font-semibold ${
                            download.priority === 2
                              ? 'bg-red-500/20 text-red-400'
                              : download.priority === 1
                                ? 'bg-amber-500/20 text-amber-400'
                                : 'bg-blue-500/20 text-blue-400'
                          }`}
                        >
                          {getPriorityLabel(download.priority)}
                        </span>
                        {download.is_blocked && (
                          <span className="px-2 py-1 rounded text-xs font-semibold bg-slate-600/50 text-slate-300">
                            Blocked
                          </span>
                        )}
                        {speedTrends[download.id] && (
                          <span className="text-xs text-slate-400">{speedTrends[download.id]}</span>
                        )}
                      </div>
                      <p className="text-sm text-slate-300 truncate font-mono text-xs">{download.id}</p>
                    </div>
                    <div className="flex items-center gap-3 ml-4">
                      <div className="text-right">
                        <p className="text-sm font-semibold text-white">
                          {formatBytesPerSecond(download.current_speed_bps)}
                        </p>
                        <p className="text-xs text-slate-400">
                          {formatDuration(download.estimated_remaining_ms)} remaining
                        </p>
                      </div>
                      <motion.div animate={{ rotate: expandedDownloads.has(download.id) ? 180 : 0 }}>
                        <ChevronDown size={18} className="text-slate-400" />
                      </motion.div>
                    </div>
                  </button>

                  {/* Bandwidth Progress Bar */}
                  <div className="px-4 pb-3 space-y-2">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs text-slate-400">
                        {formatBytes(download.bytes_downloaded)} / {formatBytes(download.total_bytes)}
                      </span>
                      <span className="text-xs text-cyan-400 font-semibold">
                        Alloc: {formatBytesPerSecond(download.allocated_bandwidth_bps)}
                      </span>
                    </div>
                    <div className="w-full h-2 rounded-full bg-slate-700/50 overflow-hidden">
                      <motion.div
                        className="h-full bg-gradient-to-r from-cyan-500 via-blue-500 to-purple-500"
                        initial={{ width: 0 }}
                        animate={{
                          width: `${((download.bytes_downloaded / download.total_bytes) * 100).toFixed(1)}%`,
                        }}
                        transition={{ duration: 0.3 }}
                      />
                    </div>
                  </div>

                  {/* Expanded Details */}
                  <AnimatePresence>
                    {expandedDownloads.has(download.id) && (
                      <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: 'auto' }}
                        exit={{ opacity: 0, height: 0 }}
                        className="px-4 pb-4 pt-2 border-t border-slate-700/50 bg-slate-900/30 space-y-2 text-xs"
                      >
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <p className="text-slate-400">Average Speed</p>
                            <p className="text-slate-200 font-semibold">
                              {formatBytesPerSecond(download.average_speed_bps)}
                            </p>
                          </div>
                          <div>
                            <p className="text-slate-400">Elapsed Time</p>
                            <p className="text-slate-200 font-semibold">
                              {formatDuration(download.elapsed_ms)}
                            </p>
                          </div>
                          <div>
                            <p className="text-slate-400">Current Speed</p>
                            <p className="text-slate-200 font-semibold">
                              {formatBytesPerSecond(download.current_speed_bps)}
                            </p>
                          </div>
                          <div>
                            <p className="text-slate-400">ETC</p>
                            <p className="text-slate-200 font-semibold">
                              {formatDuration(download.estimated_remaining_ms)}
                            </p>
                          </div>
                        </div>
                        <p className="text-slate-400 mt-2 break-all text-cyan-400/70">{download.url}</p>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        </motion.div>
      )}

      {/* Empty State */}
      {state.downloads.length === 0 && state.total_queued_downloads === 0 && (
        <div className="flex flex-col items-center justify-center py-12 text-slate-400">
          <Activity size={32} className="mb-3 opacity-50" />
          <p>No active downloads or queued items</p>
          <p className="text-xs mt-1">Queue will show here when downloads start</p>
        </div>
      )}
    </div>
  );
};

export default QueueOrchestratorDashboard;
