import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AlertTriangle, Activity, TrendingUp, CheckCircle2, Clock, AlertCircle, Zap, BarChart3 } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

interface DownloadHealth {
  download_id: string;
  status: 'Healthy' | 'AtRisk' | 'Failed' | 'Recovering' | 'Corrupted';
  consecutive_failures: number;
  recoveries_attempted: number;
  data_lost_bytes: number;
  retry_history: [number, string][];
  last_error?: {
    category: string;
    message: string;
    retry_count: number;
  };
}

interface RecoveryAction {
  action_id: string;
  download_id: string;
  action_type: string;
  status: string;
  created_at: number;
}

interface NetworkHealth {
  is_online: boolean;
  latency_ms: number;
  packet_loss_percent: number;
  dns_working: boolean;
}

interface ResilienceDashboardProps {
  isOpen: boolean;
  onClose: () => void;
}

export const ResilienceDashboard: React.FC<ResilienceDashboardProps> = ({
  isOpen,
  onClose,
}) => {
  const [atRiskDownloads, setAtRiskDownloads] = useState<DownloadHealth[]>([]);
  const [pendingActions, setPendingActions] = useState<RecoveryAction[]>([]);
  const [diagnostics, setDiagnostics] = useState<any>(null);
  const [errorStats, setErrorStats] = useState<any>(null);
  const [recoveryStats, setRecoveryStats] = useState<any>(null);
  const [loading, setLoading] = useState(false);
  const [selectedDownload, setSelectedDownload] = useState<string | null>(null);

  const loadData = async () => {
    setLoading(true);
    try {
      const [atRisk, actions, diag, errors, recovery] = await Promise.all([
        invoke('get_at_risk_downloads').catch(() => []),
        invoke('get_pending_recovery_actions').catch(() => []),
        invoke('get_diagnostics_summary').catch(() => null),
        invoke('get_error_statistics', { minutes: 60 }).catch(() => null),
        invoke('get_recovery_statistics').catch(() => null),
      ]);

      setAtRiskDownloads(atRisk as DownloadHealth[]);
      setPendingActions(actions as RecoveryAction[]);
      setDiagnostics(diag);
      setErrorStats(errors);
      setRecoveryStats(recovery);
    } catch (error) {
      console.error('Failed to load resilience data:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      loadData();
      const interval = setInterval(loadData, 10000); // Refresh every 10 seconds
      return () => clearInterval(interval);
    }
  }, [isOpen]);

  const getStatusColor = (status: string): string => {
    switch (status) {
      case 'Healthy': return 'bg-green-500/20 border-green-500/30 text-green-400';
      case 'AtRisk': return 'bg-yellow-500/20 border-yellow-500/30 text-yellow-400';
      case 'Failed': return 'bg-red-500/20 border-red-500/30 text-red-400';
      case 'Recovering': return 'bg-blue-500/20 border-blue-500/30 text-blue-400';
      case 'Corrupted': return 'bg-red-600/20 border-red-600/30 text-red-500';
      default: return 'bg-gray-500/20 border-gray-500/30 text-gray-400';
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'Healthy': return <CheckCircle2 size={20} />;
      case 'AtRisk': return <AlertCircle size={20} />;
      case 'Failed': return <AlertTriangle size={20} />;
      case 'Recovering': return <Zap size={20} className="animate-pulse" />;
      case 'Corrupted': return <AlertTriangle size={20} />;
      default: return <Activity size={20} />;
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4">
      <motion.div
        className="relative w-full max-w-6xl max-h-[90vh] bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 border border-white/10 shadow-2xl rounded-2xl flex flex-col overflow-hidden"
        initial={{ scale: 0.95, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        exit={{ scale: 0.95, opacity: 0 }}
      >
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-white/5 bg-black/20">
          <div className="flex items-center gap-3">
            <div className="p-3 bg-purple-500/20 rounded-lg border border-purple-500/30">
              <BarChart3 size={24} className="text-purple-400" />
            </div>
            <div>
              <h2 className="text-2xl font-bold text-slate-100">Resilience Dashboard</h2>
              <p className="text-sm text-slate-500">Monitor downloads, errors, and recovery actions</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-white/10 rounded-lg transition-colors text-slate-400 hover:text-white"
          >
            ✕
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto custom-scrollbar">
          <div className="p-6 space-y-6">
            {/* KPI Cards */}
            <div className="grid grid-cols-4 gap-4">
              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-4 hover:border-white/20 transition-colors"
                whileHover={{ scale: 1.02 }}
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-slate-400 text-sm">At Risk Downloads</span>
                  <AlertTriangle size={18} className="text-yellow-400" />
                </div>
                <div className="text-3xl font-bold text-yellow-400">
                  {atRiskDownloads.length}
                </div>
                <p className="text-xs text-slate-600 mt-1">Needing attention</p>
              </motion.div>

              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-4 hover:border-white/20 transition-colors"
                whileHover={{ scale: 1.02 }}
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-slate-400 text-sm">Pending Actions</span>
                  <Zap size={18} className="text-blue-400" />
                </div>
                <div className="text-3xl font-bold text-blue-400">
                  {pendingActions.length}
                </div>
                <p className="text-xs text-slate-600 mt-1">Ready to execute</p>
              </motion.div>

              {errorStats && (
                <motion.div
                  className="bg-white/5 border border-white/10 rounded-xl p-4 hover:border-white/20 transition-colors"
                  whileHover={{ scale: 1.02 }}
                >
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-slate-400 text-sm">Recent Errors (1h)</span>
                    <AlertCircle size={18} className="text-red-400" />
                  </div>
                  <div className="text-3xl font-bold text-red-400">
                    {errorStats.recent_errors_count}
                  </div>
                  <p className="text-xs text-slate-600 mt-1">Past hour</p>
                </motion.div>
              )}

              {recoveryStats && (
                <motion.div
                  className="bg-white/5 border border-white/10 rounded-xl p-4 hover:border-white/20 transition-colors"
                  whileHover={{ scale: 1.02 }}
                >
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-slate-400 text-sm">Recovery Success Rate</span>
                    <TrendingUp size={18} className="text-green-400" />
                  </div>
                  <div className="text-3xl font-bold text-green-400">
                    {recoveryStats.success_rate.toFixed(1)}%
                  </div>
                  <p className="text-xs text-slate-600 mt-1">
                    {recoveryStats.successful_recoveries} of {recoveryStats.total_recovery_plans}
                  </p>
                </motion.div>
              )}
            </div>

            {/* At Risk Downloads */}
            {atRiskDownloads.length > 0 && (
              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-6"
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
              >
                <h3 className="text-lg font-semibold text-slate-200 mb-4 flex items-center gap-2">
                  <AlertTriangle size={20} className="text-yellow-400" />
                  Downloads At Risk
                </h3>
                <div className="space-y-3">
                  {atRiskDownloads.slice(0, 5).map((dl) => (
                    <motion.div
                      key={dl.download_id}
                      className={`p-4 border rounded-lg cursor-pointer transition-all ${getStatusColor(
                        dl.status
                      )}`}
                      onClick={() => setSelectedDownload(dl.download_id)}
                      whileHover={{ scale: 1.02 }}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                          {getStatusIcon(dl.status)}
                          <span className="font-medium">{dl.download_id}</span>
                        </div>
                        <span className="text-sm">{dl.consecutive_failures} failures</span>
                      </div>
                      {dl.last_error && (
                        <p className="text-sm opacity-75 truncate">
                          {dl.last_error.message}
                        </p>
                      )}
                    </motion.div>
                  ))}
                </div>
              </motion.div>
            )}

            {/* Pending Recovery Actions */}
            {pendingActions.length > 0 && (
              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-6"
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
              >
                <h3 className="text-lg font-semibold text-slate-200 mb-4 flex items-center gap-2">
                  <Zap size={20} className="text-blue-400" />
                  Pending Recovery Actions
                </h3>
                <div className="space-y-2">
                  {pendingActions.slice(0, 5).map((action) => (
                    <motion.div
                      key={action.action_id}
                      className="bg-white/5 border border-blue-500/30 rounded-lg p-3 flex items-center justify-between"
                      whileHover={{ backgroundColor: 'rgba(59, 130, 246, 0.1)' }}
                    >
                      <div className="flex items-center gap-3 flex-1">
                        <Clock size={16} className="text-blue-400 shrink-0" />
                        <div className="min-w-0 flex-1">
                          <p className="text-sm font-medium text-slate-300 truncate">
                            {action.action_type}
                          </p>
                          <p className="text-xs text-slate-600">{action.download_id}</p>
                        </div>
                      </div>
                      <span className="text-xs bg-blue-500/20 text-blue-400 px-2 py-1 rounded shrink-0">
                        {action.status}
                      </span>
                    </motion.div>
                  ))}
                </div>
              </motion.div>
            )}

            {/* Network Health */}
            {diagnostics?.current_health && (
              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-6"
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
              >
                <h3 className="text-lg font-semibold text-slate-200 mb-4">Network Health</h3>
                <div className="grid grid-cols-2 gap-4">
                  <div className="bg-white/[0.02] rounded-lg p-4">
                    <p className="text-sm text-slate-400 mb-2">Status</p>
                    <p className="text-xl font-bold text-green-400">
                      {diagnostics.current_health.is_online ? '🟢 Online' : '🔴 Offline'}
                    </p>
                  </div>
                  <div className="bg-white/[0.02] rounded-lg p-4">
                    <p className="text-sm text-slate-400 mb-2">Latency</p>
                    <p className="text-xl font-bold text-slate-300">
                      {diagnostics.current_health.latency_ms}ms
                    </p>
                  </div>
                  <div className="bg-white/[0.02] rounded-lg p-4">
                    <p className="text-sm text-slate-400 mb-2">Packet Loss</p>
                    <p className="text-xl font-bold text-slate-300">
                      {diagnostics.current_health.packet_loss_percent.toFixed(1)}%
                    </p>
                  </div>
                  <div className="bg-white/[0.02] rounded-lg p-4">
                    <p className="text-sm text-slate-400 mb-2">DNS</p>
                    <p className="text-xl font-bold text-green-400">
                      {diagnostics.current_health.dns_working ? '✓ Working' : '✗ Failed'}
                    </p>
                  </div>
                </div>
              </motion.div>
            )}

            {/* Error Categories */}
            {errorStats?.error_categories && Object.keys(errorStats.error_categories).length > 0 && (
              <motion.div
                className="bg-white/5 border border-white/10 rounded-xl p-6"
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
              >
                <h3 className="text-lg font-semibold text-slate-200 mb-4">Error Distribution</h3>
                <div className="space-y-2">
                  {Object.entries(errorStats.error_categories).map(([category, count]) => (
                    <div key={category} className="flex items-center justify-between p-3 bg-white/[0.02] rounded-lg">
                      <span className="text-sm text-slate-300">{category}</span>
                      <span className="text-sm font-bold text-red-400">{count}</span>
                    </div>
                  ))}
                </div>
              </motion.div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="border-t border-white/5 bg-black/20 px-6 py-4 flex items-center justify-between">
          <button
            onClick={loadData}
            disabled={loading}
            className="px-4 py-2 bg-purple-600/20 hover:bg-purple-600/30 text-purple-400 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
            {loading ? 'Refreshing...' : 'Refresh'}
          </button>
          <span className="text-xs text-slate-500">
            Last updated: {new Date().toLocaleTimeString()}
          </span>
        </div>
      </motion.div>
    </div>
  );
};
