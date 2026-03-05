import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { motion, AnimatePresence } from 'framer-motion';
import {
    Magnet, Play, Pause, RotateCcw, Trash2, FolderOpen,
    ChevronDown, ChevronUp, File, CheckSquare, Square,
    AlertCircle, CheckCircle2, Loader2, HardDrive, Flag, Pin,
} from 'lucide-react';
import {
    TorrentActionFailedEvent,
    TorrentBulkActionResult,
    TorrentDiagnostics,
    TorrentFileInfo,
    TorrentStatus,
} from '../types';
import { formatSpeed } from '../utils/formatters';
import { error as logError, warn as logWarn } from '../utils/logger';

// ── helpers ────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`;
}

function formatEta(secs: number | null): string {
    if (secs === null || secs <= 0) return '—';
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}h ${m}m`;
}

function stateColor(state: string): string {
    switch (state) {
        case 'live': return 'text-emerald-400 bg-emerald-400/10';
        case 'paused': return 'text-amber-400 bg-amber-400/10';
        case 'initializing': return 'text-sky-400 bg-sky-400/10';
        case 'error': return 'text-red-400 bg-red-400/10';
        default: return 'text-slate-400 bg-slate-400/10';
    }
}

function stateIcon(state: string) {
    switch (state) {
        case 'live': return <Loader2 size={12} className="animate-spin text-emerald-400" />;
        case 'paused': return <Pause size={12} className="text-amber-400" />;
        case 'error': return <AlertCircle size={12} className="text-red-400" />;
        case 'initializing': return <Loader2 size={12} className="animate-spin text-sky-400" />;
        default: return <CheckCircle2 size={12} className="text-slate-400" />;
    }
}

function normalizePriority(priority: string): 'high' | 'normal' | 'low' {
    const v = priority.toLowerCase();
    if (v === 'high' || v === 'low' || v === 'normal') return v;
    return 'normal';
}

function nextPriority(priority: string): 'high' | 'normal' | 'low' {
    const p = normalizePriority(priority);
    if (p === 'normal') return 'high';
    if (p === 'high') return 'low';
    return 'normal';
}

function priorityStyle(priority: string): string {
    switch (normalizePriority(priority)) {
        case 'high':
            return 'text-rose-300 bg-rose-500/10 border-rose-500/30 hover:bg-rose-500/20';
        case 'low':
            return 'text-slate-400 bg-slate-600/10 border-slate-600/30 hover:bg-slate-600/20';
        default:
            return 'text-cyan-300 bg-cyan-500/10 border-cyan-500/30 hover:bg-cyan-500/20';
    }
}

function autoPauseReasonLabel(reason: string | null): string | null {
    if (!reason) return null;
    if (reason === 'queue') return 'Auto-paused by queue policy';
    if (reason === 'seeding_policy') return 'Auto-paused by seeding policy';
    return null;
}

function summarizeFailedTorrentNames(failedIds: number[], torrents: TorrentStatus[]): string {
    if (failedIds.length === 0) return 'unknown';
    const byId = new Map(torrents.map(t => [t.id, t.name]));
    const names = failedIds.map((id) => byId.get(id) ?? `#${id}`);
    const shortened = names.map((name) => (name.length > 24 ? `${name.slice(0, 21)}...` : name));
    const preview = shortened.slice(0, 2).join(', ');
    if (shortened.length <= 2) return preview;
    return `${preview}, +${shortened.length - 2} more`;
}

function actionFailureLabel(action: string): string {
    if (action === 'add_magnet') return 'Add magnet failed';
    if (action === 'add_torrent_file') return 'Add torrent file failed';
    if (action === 'add_magnet_config' || action === 'add_torrent_file_config') {
        return 'Post-add configuration failed';
    }
    if (action === 'settings_policy') return 'Policy enforcement failed after settings update';
    if (action === 'pause_all_policy') return 'Policy enforcement failed after pause-all';
    if (action === 'resume_all_policy') return 'Policy enforcement failed after resume-all';
    if (action === 'pause') return 'Pause failed';
    if (action === 'resume') return 'Resume failed';
    if (action === 'remove') return 'Remove failed';
    if (action === 'update_files') return 'File selection update failed';
    if (action === 'set_priority') return 'Priority update failed';
    if (action === 'set_pinned') return 'Pin update failed';
    if (action === 'add_magnet_policy' || action === 'add_torrent_file_policy') {
        return 'Post-add policy enforcement failed';
    }
    if (action.endsWith('_policy')) return 'Policy enforcement failed';
    return `Action "${action}" failed`;
}

function torrentNameForId(id: number | null, torrents: TorrentStatus[]): string {
    if (id === null) return 'new torrent';
    return torrents.find((t) => t.id === id)?.name ?? `#${id}`;
}

function formatTorrentActionErrorLine(
    entry: TorrentActionFailedEvent,
    torrents: TorrentStatus[],
    withMeta: boolean,
): string {
    const name = entry.id === null
        && (entry.action === 'settings_policy'
            || entry.action === 'pause_all_policy'
            || entry.action === 'resume_all_policy')
        ? 'global policy'
        : torrentNameForId(entry.id, torrents);
    const line = `${actionFailureLabel(entry.action)}: ${name} (${entry.error})`;
    if (!withMeta) return line;
    const timestamp = new Date(entry.timestamp_ms).toISOString();
    return `[${timestamp}] [${entry.category}/${entry.severity}] ${line}`;
}

