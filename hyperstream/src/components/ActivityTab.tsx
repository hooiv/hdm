import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Download, CheckCircle2, AlertTriangle, Cpu, Trash2, Filter } from 'lucide-react';

interface LedgerEvent {
  timestamp: number;
  aggregate_id: string;
  event_type: string;
  payload: Record<string, unknown>;
}

type SystemEvent =
  | { DownloadStarted: string }
  | { DownloadProgress: [string, number] }
  | { DownloadCompleted: string }
  | { ModuleAction: [string, string] }
  | { SystemError: [string, string] };

const EVENT_TYPE_CONFIG: Record<string, { icon: React.ReactNode; color: string; label: string }> = {
  download_started: { icon: <Download size={14} />, color: 'text-cyan-400 bg-cyan-500/10 border-cyan-500/20', label: 'Download Started' },
  download_completed: { icon: <CheckCircle2 size={14} />, color: 'text-emerald-400 bg-emerald-500/10 border-emerald-500/20', label: 'Download Completed' },
  download_failed: { icon: <AlertTriangle size={14} />, color: 'text-red-400 bg-red-500/10 border-red-500/20', label: 'Download Failed' },
  download_progress: { icon: <Download size={14} />, color: 'text-blue-400 bg-blue-500/10 border-blue-500/20', label: 'Progress' },
  module_action: { icon: <Cpu size={14} />, color: 'text-violet-400 bg-violet-500/10 border-violet-500/20', label: 'Module Action' },
  system_error: { icon: <AlertTriangle size={14} />, color: 'text-amber-400 bg-amber-500/10 border-amber-500/20', label: 'System Error' },
};

const DEFAULT_CONFIG = { icon: <Cpu size={14} />, color: 'text-slate-400 bg-slate-500/10 border-slate-500/20', label: 'Event' };

function getEventConfig(eventType: string) {
  return EVENT_TYPE_CONFIG[eventType] || DEFAULT_CONFIG;
}

