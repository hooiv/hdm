import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { DownloadItem } from './DownloadItem';
import type { DiscoveredMirror, DownloadTask } from '../types';
import { Virtuoso, type VirtuosoHandle } from 'react-virtuoso';
import { Inbox, Search, ArrowUpDown, ArrowUp, ArrowDown, X } from 'lucide-react';
import { motion } from 'framer-motion';

type SortField = 'name' | 'size' | 'speed' | 'progress' | 'status';
type SortDir = 'asc' | 'desc';
type StatusFilter = 'all' | 'Downloading' | 'Paused' | 'Done' | 'Error';

const STATUS_ORDER: Record<string, number> = { Downloading: 0, Paused: 1, Error: 2, Done: 3 };

interface DownloadListProps {
    tasks: DownloadTask[];
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDiscoveredMirrors?: (id: string, mirrors: DiscoveredMirror[]) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    downloadDir: string;
    spotlightRequest?: {
        taskId: string;
        token: number;
    } | null;
}

const DOWNLOAD_SPOTLIGHT_MS = 2500;

export const DownloadList: React.FC<DownloadListProps> = ({ tasks, onPause, onResume, onDiscoveredMirrors, onDelete, onMoveUp, onMoveDown, downloadDir, spotlightRequest }) => {
    const [search, setSearch] = useState('');
    const [sortField, setSortField] = useState<SortField | null>(null);
    const [sortDir, setSortDir] = useState<SortDir>('asc');
    const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
    const [spotlightedTaskId, setSpotlightedTaskId] = useState<string | null>(null);
    const virtuosoRef = useRef<VirtuosoHandle | null>(null);
    const spotlightTimerRef = useRef<number | null>(null);
    const lastActivatedSpotlightTokenRef = useRef<number | null>(null);
    const lastScrolledSpotlightTokenRef = useRef<number | null>(null);

    const toggleSort = (field: SortField) => {
        if (sortField === field) {
            if (sortDir === 'asc') setSortDir('desc');
            else { setSortField(null); setSortDir('asc'); }
        } else {
            setSortField(field);
            setSortDir(field === 'speed' || field === 'size' ? 'desc' : 'asc');
        }
    };

    const filtered = useMemo(() => {
        let result = tasks;
        if (statusFilter !== 'all') {
            result = result.filter(t => t.status === statusFilter);
        }
        if (search.trim()) {
            const q = search.toLowerCase();
            result = result.filter(t => t.filename.toLowerCase().includes(q) || (t.url?.toLowerCase().includes(q) ?? false));
        }
        if (sortField) {
            const dir = sortDir === 'asc' ? 1 : -1;
            result = [...result].sort((a, b) => {
                switch (sortField) {
                    case 'name': return dir * a.filename.localeCompare(b.filename);
                    case 'size': return dir * ((a.total || 0) - (b.total || 0));
                    case 'speed': return dir * ((a.speed || 0) - (b.speed || 0));
                    case 'progress': return dir * ((a.progress || 0) - (b.progress || 0));
                    case 'status': return dir * ((STATUS_ORDER[a.status] ?? 9) - (STATUS_ORDER[b.status] ?? 9));
                    default: return 0;
                }
            });
        }
        return result;
    }, [tasks, statusFilter, search, sortField, sortDir]);

    const statusCounts = useMemo(() => {
        const counts: Record<string, number> = { all: tasks.length, Downloading: 0, Paused: 0, Done: 0, Error: 0 };
        tasks.forEach(t => { counts[t.status] = (counts[t.status] || 0) + 1; });
        return counts;
    }, [tasks]);

    useEffect(() => {
        if (!spotlightRequest) return;

        const task = tasks.find((candidate) => candidate.id === spotlightRequest.taskId);
        if (!task) return;

        const searchQuery = search.trim().toLowerCase();
        const matchesSearch = !searchQuery
            || task.filename.toLowerCase().includes(searchQuery)
            || Boolean(task.url?.toLowerCase().includes(searchQuery));

        if (searchQuery && !matchesSearch) {
            setSearch('');
        }

        if (statusFilter !== 'all' && task.status !== statusFilter) {
            setStatusFilter('all');
        }

        if (lastActivatedSpotlightTokenRef.current === spotlightRequest.token) {
            return;
        }

        lastActivatedSpotlightTokenRef.current = spotlightRequest.token;
        setSpotlightedTaskId(spotlightRequest.taskId);

        if (spotlightTimerRef.current) {
            window.clearTimeout(spotlightTimerRef.current);
        }

        spotlightTimerRef.current = window.setTimeout(() => {
            setSpotlightedTaskId((current) => current === spotlightRequest.taskId ? null : current);
        }, DOWNLOAD_SPOTLIGHT_MS);
    }, [search, spotlightRequest, statusFilter, tasks]);

    useEffect(() => () => {
        if (spotlightTimerRef.current) {
            window.clearTimeout(spotlightTimerRef.current);
        }
    }, []);

    useEffect(() => {
        if (!spotlightRequest) return;

        const spotlightIndex = filtered.findIndex((task) => task.id === spotlightRequest.taskId);
        if (spotlightIndex < 0 || lastScrolledSpotlightTokenRef.current === spotlightRequest.token) {
            return;
        }

        const listHandle = virtuosoRef.current;
        if (!listHandle) return;

        lastScrolledSpotlightTokenRef.current = spotlightRequest.token;
        listHandle.scrollToIndex({ index: spotlightIndex, align: 'center' });
    }, [filtered, spotlightRequest]);

    const itemContent = useCallback((_index: number, task: DownloadTask) => {
        return (
            <div style={{ paddingBottom: '8px', paddingLeft: '5px', paddingRight: '5px' }}>
                <DownloadItem
                    task={task}
                    onPause={onPause}
                    onResume={onResume}
                    onDiscoveredMirrors={onDiscoveredMirrors}
                    onDelete={onDelete}
                    onMoveUp={onMoveUp}
                    onMoveDown={onMoveDown}
                    downloadDir={downloadDir}
                    isSpotlighted={task.id === spotlightedTaskId}
                />
            </div>
        );
    }, [downloadDir, onDelete, onDiscoveredMirrors, onMoveDown, onMoveUp, onPause, onResume, spotlightedTaskId]);

    if (tasks.length === 0) {
        return (
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="flex flex-col items-center justify-center h-full p-8"
            >
                <div className="relative mb-8">
                    <div className="absolute inset-0 bg-cyan-500/20 blur-[60px] rounded-full" />
                    <div className="relative p-8 bg-white/5 rounded-full border border-white/5 backdrop-blur-3xl shadow-[0_0_50px_rgba(0,0,0,0.5)]">
                        <Inbox size={64} strokeWidth={1} className="text-cyan-400/60" />
                    </div>
                </div>
                <h3 className="display-lg text-white mb-2 scale-75 origin-center text-center opacity-80">STATION IDLE</h3>
                <p className="text-xs font-bold tracking-[0.2em] text-slate-600 uppercase mb-8 text-center ml-2">
                    Waiting for data transit directives
                </p>
                <motion.button
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.95 }}
                    className="bg-white/5 hover:bg-white/10 text-cyan-400 px-8 py-3 rounded-xl text-[10px] font-black uppercase tracking-widest border border-cyan-500/20 transition-all"
                >
                    Add link to begin
                </motion.button>
            </motion.div>
        );
    }

    const SortBtn = ({ field, label }: { field: SortField; label: string }) => {
        const active = sortField === field;
        return (
            <button
                onClick={() => toggleSort(field)}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all ${
                    active ? 'bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 shadow-[0_0_15px_rgba(0,242,255,0.1)]' : 'text-slate-600 hover:text-slate-400 border border-transparent'
                }`}
            >
                {label}
                {active ? (sortDir === 'asc' ? <ArrowUp size={10} strokeWidth={3} /> : <ArrowDown size={10} strokeWidth={3} />) : <ArrowUpDown size={10} strokeWidth={3} className="opacity-20" />}
            </button>
        );
    };

    const hasFilters = search || statusFilter !== 'all' || sortField;

    return (
        <div className="flex flex-col h-full bg-transparent">
            {/* Toolbar - Kinetic Design */}
            <div className="px-6 py-4 flex flex-col gap-4 shrink-0 bg-white/[0.01] backdrop-blur-sm">
                <div className="flex items-center gap-4">
                    <div className="relative flex-1">
                        <Search size={14} className="absolute left-4 top-1/2 -translate-y-1/2 text-slate-600" />
                        <input
                            type="text"
                            value={search}
                            onChange={e => setSearch(e.target.value)}
                            placeholder="SEARCH TRANSIT HUB..."
                            className="w-full bg-white/[0.03] border border-white/5 rounded-xl pl-12 pr-10 py-2.5 text-xs text-slate-200 placeholder:text-slate-700 focus:outline-none focus:border-cyan-500/30 focus:bg-white/[0.05] tracking-widest font-bold transition-all"
                        />
                        {search && (
                            <button onClick={() => setSearch('')} className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300">
                                <X size={12} />
                            </button>
                        )}
                    </div>
                    <div className="flex items-center gap-1">
                        <SortBtn field="name" label="Name" />
                        <SortBtn field="size" label="Size" />
                        <SortBtn field="speed" label="Speed" />
                        <SortBtn field="progress" label="Progress" />
                        <SortBtn field="status" label="Status" />
                    </div>
                    {hasFilters && (
                        <button
                            onClick={() => { setSearch(''); setSortField(null); setSortDir('asc'); setStatusFilter('all'); }}
                            className="text-xs text-slate-500 hover:text-red-400 transition-colors ml-1"
                        >
                            Clear
                        </button>
                    )}
                </div>
                {/* Status Filter Tabs */}
                <div className="flex items-center gap-2">
                    {(['all', 'Downloading', 'Paused', 'Done', 'Error'] as StatusFilter[]).map(s => {
                        const count = statusCounts[s] || 0;
                        if (s !== 'all' && count === 0) return null;
                        const labels: Record<StatusFilter, string> = { all: 'All Hubs', Downloading: 'Active', Paused: 'Suspended', Done: 'Archive', Error: 'Failed' };
                        const colors: Record<StatusFilter, string> = {
                            all: 'text-slate-400 bg-white/5',
                            Downloading: 'text-cyan-400 bg-cyan-500/10 shadow-[0_0_15px_rgba(0,242,255,0.1)]',
                            Paused: 'text-amber-400 bg-amber-500/10',
                            Done: 'text-emerald-400 bg-emerald-500/10',
                            Error: 'text-red-400 bg-red-500/10',
                        };
                        return (
                            <button
                                key={s}
                                onClick={() => setStatusFilter(s)}
                                className={`px-4 py-2 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all ${
                                    statusFilter === s ? colors[s] + ' border border-current/20' : 'text-slate-600 hover:text-slate-400 hover:bg-white/5'
                                }`}
                            >
                                {labels[s]} {count > 0 && <span className="ml-1 opacity-40 font-mono">{count}</span>}
                            </button>
                        );
                    })}
                    {filtered.length !== tasks.length && (
                        <span className="text-xs text-slate-500 ml-auto">
                            {filtered.length} of {tasks.length}
                        </span>
                    )}
                </div>
            </div>
            {/* List */}
            {filtered.length > 0 ? (
                <Virtuoso
                    ref={virtuosoRef}
                    style={{ height: '100%', width: '100%' }}
                    data={filtered}
                    itemContent={itemContent}
                    computeItemKey={(_, task) => task.id}
                    alignToBottom={false}
                    overscan={200}
                />
            ) : (
                <div className="flex-1 flex flex-col items-center justify-center text-slate-500 opacity-60">
                    <p className="text-sm">No downloads match your filters.</p>
                    <button
                        onClick={() => { setSearch(''); setStatusFilter('all'); setSortField(null); }}
                        className="text-xs text-cyan-400 hover:text-cyan-300 mt-2"
                    >
                        Reset filters
                    </button>
                </div>
            )}
        </div>
    );
};