function isWarningSeverity(entry: TorrentActionFailedEvent): boolean {
    return entry.severity.toLowerCase() === 'warning';
}

function matchesIssueFilter(entry: TorrentActionFailedEvent, filter: IssueFilter): boolean {
    if (filter === 'all') return true;
    if (filter === 'warnings') return isWarningSeverity(entry);
    return !isWarningSeverity(entry);
}

// ── confirm-delete modal ───────────────────────────────────────────────────

const DeleteConfirmModal: React.FC<{
    torrentName: string;
    onConfirm: (deleteFiles: boolean) => void;
    onCancel: () => void;
}> = ({ torrentName, onConfirm, onCancel }) => (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onCancel} />
        <motion.div
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            className="relative w-full max-w-sm bg-slate-900 border border-slate-700 rounded-xl p-6 shadow-2xl"
        >
            <div className="absolute top-0 left-0 w-full h-0.5 bg-gradient-to-r from-red-600 to-rose-600 rounded-t-xl" />
            <div className="flex items-start gap-3 mb-4">
                <Trash2 className="text-red-400 mt-0.5 shrink-0" size={20} />
                <div>
                    <p className="font-semibold text-white">Remove Torrent</p>
                    <p className="text-sm text-slate-400 mt-1 break-all">
                        "{torrentName}"
                    </p>
                </div>
            </div>
            <p className="text-sm text-slate-400 mb-5">
                Do you also want to delete downloaded files from disk?
            </p>
            <div className="flex flex-col gap-2">
                <button
                    onClick={() => onConfirm(true)}
                    className="w-full py-2.5 rounded-lg bg-red-700 hover:bg-red-600 text-white font-semibold text-sm transition-colors"
                >
                    Remove &amp; Delete Files
                </button>
                <button
                    onClick={() => onConfirm(false)}
                    className="w-full py-2.5 rounded-lg bg-slate-700 hover:bg-slate-600 text-white font-medium text-sm transition-colors"
                >
                    Remove Only (Keep Files)
                </button>
                <button
                    onClick={onCancel}
                    className="w-full py-2 text-slate-500 hover:text-slate-300 text-sm transition-colors"
                >
                    Cancel
                </button>
            </div>
        </motion.div>
    </div>
);

// ── per-file row ────────────────────────────────────────────────────────────

const FileRow: React.FC<{
    file: TorrentFileInfo;
    onToggle: (id: number, included: boolean) => void;
}> = ({ file, onToggle }) => (
    <div className="flex items-center gap-3 py-2 px-3 rounded-lg hover:bg-slate-700/30 transition-colors group">
        <button
            onClick={() => onToggle(file.id, !file.included)}
            className="shrink-0 text-slate-400 hover:text-teal-400 transition-colors"
            title={file.included ? 'Deselect file' : 'Select file'}
        >
            {file.included ? (
                <CheckSquare size={16} className="text-teal-400" />
            ) : (
                <Square size={16} />
            )}
        </button>
        <File size={14} className="shrink-0 text-slate-500" />
        <span className="flex-1 text-xs text-slate-300 truncate" title={file.name}>
            {file.name}
        </span>
        <span className="text-xs text-slate-500 shrink-0 w-16 text-right">
            {formatBytes(file.size)}
        </span>
        <div className="w-20 h-1.5 bg-slate-700 rounded-full overflow-hidden shrink-0">
            <div
                className="h-full bg-teal-500/70 rounded-full transition-all duration-500"
                style={{ width: `${file.progress_percent}%` }}
            />
        </div>
        <span className="text-xs text-slate-500 shrink-0 w-10 text-right">
            {file.progress_percent.toFixed(0)}%
        </span>
    </div>
);

// ── main torrent item ───────────────────────────────────────────────────────

