import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
  ListOrdered, ArrowUp, ArrowBigUp, ChevronUp, ChevronDown,
  RefreshCw, Play, X, Settings2, AlertCircle, Pause, PauseCircle,
  PlayCircle, Link2, Unlink, Hash, FolderOpen
} from 'lucide-react';

interface QueuedDownload {
  id: string;
  url: string;
  path: string;
  priority: string; // "Low" | "Normal" | "High"
  added_at: number;
  custom_headers: Record<string, string> | null;
  expected_checksum: string | null;
  retry_count: number;
  max_retries: number;
  retry_delay_ms: number;
  depends_on: string[];
  custom_segments: number | null;
  group: string | null;
}

interface QueueStatus {
  max_concurrent: number;
  active_count: number;
  queued_count: number;
  queued_items: QueuedDownload[];
  active_ids: string[];
  paused: boolean;
  blocked_ids: string[];
}

const priorityColors: Record<string, { text: string; bg: string; border: string }> = {
  High: { text: 'text-red-400', bg: 'bg-red-500/10', border: 'border-red-500/20' },
  Normal: { text: 'text-blue-400', bg: 'bg-blue-500/10', border: 'border-blue-500/20' },
  Low: { text: 'text-slate-400', bg: 'bg-slate-500/10', border: 'border-slate-500/20' },
};