function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000);
  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();
  if (isToday) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' }) + ' ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatRelativeTime(ts: number): string {
  const diff = Math.floor(Date.now() / 1000) - ts;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function getEventSummary(event: LedgerEvent): string {
  const p = event.payload;
  if (typeof p === 'object' && p !== null) {
    if ('filename' in p && typeof p.filename === 'string') return p.filename;
    if ('url' in p && typeof p.url === 'string') {
      try {
        return new URL(p.url as string).pathname.split('/').pop() || (p.url as string);
      } catch {
        return p.url as string;
      }
    }
    if ('message' in p && typeof p.message === 'string') return p.message as string;
    if ('error' in p && typeof p.error === 'string') return p.error as string;
  }
  return event.aggregate_id || event.event_type;
}

function systemEventToLedger(ev: SystemEvent): LedgerEvent {
  const ts = Math.floor(Date.now() / 1000);
  if ('DownloadStarted' in ev) return { timestamp: ts, aggregate_id: ev.DownloadStarted, event_type: 'download_started', payload: {} };
  if ('DownloadCompleted' in ev) return { timestamp: ts, aggregate_id: ev.DownloadCompleted, event_type: 'download_completed', payload: {} };
  if ('DownloadProgress' in ev) return { timestamp: ts, aggregate_id: ev.DownloadProgress[0], event_type: 'download_progress', payload: { bytes: ev.DownloadProgress[1] } };
  if ('ModuleAction' in ev) return { timestamp: ts, aggregate_id: ev.ModuleAction[0], event_type: 'module_action', payload: { action: ev.ModuleAction[1] } };
  if ('SystemError' in ev) return { timestamp: ts, aggregate_id: ev.SystemError[0], event_type: 'system_error', payload: { error: ev.SystemError[1] } };
  return { timestamp: ts, aggregate_id: '', event_type: 'unknown', payload: {} };
}

const EVENT_FILTERS = ['all', 'download_started', 'download_completed', 'download_failed', 'module_action', 'system_error'] as const;

export const ActivityTab: React.FC = () => {
  const [events, setEvents] = useState<LedgerEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>('all');
  const [liveEvents, setLiveEvents] = useState<LedgerEvent[]>([]);
  const maxLive = useRef(50);

  const loadEvents = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<LedgerEvent[]>('get_activity_log', {
        limit: 500,
        eventType: filter === 'all' ? null : filter,
      });
      setEvents(result);
    } catch (err) {
      console.error('[ActivityTab] Failed to load events:', err);
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    loadEvents();
  }, [loadEvents]);

  // Listen for live system events
  useEffect(() => {
    const unlisten = listen<SystemEvent>('system-bus-event', (ev) => {
      const ledger = systemEventToLedger(ev.payload);
      // Skip high-frequency progress events in the live feed
      if (ledger.event_type === 'download_progress') return;
      setLiveEvents(prev => {
        const next = [ledger, ...prev];
        if (next.length > maxLive.current) next.length = maxLive.current;
        return next;
      });
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  const clearLive = () => setLiveEvents([]);

  // Combine live + persisted, deduplicate by timestamp+id
  const allEvents = [...liveEvents, ...events];
  const filtered = filter === 'all' ? allEvents : allEvents.filter(e => e.event_type === filter);

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden px-6 py-4">
      {/* Toolbar */}
      <div className="flex items-center gap-3 mb-4 flex-shrink-0">
        <div className="flex items-center gap-1 bg-black/20 rounded-lg p-1 border border-white/5">
          <Filter size={14} className="text-slate-500 ml-2" />
          {EVENT_FILTERS.map(f => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              className={`px-3 py-1.5 rounded-md text-xs font-medium transition-all ${
                filter === f
                  ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
                  : 'text-slate-400 hover:text-white hover:bg-white/5 border border-transparent'
              }`}
            >
              {f === 'all' ? 'All' : (EVENT_TYPE_CONFIG[f]?.label || f)}
            </button>
          ))}
        </div>

        <div className="flex-1" />

        {liveEvents.length > 0 && (
          <button
            onClick={clearLive}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-slate-400 hover:text-white bg-white/5 hover:bg-white/10 transition-all border border-white/5"
          >
            <Trash2 size={12} />
            Clear Live ({liveEvents.length})
          </button>
        )}

        <button
          onClick={loadEvents}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-slate-400 hover:text-white bg-white/5 hover:bg-white/10 transition-all border border-white/5"
        >
          <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {/* Live events banner */}
      {liveEvents.length > 0 && (
        <div className="mb-3 px-3 py-2 bg-cyan-500/5 border border-cyan-500/20 rounded-lg flex items-center gap-2 flex-shrink-0">
          <div className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />
          <span className="text-xs text-cyan-300 font-medium">{liveEvents.length} new event{liveEvents.length > 1 ? 's' : ''} since page load</span>
        </div>
      )}

      {/* Event list */}
      <div className="flex-1 overflow-y-auto custom-scrollbar space-y-1">
        {filtered.length === 0 && !loading && (
          <div className="flex-1 flex flex-col items-center justify-center text-slate-500 py-20">
            <Cpu size={48} className="mb-4 opacity-30" />
            <p className="text-sm">No activity events found</p>
            <p className="text-xs text-slate-600 mt-1">Events will appear as downloads start, complete, or fail</p>
          </div>
        )}

        {filtered.map((event, idx) => {
          const config = getEventConfig(event.event_type);
          const summary = getEventSummary(event);
          const isLive = idx < liveEvents.length && filter === 'all';

          return (
            <div
              key={`${event.timestamp}-${event.aggregate_id}-${idx}`}
              className={`flex items-start gap-3 px-4 py-3 rounded-lg border transition-all hover:bg-white/[0.02] ${
                isLive ? 'bg-cyan-500/[0.03] border-cyan-500/10' : 'bg-white/[0.01] border-white/5'
              }`}
            >
              {/* Icon */}
              <div className={`flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center border ${config.color}`}>
                {config.icon}
              </div>

              {/* Content */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className={`text-xs font-bold ${config.color.split(' ')[0]}`}>
                    {config.label}
                  </span>
                  {isLive && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 font-bold">
                      LIVE
                    </span>
                  )}
                </div>
                <p className="text-xs text-slate-300 mt-0.5 truncate" title={summary}>
                  {summary}
                </p>
                {event.aggregate_id && event.aggregate_id !== summary && (
                  <p className="text-[10px] text-slate-600 mt-0.5 font-mono truncate">
                    ID: {event.aggregate_id}
                  </p>
                )}
              </div>

              {/* Timestamp */}
              <div className="flex-shrink-0 text-right">
                <p className="text-[10px] text-slate-500">{formatRelativeTime(event.timestamp)}</p>
                <p className="text-[10px] text-slate-600">{formatTimestamp(event.timestamp)}</p>
              </div>
            </div>
          );
        })}
      </div>

      {/* Footer stats */}
      <div className="flex items-center justify-between pt-3 border-t border-white/5 flex-shrink-0 mt-2">
        <span className="text-[10px] text-slate-600">
          Showing {filtered.length} event{filtered.length !== 1 ? 's' : ''}
          {liveEvents.length > 0 && ` (${liveEvents.length} live)`}
        </span>
        <span className="text-[10px] text-slate-600">
          Events are persisted to disk and survive restarts
        </span>
      </div>
    </div>
  );
};