const TorrentItem: React.FC<{
    status: TorrentStatus;
    onPlay: (id: number) => void;
    onRefresh: () => void;
}> = ({ status, onPlay, onRefresh }) => {
    const [expanded, setExpanded] = useState(false);
    const [files, setFiles] = useState<TorrentFileInfo[]>([]);
    const [loadingFiles, setLoadingFiles] = useState(false);
    const [confirmDelete, setConfirmDelete] = useState(false);
    const [actionLoading, setActionLoading] = useState<string | null>(null);

    const fetchFiles = useCallback(async () => {
        if (loadingFiles) return;
        setLoadingFiles(true);
        try {
            const f = await invoke<TorrentFileInfo[]>('get_torrent_files', { id: status.id });
            setFiles(f);
        } catch (e) {
            logError('Failed to fetch torrent files', e);
        } finally {
            setLoadingFiles(false);
        }
    }, [status.id, loadingFiles]);

    const handleExpand = () => {
        if (!expanded) fetchFiles();
        setExpanded(v => !v);
    };

    const withAction = async (key: string, fn: () => Promise<void>) => {
        setActionLoading(key);
        try {
            await fn();
            onRefresh();
        } catch (e) {
            logError(`Torrent action "${key}" failed`, e);
        } finally {
            setActionLoading(null);
        }
    };

    const handlePauseResume = () => {
        if (status.state === 'paused') {
            withAction('resume', () => invoke('resume_torrent', { id: status.id }));
        } else {
            withAction('pause', () => invoke('pause_torrent', { id: status.id }));
        }
    };

    const handleDelete = (deleteFiles: boolean) => {
        setConfirmDelete(false);
        withAction('remove', () => invoke('remove_torrent', { id: status.id, deleteFiles }));
    };

    const handleCyclePriority = () => {
        const next = nextPriority(status.priority);
        withAction('priority', () => invoke('set_torrent_priority', { id: status.id, priority: next }));
    };

    const handleTogglePinned = () => {
        withAction('pin', () => invoke('set_torrent_pinned', { id: status.id, pinned: !status.pinned }));
    };

    const handleToggleFile = async (fileId: number, included: boolean) => {
        const currentlyIncluded = files.filter(f => f.included).length;
        if (!included && currentlyIncluded <= 1) {
            return;
        }

        const newIncludes = files
            .map(f => (f.id === fileId ? { ...f, included } : f))
            .filter(f => f.included)
            .map(f => f.id);
        setFiles(prev => prev.map(f => (f.id === fileId ? { ...f, included } : f)));
        try {
            await invoke('update_torrent_files', { id: status.id, includedIds: newIncludes });
        } catch (e) {
            logError('Failed to update torrent file selection', e);
            fetchFiles(); // revert on error
        }
    };

    const isPaused = status.state === 'paused';
    const isError = status.state === 'error';

    // gradient bar: red for error, green for done, teal for downloading
    const barColor = isError
        ? 'bg-red-500'
        : status.finished
        ? 'bg-emerald-500'
        : 'bg-teal-500';

    return (
        <>
            <div className={`relative mb-2 rounded-xl border transition-all duration-200 ${
                isError
                    ? 'bg-red-950/20 border-red-800/30'
                    : 'bg-slate-800/50 border-slate-700/40 hover:border-slate-600/60'
            }`}>
                {/* Main row */}
                <div className="p-4">
                    <div className="flex items-start gap-3">
                        {/* Icon */}
                        <div className={`p-2.5 rounded-lg shrink-0 mt-0.5 ${
                            isError ? 'bg-red-500/10' : 'bg-teal-500/10'
                        }`}>
                            <Magnet
                                size={20}
                                className={isError ? 'text-red-400' : 'text-teal-400'}
                            />
                        </div>

                        {/* Content */}
                        <div className="flex-1 min-w-0">
                            {/* Title row */}
                            <div className="flex items-center justify-between gap-2 mb-1.5">
                                <p className="font-semibold text-slate-200 truncate text-sm">
                                    {status.name}
                                </p>
                                <div className="flex items-center gap-1.5 shrink-0">
                                    <button
                                        onClick={handleTogglePinned}
                                        disabled={actionLoading !== null}
                                        className={`flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide border px-2 py-0.5 rounded-full transition-colors disabled:opacity-50 ${
                                            status.pinned
                                                ? 'text-amber-300 bg-amber-500/10 border-amber-500/30 hover:bg-amber-500/20'
                                                : 'text-slate-400 bg-slate-600/10 border-slate-600/30 hover:bg-slate-600/20'
                                        }`}
                                        title={status.pinned ? 'Pinned (click to unpin)' : 'Pin torrent'}
                                    >
                                        <Pin size={10} />
                                        {status.pinned ? 'pinned' : 'pin'}
                                    </button>
                                    <button
                                        onClick={handleCyclePriority}
                                        disabled={actionLoading !== null}
                                        className={`flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide border px-2 py-0.5 rounded-full transition-colors disabled:opacity-50 ${priorityStyle(status.priority)}`}
                                        title={`Priority: ${normalizePriority(status.priority)} (click to cycle)`}
                                    >
                                        <Flag size={10} />
                                        {normalizePriority(status.priority)}
                                    </button>
                                    <div className={`flex items-center gap-1.5 text-xs font-mono px-2 py-0.5 rounded-full ${stateColor(status.state)}`}>
                                        {stateIcon(status.state)}
                                        {status.state}
                                    </div>
                                </div>
                            </div>

                            {/* Error message */}
                            {isError && status.error && (
                                <p className="text-xs text-red-400 mb-2 flex items-center gap-1">
                                    <AlertCircle size={12} /> {status.error}
                                </p>
                            )}
                            {!isError && status.state === 'paused' && autoPauseReasonLabel(status.auto_pause_reason) && (
                                <p className="text-xs text-amber-400/90 mb-2">
                                    {autoPauseReasonLabel(status.auto_pause_reason)}
                                </p>
                            )}

                            {/* Stats row */}
                            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-500 mb-2">
                                <span title="Download speed">
                                    ↓ <span className="text-slate-300">{formatSpeed(status.speed_download)}</span>
                                </span>
                                <span title="Upload speed">
                                    ↑ <span className="text-slate-300">{formatSpeed(status.speed_upload)}</span>
                                </span>
                                <span title="Active peers / total peers seen">
                                    ⇄ <span className="text-slate-300">{status.peers_live}</span>
                                    <span className="text-slate-600">/{status.peers_total}</span>
                                </span>
                                {status.ratio > 0 && (
                                    <span title="Upload ratio">
                                        ⟳ <span className="text-slate-300">{status.ratio.toFixed(2)}</span>
                                    </span>
                                )}
                                <span title="Total size">
                                    <HardDrive size={10} className="inline mr-0.5" />
                                    <span className="text-slate-300">{formatBytes(status.total_size)}</span>
                                </span>
                                {status.eta_secs !== null && status.state === 'live' && (
                                    <span title="ETA">ETA <span className="text-slate-300">{formatEta(status.eta_secs)}</span></span>
                                )}
                            </div>

                            {/* Progress bar */}
                            <div className="flex items-center gap-3">
                                <div className="flex-1 h-1.5 bg-slate-700/60 rounded-full overflow-hidden">
                                    <div
                                        className={`h-full ${barColor} rounded-full transition-all duration-500 ease-out`}
                                        style={{ width: `${status.progress_percent}%` }}
                                    />
                                </div>
                                <span className="text-xs font-medium text-slate-400 shrink-0 w-10 text-right font-mono">
                                    {status.progress_percent.toFixed(1)}%
                                </span>
                            </div>
                        </div>

                        {/* Action buttons */}
                        <div className="flex items-center gap-1.5 ml-1 shrink-0">
                            {/* Pause / Resume */}
                            {(status.state === 'live' || status.state === 'paused') && (
                                <button
                                    onClick={handlePauseResume}
                                    disabled={actionLoading !== null}
                                    className={`p-2 rounded-lg transition-colors ${
                                        isPaused
                                            ? 'bg-emerald-600/20 hover:bg-emerald-600/40 text-emerald-400'
                                            : 'bg-amber-600/20 hover:bg-amber-600/40 text-amber-400'
                                    }`}
                                    title={isPaused ? 'Resume' : 'Pause'}
                                >
                                    {actionLoading === 'pause' || actionLoading === 'resume' ? (
                                        <Loader2 size={16} className="animate-spin" />
                                    ) : isPaused ? (
                                        <RotateCcw size={16} />
                                    ) : (
                                        <Pause size={16} />
                                    )}
                                </button>
                            )}

                            {/* Stream / Play */}
                            <button
                                onClick={() => onPlay(status.id)}
                                className="p-2 bg-teal-600/20 hover:bg-teal-600/40 text-teal-400 rounded-lg transition-colors"
                                title="Stream largest file"
                            >
                                <Play size={16} fill="currentColor" />
                            </button>

                            {/* Open folder */}
                            <button
                                onClick={() => invoke('open_torrent_folder', { id: status.id })}
                                className="p-2 bg-slate-700/50 hover:bg-slate-600/50 text-slate-400 rounded-lg transition-colors"
                                title="Open save folder"
                            >
                                <FolderOpen size={16} />
                            </button>

                            {/* Delete */}
                            <button
                                onClick={() => setConfirmDelete(true)}
                                disabled={actionLoading !== null}
                                className="p-2 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded-lg transition-colors"
                                title="Remove torrent"
                            >
                                {actionLoading === 'remove' ? (
                                    <Loader2 size={16} className="animate-spin" />
                                ) : (
                                    <Trash2 size={16} />
                                )}
                            </button>

                            {/* Expand files */}
                            <button
                                onClick={handleExpand}
                                className="p-2 bg-slate-700/50 hover:bg-slate-600/50 text-slate-400 rounded-lg transition-colors"
                                title="Show files"
                            >
                                {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                            </button>
                        </div>
                    </div>
                </div>

                {/* Expandable file list */}
                <AnimatePresence>
                    {expanded && (
                        <motion.div
                            initial={{ height: 0, opacity: 0 }}
                            animate={{ height: 'auto', opacity: 1 }}
                            exit={{ height: 0, opacity: 0 }}
                            transition={{ duration: 0.2 }}
                            className="overflow-hidden"
                        >
                            <div className="border-t border-slate-700/40 px-4 py-3">
                                <div className="flex items-center justify-between mb-2">
                                    <p className="text-xs font-semibold text-slate-500 uppercase tracking-wider">
                                        Files ({files.length})
                                    </p>
                                    <p className="text-xs text-slate-600 font-mono">
                                        {formatBytes(status.downloaded)} / {formatBytes(status.total_size)}
                                    </p>
                                </div>
                                {loadingFiles ? (
                                    <p className="text-xs text-slate-500 py-2 flex items-center gap-2">
                                        <Loader2 size={12} className="animate-spin" />
                                        Loading files…
                                    </p>
                                ) : files.length > 0 ? (
                                    <div className="space-y-0.5 max-h-48 overflow-y-auto custom-scrollbar">
                                        {files.map(f => (
                                            <FileRow
                                                key={f.id}
                                                file={f}
                                                onToggle={handleToggleFile}
                                            />
                                        ))}
                                    </div>
                                ) : (
                                    <p className="text-xs text-slate-600 italic py-1">
                                        No file info available yet — metadata may still be fetching.
                                    </p>
                                )}
                            </div>
                        </motion.div>
                    )}
                </AnimatePresence>
            </div>

            {/* Delete confirmation */}
            <AnimatePresence>
                {confirmDelete && (
                    <DeleteConfirmModal
                        torrentName={status.name}
                        onConfirm={handleDelete}
                        onCancel={() => setConfirmDelete(false)}
                    />
                )}
            </AnimatePresence>
        </>
    );
};

// ── list container ─────────────────────────────────────────────────────────

interface TorrentListProps {
    onPlay: (id: number) => void;
}

type IssueFilter = 'all' | 'errors' | 'warnings';

interface RecentIssueUndoState {
    clearedCount: number;
    filter: IssueFilter;
    clearToken: number | null;
}

interface RecentIssueClearResult {
    removed_count: number;
    clear_token: number | null;
}

const ISSUE_FILTER_STORAGE_KEY = 'hyperstream_torrent_issue_filter';

function isIssueFilter(value: string | null): value is IssueFilter {
    return value === 'all' || value === 'errors' || value === 'warnings';
}

function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName.toLowerCase();
    return tag === 'input' || tag === 'textarea' || target.isContentEditable;
}

