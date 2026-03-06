import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Search, ArrowUpDown, ArrowUp, ArrowDown, Trash2, FolderOpen, RefreshCw, FileDown, X, Clock, CheckCircle, XCircle, AlertCircle, BarChart3 } from 'lucide-react';
import { motion } from 'framer-motion';

interface HistoryEntry {
  id: string;
  url: string;
  path: string;
  filename: string;
  total_size: number;
  downloaded_bytes: number;
  status: string;
  started_at: string;
  finished_at: string;
  avg_speed_bps: number;
  duration_secs: number;
  segments_used: number;
  error_message?: string;
  source_type?: string;
}

interface HistoryPage {
  entries: HistoryEntry[];
  total_count: number;
  page: number;
  page_size: number;
  total_pages: number;
}

type SortBy = 'date' | 'name' | 'size' | 'speed';
type StatusFilter = '' | 'Complete' | 'Error' | 'Cancelled';

const formatBytes = (b: number): string => {
  if (b >= 1073741824) return (b / 1073741824).toFixed(2) + ' GB';
  if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB';
  if (b >= 1024) return (b / 1024).toFixed(0) + ' KB';
  return b + ' B';
};

const formatSpeed = (b: number): string => {
  if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB/s';
  if (b >= 1024) return (b / 1024).toFixed(0) + ' KB/s';
  return b + ' B/s';
};

const formatDuration = (secs: number): string => {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
};

const formatDate = (iso: string): string => {
  if (!iso) return '—';
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }) + ' ' + d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  } catch { return iso; }
};

const StatusIcon: React.FC<{ status: string }> = ({ status }) => {
  switch (status) {
    case 'Complete': return <CheckCircle size={14} className="text-emerald-400" />;
    case 'Error': return <XCircle size={14} className="text-red-400" />;
    case 'Cancelled': return <AlertCircle size={14} className="text-amber-400" />;
    default: return <Clock size={14} className="text-slate-400" />;
  }
};