export const QueueManager: React.FC = () => {
  const [status, setStatus] = useState<QueueStatus | null>(null);
  const [maxConcurrent, setMaxConcurrent] = useState<number>(3);
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [groups, setGroups] = useState<string[]>([]);
  const [filterGroup, setFilterGroup] = useState<string | null>(null);

  const refreshQueue = useCallback(async () => {
    try {
      const [s, g] = await Promise.all([
        invoke<QueueStatus>('get_queue_status'),
        invoke<string[]>('get_queue_groups'),
      ]);
      setStatus(s);
      setMaxConcurrent(s.max_concurrent);
      setGroups(g);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    refreshQueue();
    const interval = setInterval(refreshQueue, 3000);
    return () => clearInterval(interval);
  }, [refreshQueue]);

  const handleRemove = async (id: string) => {
    try {
      await invoke('remove_from_queue', { id });
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleSetPriority = async (id: string, priority: string) => {
    try {
      await invoke('set_queue_priority', { id, priority });
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleMoveToFront = async (id: string) => {
    try {
      await invoke('move_queue_item_to_front', { id });
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleClearQueue = async () => {
    if (!window.confirm('Clear all queued downloads? Active downloads will continue.')) return;
    try {
      await invoke('clear_download_queue');
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handlePauseQueue = async () => {
    try {
      await invoke('pause_download_queue');
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleResumeQueue = async () => {
    try {
      await invoke('resume_download_queue');
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleRemoveDependency = async (downloadId: string, depId: string) => {
    try {
      await invoke('remove_download_dependency', { downloadId, dependsOnId: depId });
      refreshQueue();
    } catch { /* ignore */ }
  };

  const handleMaxConcurrentChange = async (val: number) => {
    const clamped = Math.max(1, Math.min(20, val));
    try {
      await invoke('set_max_concurrent_downloads', { max: clamped });
      setMaxConcurrent(clamped);
      refreshQueue();
    } catch { /* ignore */ }
  };

  const relativeTime = (ms: number) => {
    const diff = Date.now() - ms;
    if (diff < 60000) return 'just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return `${Math.floor(diff / 86400000)}d ago`;
  };

  return (
    <div className="p-4 space-y-4 h-full overflow-y-auto custom-scrollbar">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <ListOrdered size={20} className="text-cyan-400" />
          <h2 className="text-lg font-bold text-white tracking-tight">Download Queue</h2>
          {status && (
            <div className="flex gap-2 ml-2">
              <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono">
                {status.active_count} active
              </span>
              <span className="text-[10px] px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20 font-mono">
                {status.queued_count} queued
              </span>
              {status.paused && (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/20 font-mono animate-pulse">
                  PAUSED
                </span>
              )}
            </div>
          )}
        </div>
        <div className="flex items-center gap-2">
          {/* Pause / Resume Queue */}
          {status && (
            status.paused ? (
              <button
                onClick={handleResumeQueue}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 hover:bg-emerald-500/20 transition-colors"
                title="Resume Queue"
              >
                <PlayCircle size={14} /> Resume
              </button>
            ) : (
              <button
                onClick={handlePauseQueue}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-amber-400 bg-amber-500/10 border border-amber-500/20 hover:bg-amber-500/20 transition-colors"
                title="Pause Queue — stops dequeuing new downloads"
              >
                <PauseCircle size={14} /> Pause
              </button>
            )
          )}
          <button
            onClick={() => setShowSettings(!showSettings)}
            className="p-2 rounded-lg text-slate-400 hover:text-white hover:bg-white/5 transition-colors"
            title="Queue Settings"
          >
            <Settings2 size={16} />
          </button>
          <button
            onClick={refreshQueue}
            className="p-2 rounded-lg text-slate-400 hover:text-cyan-400 hover:bg-cyan-500/10 transition-colors"
            title="Refresh"
          >
            <RefreshCw size={16} />
          </button>
          {status && status.queued_count > 0 && (
            <button
              onClick={handleClearQueue}
              className="px-3 py-1.5 rounded-lg text-xs font-medium text-red-400 bg-red-500/10 border border-red-500/20 hover:bg-red-500/20 transition-colors"
            >
              Clear Queue
            </button>
          )}
        </div>
      </div>

      {/* Settings Panel */}
      <AnimatePresence>
        {showSettings && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="overflow-hidden"
          >
            <div className="p-4 rounded-xl bg-slate-900/60 border border-slate-700/30 backdrop-blur-md space-y-3">
              <div className="flex items-center justify-between">
                <label className="text-xs text-slate-400 font-medium">Max Concurrent Downloads</label>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleMaxConcurrentChange(maxConcurrent - 1)}
                    className="p-1 rounded bg-slate-800 border border-slate-700 text-slate-400 hover:text-white transition-colors"
                  >
                    <ChevronDown size={14} />
                  </button>
                  <span className="text-sm font-mono text-white w-8 text-center">{maxConcurrent}</span>
                  <button
                    onClick={() => handleMaxConcurrentChange(maxConcurrent + 1)}
                    className="p-1 rounded bg-slate-800 border border-slate-700 text-slate-400 hover:text-white transition-colors"
                  >
                    <ChevronUp size={14} />
                  </button>
                </div>
              </div>
              <p className="text-[10px] text-slate-500">
                Controls how many downloads run simultaneously. Queued downloads start automatically when slots become available.
              </p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {error && (
        <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400 flex items-center gap-2">
          <AlertCircle size={14} />
          {error}
        </div>
      )}

      {/* Group Filter */}
      {groups.length > 0 && (
        <div className="flex items-center gap-2 flex-wrap">
          <FolderOpen size={14} className="text-slate-500" />
          <button
            onClick={() => setFilterGroup(null)}
            className={`text-[10px] px-2 py-0.5 rounded-full border transition-colors ${
              filterGroup === null
                ? 'bg-cyan-500/20 text-cyan-400 border-cyan-500/30'
                : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-white'
            }`}
          >
            All
          </button>
          {groups.map(g => (
            <button
              key={g}
              onClick={() => setFilterGroup(g)}
              className={`text-[10px] px-2 py-0.5 rounded-full border transition-colors ${
                filterGroup === g
                  ? 'bg-cyan-500/20 text-cyan-400 border-cyan-500/30'
                  : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-white'
              }`}
            >
              {g}
            </button>
          ))}
        </div>
      )}

      {/* Active Downloads */}
      {status && status.active_ids.length > 0 && (
        <div className="space-y-2">
          <h3 className="text-xs font-semibold text-emerald-400 uppercase tracking-wider flex items-center gap-2">
            <Play size={12} /> Active ({status.active_count}/{status.max_concurrent} slots)
          </h3>
          <div className="space-y-1">
            {status.active_ids.map(id => (
              <div key={id} className="flex items-center gap-3 p-2.5 rounded-lg bg-emerald-500/5 border border-emerald-500/10">
                <div className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
                <span className="text-xs text-slate-300 font-mono truncate flex-1">{id}</span>
                <span className="text-[10px] text-emerald-400 font-medium">Downloading</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Queued Items */}
      {status && status.queued_items.length > 0 ? (
        <div className="space-y-2">
          <h3 className="text-xs font-semibold text-blue-400 uppercase tracking-wider flex items-center gap-2">
            <ListOrdered size={12} /> Queued ({status.queued_count})
          </h3>
          <div className="space-y-1.5">
            <AnimatePresence>
              {status.queued_items
                .filter(item => !filterGroup || item.group === filterGroup)
                .map((item, index) => {
                const pc = priorityColors[item.priority] || priorityColors.Normal;
                const isBlocked = status.blocked_ids?.includes(item.id);
                return (
                  <motion.div
                    key={item.id}
                    initial={{ opacity: 0, x: -10 }}
                    animate={{ opacity: 1, x: 0 }}
                    exit={{ opacity: 0, x: 10 }}
                    className={`flex items-center gap-3 p-3 rounded-lg bg-slate-900/50 border transition-colors group ${
                      isBlocked
                        ? 'border-amber-500/30 bg-amber-500/5'
                        : 'border-slate-700/30 hover:border-slate-600/50'
                    }`}
                  >
                    {/* Position */}
                    <span className="text-[10px] font-mono text-slate-500 w-6 text-center">#{index + 1}</span>

                    {/* Priority badge */}
                    <span className={`text-[9px] uppercase font-bold px-1.5 py-0.5 rounded ${pc.text} ${pc.bg} border ${pc.border}`}>
                      {item.priority}
                    </span>

                    {/* Info */}
                    <div className="flex-1 min-w-0">
                      <div className="text-xs text-slate-200 truncate" title={item.url}>
                        {item.url.split('/').pop() || item.url}
                      </div>
                      <div className="text-[10px] text-slate-500 truncate mt-0.5">
                        {item.url}
                      </div>
                      <div className="flex gap-3 mt-1 text-[10px] text-slate-500 flex-wrap">
                        <span>Added {relativeTime(item.added_at)}</span>
                        {item.retry_count > 0 && (
                          <span className="text-amber-400">Retry {item.retry_count}/{item.max_retries}</span>
                        )}
                        {item.expected_checksum && (
                          <span className="text-cyan-400" title={item.expected_checksum}>✓ Checksum</span>
                        )}
                        {item.custom_segments && (
                          <span className="text-violet-400 flex items-center gap-0.5" title={`${item.custom_segments} segments`}>
                            <Hash size={9} /> {item.custom_segments} seg
                          </span>
                        )}
                        {item.group && (
                          <span className="text-teal-400 flex items-center gap-0.5">
                            <FolderOpen size={9} /> {item.group}
                          </span>
                        )}
                      </div>
                      {/* Dependency badges */}
                      {item.depends_on.length > 0 && (
                        <div className="flex gap-1 mt-1 flex-wrap">
                          {item.depends_on.map(depId => (
                            <span
                              key={depId}
                              className="inline-flex items-center gap-1 text-[9px] px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20"
                            >
                              <Link2 size={8} />
                              <span className="max-w-[100px] truncate" title={depId}>{depId}</span>
                              <button
                                onClick={() => handleRemoveDependency(item.id, depId)}
                                className="ml-0.5 hover:text-red-400 transition-colors"
                                title="Remove dependency"
                              >
                                <Unlink size={8} />
                              </button>
                            </span>
                          ))}
                        </div>
                      )}
                      {isBlocked && (
                        <div className="text-[9px] text-amber-400 mt-1 flex items-center gap-1">
                          <Pause size={9} /> Waiting for dependencies
                        </div>
                      )}
                    </div>

                    {/* Actions */}
                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={() => handleMoveToFront(item.id)}
                        className="p-1.5 rounded text-slate-400 hover:text-amber-400 hover:bg-amber-500/10 transition-colors"
                        title="Move to front"
                      >
                        <ArrowBigUp size={14} />
                      </button>

                      {/* Priority cycle: Low → Normal → High */}
                      <button
                        onClick={() => {
                          const next = item.priority === 'Low' ? 'Normal' : item.priority === 'Normal' ? 'High' : 'Low';
                          handleSetPriority(item.id, next);
                        }}
                        className="p-1.5 rounded text-slate-400 hover:text-blue-400 hover:bg-blue-500/10 transition-colors"
                        title={`Priority: ${item.priority} (click to cycle)`}
                      >
                        <ArrowUp size={14} />
                      </button>

                      <button
                        onClick={() => handleRemove(item.id)}
                        className="p-1.5 rounded text-slate-400 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                        title="Remove from queue"
                      >
                        <X size={14} />
                      </button>
                    </div>
                  </motion.div>
                );
              })}
            </AnimatePresence>
          </div>
        </div>
      ) : (
        status && (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500">
            <ListOrdered size={40} className="mb-3 opacity-30" />
            <p className="text-sm font-medium">Queue is empty</p>
            <p className="text-xs mt-1 opacity-70">
              Downloads will queue when {status.max_concurrent} concurrent slots are full
            </p>
          </div>
        )
      )}
    </div>
  );
};