export const TorrentList: React.FC<TorrentListProps> = ({ onPlay }) => {
    const [torrents, setTorrents] = useState<TorrentStatus[]>([]);
    const [bulkActionLoading, setBulkActionLoading] = useState<'pause' | 'resume' | null>(null);
    const [bulkResult, setBulkResult] = useState<string | null>(null);
    const [recentErrors, setRecentErrors] = useState<TorrentActionFailedEvent[]>([]);
    const [recentIssueUndo, setRecentIssueUndo] = useState<RecentIssueUndoState | null>(null);
    const [recentIssueAction, setRecentIssueAction] = useState<'clear' | 'undo' | null>(null);
    const [issueFilter, setIssueFilter] = useState<IssueFilter>(() => {
        try {
            const raw = window.localStorage.getItem(ISSUE_FILTER_STORAGE_KEY);
            return isIssueFilter(raw) ? raw : 'all';
        } catch {
            return 'all';
        }
    });
    const fetchingRef = useRef(false);
    const mountedRef = useRef(true);
    const torrentsRef = useRef<TorrentStatus[]>([]);
    const bulkResultTimerRef = useRef<number | null>(null);

    useEffect(() => {
        torrentsRef.current = torrents;
    }, [torrents]);

    useEffect(() => {
        try {
            window.localStorage.setItem(ISSUE_FILTER_STORAGE_KEY, issueFilter);
        } catch {
            // Ignore storage failures and keep the in-memory value.
        }
    }, [issueFilter]);

    useEffect(() => {
        const onKeyDown = (e: KeyboardEvent) => {
            if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
            if (isTypingTarget(e.target)) return;

            const key = e.key.toLowerCase();
            if (key === 'a') {
                setIssueFilter('all');
            } else if (key === 'e') {
                setIssueFilter('errors');
            } else if (key === 'w') {
                setIssueFilter('warnings');
            } else {
                return;
            }
            e.preventDefault();
        };

        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, []);

    const reportBulkResult = useCallback((message: string) => {
        setBulkResult(message);
        if (bulkResultTimerRef.current !== null) {
            window.clearTimeout(bulkResultTimerRef.current);
        }
        bulkResultTimerRef.current = window.setTimeout(() => {
            setBulkResult(null);
            bulkResultTimerRef.current = null;
        }, 3000);
    }, []);

    const addRecentError = useCallback((entry: TorrentActionFailedEvent) => {
        setRecentErrors((prev) => [entry, ...prev].slice(0, 10));
    }, []);

    const loadRecentErrors = useCallback(async () => {
        try {
            const recent = await invoke<TorrentActionFailedEvent[]>('get_recent_torrent_errors');
            if (mountedRef.current) {
                setRecentErrors(recent.slice(0, 10));
            }
        } catch (e) {
            logError('Failed to load recent torrent errors', e);
        }
    }, []);

    const fetchTorrents = useCallback(async () => {
        if (fetchingRef.current || !mountedRef.current) return;
        fetchingRef.current = true;
        try {
            const list = await invoke<TorrentStatus[]>('get_torrents');
            if (mountedRef.current) setTorrents(list);
        } catch (e) {
            logError('Failed to fetch torrents', e);
        } finally {
            fetchingRef.current = false;
        }
    }, []);

    useEffect(() => {
        mountedRef.current = true;
        fetchTorrents();
        void loadRecentErrors();
        const interval = setInterval(() => {
            // Respect page visibility to save resources
            if (!document.hidden) fetchTorrents();
        }, 3000);
        return () => {
            mountedRef.current = false;
            clearInterval(interval);
            if (bulkResultTimerRef.current !== null) {
                window.clearTimeout(bulkResultTimerRef.current);
                bulkResultTimerRef.current = null;
            }
        };
    }, [fetchTorrents, loadRecentErrors]);

    useEffect(() => {
        let isDisposed = false;
        let unlisten: (() => void) | null = null;

        listen('torrents_refresh', () => {
            if (!isDisposed) {
                void fetchTorrents();
            }
        })
            .then((fn) => {
                if (isDisposed) {
                    fn();
                } else {
                    unlisten = fn;
                }
            })
            .catch((e) => {
                logError('Failed to subscribe to torrents_refresh', e);
            });

        return () => {
            isDisposed = true;
            if (unlisten) {
                unlisten();
            }
        };
    }, [fetchTorrents]);

    useEffect(() => {
        let isDisposed = false;
        let unlisten: (() => void) | null = null;

        listen<TorrentActionFailedEvent>('torrent_action_failed', (event) => {
            if (isDisposed) return;
            const payload = event.payload;
            const headline = formatTorrentActionErrorLine(payload, torrentsRef.current, false);
            reportBulkResult(headline);
            addRecentError(payload);
            if (isWarningSeverity(payload)) {
                logWarn('Torrent action warning', payload);
            } else {
                logError('Torrent action failed', payload);
            }
        })
            .then((fn) => {
                if (isDisposed) {
                    fn();
                } else {
                    unlisten = fn;
                }
            })
            .catch((e) => {
                logError('Failed to subscribe to torrent_action_failed', e);
            });

        return () => {
            isDisposed = true;
            if (unlisten) {
                unlisten();
            }
        };
    }, [addRecentError, reportBulkResult]);

    // Sort: active first, then paused, then errors, then done
    const sorted = [...torrents].sort((a, b) => {
        const order = { live: 0, initializing: 1, paused: 2, error: 3 };
        const oa = order[a.state as keyof typeof order] ?? 4;
        const ob = order[b.state as keyof typeof order] ?? 4;
        if (oa !== ob) return oa - ob;
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        const pOrder = { high: 0, normal: 1, low: 2 };
        const pa = pOrder[normalizePriority(a.priority)];
        const pb = pOrder[normalizePriority(b.priority)];
        if (pa !== pb) return pa - pb;
        return a.id - b.id;
    });

    const hasPausable = torrents.some(t => t.state === 'live');
    const hasResumable = torrents.some(t => t.state === 'paused' || t.state === 'error');
    const showSummaryBar = torrents.length > 0 || bulkResult !== null || recentErrors.length > 0 || recentIssueUndo !== null;
    const filteredRecentIssues = recentErrors.filter((entry) => matchesIssueFilter(entry, issueFilter));
    const warningIssueCount = recentErrors.filter((entry) => isWarningSeverity(entry)).length;
    const errorIssueCount = recentErrors.length - warningIssueCount;
    const latestRecentError = filteredRecentIssues[0] ?? null;
    const latestRecentErrorMessage = latestRecentError
        ? formatTorrentActionErrorLine(latestRecentError, torrents, false)
        : '';
    const latestRecentErrorTitle = latestRecentError
        ? formatTorrentActionErrorLine(latestRecentError, torrents, true)
        : '';
    const latestIssueIsWarning = latestRecentError ? isWarningSeverity(latestRecentError) : false;
    const isClearingRecentIssues = recentIssueAction === 'clear';
    const isUndoingRecentIssues = recentIssueAction === 'undo';
    const recentIssueActionBusy = recentIssueAction !== null;
    const recentIssueUndoTitle = recentIssueUndo === null
        ? ''
        : recentIssueUndo.filter === 'all'
        ? `Undo clear-all (${recentIssueUndo.clearedCount})`
        : `Undo clear ${recentIssueUndo.filter} (${recentIssueUndo.clearedCount})`;

    const handlePauseAll = useCallback(async () => {
        if (bulkActionLoading || !hasPausable) return;
        setBulkActionLoading('pause');
        try {
            const result = await invoke<TorrentBulkActionResult>('pause_all_torrents');
            const { attempted, succeeded, failed, failed_ids } = result;
            if (failed > 0) {
                const failedNames = summarizeFailedTorrentNames(failed_ids, torrents);
                reportBulkResult(`Paused ${succeeded}/${attempted} (failed: ${failedNames})`);
            } else {
                reportBulkResult(`Paused ${succeeded} torrent${succeeded === 1 ? '' : 's'}`);
            }
            await fetchTorrents();
        } catch (e) {
            logError('Failed to pause all torrents', e);
            reportBulkResult('Pause-all failed');
        } finally {
            setBulkActionLoading(null);
        }
    }, [bulkActionLoading, hasPausable, fetchTorrents, reportBulkResult, torrents]);

    const handleResumeAll = useCallback(async () => {
        if (bulkActionLoading || !hasResumable) return;
        setBulkActionLoading('resume');
        try {
            const result = await invoke<TorrentBulkActionResult>('resume_all_torrents');
            const { attempted, succeeded, failed, failed_ids } = result;
            if (failed > 0) {
                const failedNames = summarizeFailedTorrentNames(failed_ids, torrents);
                reportBulkResult(`Resumed ${succeeded}/${attempted} (failed: ${failedNames})`);
            } else {
                reportBulkResult(`Resumed ${succeeded} torrent${succeeded === 1 ? '' : 's'}`);
            }
            await fetchTorrents();
        } catch (e) {
            logError('Failed to resume all torrents', e);
            reportBulkResult('Resume-all failed');
        } finally {
            setBulkActionLoading(null);
        }
    }, [bulkActionLoading, hasResumable, fetchTorrents, reportBulkResult, torrents]);

    const handleClearRecentErrors = useCallback(async () => {
        if (recentIssueActionBusy) return;
        const toClearCount = filteredRecentIssues.length;
        if (toClearCount === 0) return;

        const filterToClear = issueFilter;
        setRecentIssueAction('clear');
        setRecentIssueUndo(null);
        setRecentErrors((prev) => {
            if (filterToClear === 'all') return [];
            return prev.filter((entry) => !matchesIssueFilter(entry, filterToClear));
        });

        try {
            const clearResult = await invoke<RecentIssueClearResult>('clear_recent_torrent_issues', {
                filter: filterToClear === 'all' ? null : filterToClear,
            });
            const removedCount = clearResult.removed_count;

            if (removedCount > 0) {
                setRecentIssueUndo({
                    clearedCount: removedCount,
                    filter: filterToClear,
                    clearToken: clearResult.clear_token ?? null,
                });
            }

            if (removedCount <= 0) {
                reportBulkResult('No recent issues were cleared');
            } else if (filterToClear === 'all') {
                reportBulkResult('Cleared all recent issues');
            } else {
                reportBulkResult(`Cleared ${removedCount} recent ${filterToClear}`);
            }
            await loadRecentErrors();
        } catch (e) {
            logError('Failed to clear recent torrent errors', e);
            await loadRecentErrors();
            reportBulkResult('Clear recent issues failed');
        } finally {
            setRecentIssueAction(null);
        }
    }, [filteredRecentIssues.length, issueFilter, loadRecentErrors, recentIssueActionBusy, reportBulkResult]);

    const handleUndoRecentIssueClear = useCallback(async () => {
        if (recentIssueActionBusy || recentIssueUndo === null) return;
        setRecentIssueAction('undo');
        try {
            const restoredCount = await invoke<number>('restore_recent_torrent_issues', {
                expectedToken: recentIssueUndo.clearToken ?? null,
            });
            await loadRecentErrors();
            if (restoredCount <= 0) {
                reportBulkResult('Nothing to restore');
            } else {
                reportBulkResult(`Restored ${restoredCount} recent issue${restoredCount === 1 ? '' : 's'}`);
            }
            setRecentIssueUndo(null);
        } catch (e) {
            logError('Failed to restore cleared recent torrent issues', e);
            reportBulkResult('Undo recent clear failed');
        } finally {
            setRecentIssueAction(null);
        }
    }, [loadRecentErrors, recentIssueActionBusy, recentIssueUndo, reportBulkResult]);

    const handleCopyRecentErrors = useCallback(async () => {
        if (filteredRecentIssues.length === 0) return;
        const lines = filteredRecentIssues.map((entry) => formatTorrentActionErrorLine(entry, torrents, true));
        try {
            await navigator.clipboard.writeText(lines.join('\n'));
            reportBulkResult(`Copied ${filteredRecentIssues.length} recent issue${filteredRecentIssues.length === 1 ? '' : 's'}`);
        } catch (e) {
            logError('Failed to copy recent torrent errors', e);
            reportBulkResult('Copy recent errors failed');
        }
    }, [filteredRecentIssues, reportBulkResult, torrents]);

    const handleCopyDiagnostics = useCallback(async () => {
        try {
            const diagnostics = await invoke<TorrentDiagnostics>('get_torrent_diagnostics');
            const filteredIssues = diagnostics.recent_errors.filter((entry) => {
                if (issueFilter === 'all') return true;
                if (issueFilter === 'warnings') return isWarningSeverity(entry);
                return !isWarningSeverity(entry);
            });
            const payload = {
                exported_at_iso: new Date().toISOString(),
                ui_context: {
                    issue_filter: issueFilter,
                    issue_counts: {
                        all: diagnostics.recent_errors.length,
                        errors: diagnostics.recent_failures.length,
                        warnings: diagnostics.recent_warnings.length,
                    },
                    filtered_issue_count: filteredIssues.length,
                },
                filtered_recent_issues: filteredIssues,
                filtered_recent_issue_lines: filteredIssues.map((entry) =>
                    formatTorrentActionErrorLine(entry, diagnostics.torrents, true),
                ),
                diagnostics,
            };
            await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
            reportBulkResult(
                `Copied diagnostics (${diagnostics.recent_error_count} errors, ${diagnostics.recent_warning_count} warnings, filter: ${issueFilter})`,
            );
        } catch (e) {
            logError('Failed to copy torrent diagnostics', e);
            reportBulkResult('Copy diagnostics failed');
        }
    }, [issueFilter, reportBulkResult]);

    return (
        <div className="w-full h-full flex flex-col overflow-hidden">
            {/* Summary bar */}
            {showSummaryBar && (
                <div className="border-b border-slate-800/60">
                    <div className="px-4 pt-3 pb-2 flex items-center gap-4 text-xs text-slate-500">
                        <span>{torrents.length} torrent{torrents.length !== 1 ? 's' : ''}</span>
                        <span>↓ {formatSpeed(torrents.reduce((s, t) => s + t.speed_download, 0))}</span>
                        <span>↑ {formatSpeed(torrents.reduce((s, t) => s + t.speed_upload, 0))}</span>
                        {bulkResult && (
                            <span className="text-cyan-400">{bulkResult}</span>
                        )}
                        <div className="ml-auto flex items-center gap-2">
                            <button
                                onClick={handlePauseAll}
                                disabled={bulkActionLoading !== null || !hasPausable}
                                className="px-2 py-1 rounded bg-amber-600/20 text-amber-400 border border-amber-700/40 disabled:opacity-40 disabled:cursor-not-allowed hover:bg-amber-600/30 transition-colors"
                            >
                                {bulkActionLoading === 'pause' ? 'Pausing…' : 'Pause all'}
                            </button>
                            <button
                                onClick={handleResumeAll}
                                disabled={bulkActionLoading !== null || !hasResumable}
                                className="px-2 py-1 rounded bg-emerald-600/20 text-emerald-400 border border-emerald-700/40 disabled:opacity-40 disabled:cursor-not-allowed hover:bg-emerald-600/30 transition-colors"
                            >
                                {bulkActionLoading === 'resume' ? 'Resuming…' : 'Resume all'}
                            </button>
                            <button
                                onClick={handleCopyDiagnostics}
                                className="px-2 py-1 rounded bg-slate-700/30 text-slate-300 border border-slate-600/50 hover:bg-slate-700/50 transition-colors"
                            >
                                Copy diagnostics
                            </button>
                            <span>{torrents.filter(t => t.finished).length} completed</span>
                        </div>
                    </div>
                    {(recentErrors.length > 0 || recentIssueUndo !== null) && (
                        <div className="px-4 pb-2 flex items-center gap-2 text-xs">
                            <span className={`${latestIssueIsWarning ? 'text-amber-300' : 'text-red-300'} shrink-0`}>Recent issues:</span>
                            <div className="flex items-center gap-1 shrink-0">
                                <button
                                    type="button"
                                    onClick={() => setIssueFilter('all')}
                                    className={`px-1.5 py-0.5 rounded border transition-colors ${
                                        issueFilter === 'all'
                                            ? 'text-slate-200 border-slate-500 bg-slate-700/40'
                                            : 'text-slate-500 border-slate-700 hover:text-slate-300'
                                    }`}
                                >
                                    all {recentErrors.length}
                                </button>
                                <button
                                    type="button"
                                    onClick={() => setIssueFilter('errors')}
                                    className={`px-1.5 py-0.5 rounded border transition-colors ${
                                        issueFilter === 'errors'
                                            ? 'text-red-300 border-red-600/60 bg-red-900/20'
                                            : 'text-slate-500 border-slate-700 hover:text-slate-300'
                                    }`}
                                >
                                    errors {errorIssueCount}
                                </button>
                                <button
                                    type="button"
                                    onClick={() => setIssueFilter('warnings')}
                                    className={`px-1.5 py-0.5 rounded border transition-colors ${
                                        issueFilter === 'warnings'
                                            ? 'text-amber-300 border-amber-600/60 bg-amber-900/20'
                                            : 'text-slate-500 border-slate-700 hover:text-slate-300'
                                    }`}
                                >
                                    warnings {warningIssueCount}
                                </button>
                            </div>
                            <span className="text-[11px] text-slate-500 shrink-0">A/E/W</span>
                            <span
                                className={`${latestIssueIsWarning ? 'text-amber-400' : 'text-red-400'} truncate`}
                                title={latestRecentErrorTitle}
                            >
                                {latestRecentErrorMessage || 'No issues for current filter'}
                            </span>
                            <button
                                type="button"
                                onClick={handleCopyRecentErrors}
                                disabled={filteredRecentIssues.length === 0 || recentIssueActionBusy}
                                className="ml-auto text-slate-500 hover:text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                            >
                                copy
                            </button>
                            <button
                                type="button"
                                onClick={handleClearRecentErrors}
                                disabled={filteredRecentIssues.length === 0 || recentIssueActionBusy}
                                className="text-slate-500 hover:text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                            >
                                {isClearingRecentIssues ? 'clearing...' : `clear ${issueFilter === 'all' ? 'all' : issueFilter}`}
                            </button>
                            <button
                                type="button"
                                onClick={handleUndoRecentIssueClear}
                                disabled={recentIssueUndo === null || recentIssueActionBusy}
                                className="text-slate-500 hover:text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                                title={recentIssueUndoTitle}
                            >
                                {isUndoingRecentIssues ? 'undoing...' : 'undo'}
                            </button>
                        </div>
                    )}
                    {filteredRecentIssues.length > 1 && (
                        <div className="px-4 pb-2 text-[11px] text-slate-500 truncate">
                            +{filteredRecentIssues.length - 1} more recent issue{filteredRecentIssues.length > 2 ? 's' : ''}
                        </div>
                    )}
                </div>
            )}

            <div className="flex-1 overflow-y-auto p-4 custom-scrollbar space-y-1">
                {sorted.length > 0 ? (
                    sorted.map(t => (
                        <TorrentItem
                            key={t.id}
                            status={t}
                            onPlay={onPlay}
                            onRefresh={fetchTorrents}
                        />
                    ))
                ) : (
                    <div className="flex flex-col items-center justify-center h-64 text-slate-500">
                        <div className="p-4 bg-slate-800/50 rounded-full mb-4">
                            <Magnet size={48} className="text-slate-700" />
                        </div>
                        <p className="text-lg font-medium">No Active Torrents</p>
                        <p className="text-sm opacity-70 mt-1">
                            Paste a magnet link or drop a .torrent file to begin
                        </p>
                    </div>
                )}
            </div>
        </div>
    );
};