export const HistoryTab: React.FC = () => {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [totalPages, setTotalPages] = useState(1);
  const [page, setPage] = useState(1);
  const [pageSize] = useState(50);
  const [sortBy, setSortBy] = useState<SortBy>('date');
  const [sortDesc, setSortDesc] = useState(true);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('');
  const [searchQuery, setSearchQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [summary, setSummary] = useState<{ total_downloads: number; completed: number; failed: number; cancelled: number; total_bytes: number; avg_speed_bps: number } | null>(null);
  const [showSummary, setShowSummary] = useState(false);

  const fetchHistory = useCallback(async () => {
    setLoading(true);
    try {
      const filter: Record<string, unknown> = {
        page,
        page_size: pageSize,
        sort_by: sortBy,
        sort_desc: sortDesc,
      };
      if (statusFilter) filter.status = statusFilter;
      if (searchQuery.trim()) filter.date_from = undefined; // keep structure

      let result: HistoryPage;
      if (searchQuery.trim()) {
        // Use search endpoint for text search
        const found: HistoryEntry[] = await invoke('search_download_history', { query: searchQuery, limit: pageSize });
        result = { entries: found, total_count: found.length, page: 1, page_size: pageSize, total_pages: 1 };
      } else {
        result = await invoke('get_download_history', { filter });
      }
      setEntries(result.entries);
      setTotalCount(result.total_count);
      setTotalPages(result.total_pages);
    } catch (err) {
      console.error('Failed to load history:', err);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, sortBy, sortDesc, statusFilter, searchQuery]);

  useEffect(() => { fetchHistory(); }, [fetchHistory]);

  const handleExportCsv = async () => {
    try {
      const csvPath: string = await invoke('export_download_history_csv');
      alert(`Exported to: ${csvPath}`);
    } catch (err) {
      console.error('CSV export failed:', err);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke('delete_history_entry', { id });
      setEntries(prev => prev.filter(e => e.id !== id));
      setTotalCount(prev => prev - 1);
    } catch (err) {
      console.error('Failed to delete history entry:', err);
    }
  };

  const handleOpenFolder = async (path: string) => {
    try {
      // Extract directory from file path
      const dir = path.replace(/[\\/][^\\/]+$/, '');
      await invoke('open_folder', { path: dir });
    } catch (err) {
      console.error('Failed to open folder:', err);
    }
  };

  const handleClearAll = async () => {
    try {
      await invoke('clear_download_history');
      setEntries([]);
      setTotalCount(0);
      setTotalPages(1);
      setSummary(null);
    } catch (err) {
      console.error('Failed to clear history:', err);
    }
  };

  const fetchSummary = async () => {
    try {
      const s = await invoke<{ total_downloads: number; completed: number; failed: number; cancelled: number; total_bytes: number; avg_speed_bps: number }>('get_history_summary');
      setSummary(s);
      setShowSummary(true);
    } catch (err) {
      console.error('Failed to load summary:', err);
    }
  };

  const toggleSort = (field: SortBy) => {
    if (sortBy === field) setSortDesc(!sortDesc);
    else { setSortBy(field); setSortDesc(field === 'date' || field === 'speed' || field === 'size'); }
    setPage(1);
  };

  const SortBtn = ({ field, label }: { field: SortBy; label: string }) => {
    const active = sortBy === field;
    return (
      <button
        onClick={() => toggleSort(field)}
        className={`flex items-center gap-1 px-2 py-1 rounded text-xs font-medium transition-colors ${
          active ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30' : 'text-slate-400 hover:text-slate-200 border border-transparent'
        }`}
      >
        {label}
        {active ? (sortDesc ? <ArrowDown size={11} /> : <ArrowUp size={11} />) : <ArrowUpDown size={11} className="opacity-40" />}
      </button>
    );
  };

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="px-4 py-3 flex flex-col gap-2 shrink-0 border-b border-slate-700/30">
        <div className="flex items-center gap-2">
          <div className="relative flex-1 max-w-sm">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500" />
            <input
              type="text"
              value={searchQuery}
              onChange={e => { setSearchQuery(e.target.value); setPage(1); }}
              placeholder="Search history..."
              className="w-full bg-slate-800/50 border border-slate-700/50 rounded-lg pl-8 pr-7 py-1.5 text-xs text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-cyan-500/40"
            />
            {searchQuery && (
              <button onClick={() => setSearchQuery('')} className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300">
                <X size={12} />
              </button>
            )}
          </div>
          <div className="flex items-center gap-1">
            <SortBtn field="date" label="Date" />
            <SortBtn field="name" label="Name" />
            <SortBtn field="size" label="Size" />
            <SortBtn field="speed" label="Speed" />
          </div>
          <button
            onClick={handleExportCsv}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-slate-400 hover:text-cyan-300 border border-slate-700/50 rounded-lg hover:border-cyan-500/30 transition-colors ml-auto"
          >
            <FileDown size={13} /> Export CSV
          </button>
          <button
            onClick={fetchSummary}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-slate-400 hover:text-violet-300 border border-slate-700/50 rounded-lg hover:border-violet-500/30 transition-colors"
            title="View download summary"
          >
            <BarChart3 size={13} /> Summary
          </button>
          <button
            onClick={handleClearAll}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-slate-400 hover:text-red-300 border border-slate-700/50 rounded-lg hover:border-red-500/30 transition-colors"
            title="Clear all history"
          >
            <Trash2 size={13} /> Clear
          </button>
          <button
            onClick={fetchHistory}
            className="p-1.5 text-slate-500 hover:text-slate-300 transition-colors"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
          </button>
        </div>

        {/* Status filters */}
        <div className="flex items-center gap-1">
          {[
            { val: '' as StatusFilter, label: 'All', color: 'text-slate-300 bg-slate-700/40' },
            { val: 'Complete' as StatusFilter, label: 'Completed', color: 'text-emerald-300 bg-emerald-500/15' },
            { val: 'Error' as StatusFilter, label: 'Failed', color: 'text-red-300 bg-red-500/15' },
            { val: 'Cancelled' as StatusFilter, label: 'Cancelled', color: 'text-amber-300 bg-amber-500/15' },
          ].map(({ val, label, color }) => (
            <button
              key={val}
              onClick={() => { setStatusFilter(val); setPage(1); }}
              className={`px-2.5 py-1 rounded-full text-xs font-medium transition-colors ${
                statusFilter === val ? color + ' ring-1 ring-current/30' : 'text-slate-500 hover:text-slate-300'
              }`}
            >
              {label}
            </button>
          ))}
          <span className="text-xs text-slate-500 ml-auto">
            {totalCount} {totalCount === 1 ? 'entry' : 'entries'}
          </span>
        </div>

        {/* Summary Panel */}
        {showSummary && summary && (
          <div className="flex items-center gap-4 px-3 py-2 rounded-lg bg-violet-500/5 border border-violet-500/20">
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-slate-500">Total:</span>
              <span className="text-slate-200 font-medium">{summary.total_downloads}</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-emerald-500">Done:</span>
              <span className="text-emerald-300">{summary.completed}</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-red-500">Failed:</span>
              <span className="text-red-300">{summary.failed}</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-amber-500">Cancelled:</span>
              <span className="text-amber-300">{summary.cancelled}</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-slate-500">Downloaded:</span>
              <span className="text-cyan-300">{formatBytes(summary.total_bytes)}</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-slate-500">Avg Speed:</span>
              <span className="text-cyan-300">{formatSpeed(summary.avg_speed_bps)}</span>
            </div>
            <button onClick={() => setShowSummary(false)} className="ml-auto text-slate-500 hover:text-slate-300">
              <X size={12} />
            </button>
          </div>
        )}
      </div>

      {/* Entries List */}
      <div className="flex-1 overflow-y-auto custom-scrollbar px-4 py-2 space-y-1.5">
        {entries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-slate-500 opacity-50">
            <Clock size={40} className="mb-3" />
            <p className="text-sm">{loading ? 'Loading...' : 'No history entries found.'}</p>
          </div>
        ) : (
          entries.map(entry => (
            <motion.div
              key={entry.id}
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              className="bg-slate-800/40 border border-slate-700/30 rounded-lg px-3 py-2.5 hover:border-slate-600/40 transition-colors group"
            >
              <div className="flex items-start gap-3">
                <StatusIcon status={entry.status} />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-slate-200 truncate" title={entry.filename}>
                      {entry.filename}
                    </span>
                    <span className="text-xs text-slate-500 font-mono shrink-0">
                      {formatBytes(entry.total_size)}
                    </span>
                  </div>
                  <div className="flex items-center gap-3 mt-0.5 text-xs text-slate-500">
                    <span>{formatDate(entry.finished_at || entry.started_at)}</span>
                    {entry.avg_speed_bps > 0 && <span className="text-cyan-400/70 font-mono">{formatSpeed(entry.avg_speed_bps)}</span>}
                    {entry.duration_secs > 0 && <span>{formatDuration(entry.duration_secs)}</span>}
                    {entry.segments_used > 1 && <span>{entry.segments_used} threads</span>}
                  </div>
                  {entry.error_message && (
                    <p className="text-xs text-red-400/80 mt-1 truncate" title={entry.error_message}>
                      {entry.error_message}
                    </p>
                  )}
                </div>
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                  {entry.path && (
                    <button
                      onClick={() => handleOpenFolder(entry.path)}
                      className="p-1.5 text-slate-500 hover:text-cyan-400 transition-colors"
                      title="Open folder"
                    >
                      <FolderOpen size={13} />
                    </button>
                  )}
                  <button
                    onClick={() => handleDelete(entry.id)}
                    className="p-1.5 text-slate-500 hover:text-red-400 transition-colors"
                    title="Remove from history"
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            </motion.div>
          ))
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="px-4 py-2 border-t border-slate-700/30 flex items-center justify-center gap-2 shrink-0">
          <button
            disabled={page <= 1}
            onClick={() => setPage(p => p - 1)}
            className="px-3 py-1 text-xs text-slate-400 hover:text-slate-200 disabled:opacity-30 border border-slate-700/50 rounded"
          >
            Previous
          </button>
          <span className="text-xs text-slate-500">
            Page {page} of {totalPages}
          </span>
          <button
            disabled={page >= totalPages}
            onClick={() => setPage(p => p + 1)}
            className="px-3 py-1 text-xs text-slate-400 hover:text-slate-200 disabled:opacity-30 border border-slate-700/50 rounded"
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
};
