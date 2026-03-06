import React, { useCallback, useMemo, useState } from 'react';
import { DownloadItem } from './DownloadItem';
import type { DownloadTask } from '../types';
import { Virtuoso } from 'react-virtuoso';
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
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    downloadDir: string;
}

export const DownloadList: React.FC<DownloadListProps> = ({ tasks, onPause, onResume, onDelete, onMoveUp, onMoveDown, downloadDir }) => {
    const [search, setSearch] = useState('');
    const [sortField, setSortField] = useState<SortField | null>(null);
    const [sortDir, setSortDir] = useState<SortDir>('asc');
    const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');

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
            result = result.filter(t => t.filename.toLowerCase().includes(q) || (t.url && t.url.toLowerCase().includes(q)));
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

    const itemContent = useCallback((_index: number, task: DownloadTask) => {
        return (
            <div style={{ paddingBottom: '8px', paddingLeft: '5px', paddingRight: '5px' }}>
                <DownloadItem
                    task={task}
                    onPause={onPause}
                    onResume={onResume}
                    onDelete={onDelete}
                    onMoveUp={onMoveUp}
                    onMoveDown={onMoveDown}
                    downloadDir={downloadDir}
                />
            </div>
        );
    }, [onPause, onResume, onDelete, onMoveUp, onMoveDown, downloadDir]);

    if (tasks.length === 0) {
        return (
            <motion.div
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                className="flex flex-col items-center justify-center h-full text-slate-500 opacity-60"
            >
                <div className="p-6 bg-slate-800/30 rounded-full mb-4 border border-slate-700/30">
                    <Inbox size={48} className="text-slate-400" />
                </div>
                <h3 className="text-lg font-semibold text-slate-300">No Downloads Yet</h3>
                <p className="text-sm max-w-xs text-center mt-2">
                    Click the "Add Download" button to start downloading files.
                </p>
            </motion.div>
        );
    }

    const SortBtn = ({ field, label }: { field: SortField; label: string }) => {
        const active = sortField === field;
        return (
            <button
                onClick={() => toggleSort(field)}
                className={`flex items-center gap-1 px-2 py-1 rounded text-xs font-medium transition-colors ${
                    active ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30' : 'text-slate-400 hover:text-slate-200 border border-transparent'
                }`}
            >
                {label}
                {active ? (sortDir === 'asc' ? <ArrowUp size={11} /> : <ArrowDown size={11} />) : <ArrowUpDown size={11} className="opacity-40" />}
            </button>
        );
    };

    const hasFilters = search || statusFilter !== 'all' || sortField;

    return (
        <div className="flex flex-col h-full">
            {/* Toolbar */}
            <div className="px-3 pb-2 flex flex-col gap-2 shrink-0">
                {/* Search + Sort Row */}
                <div className="flex items-center gap-2">
                    <div className="relative flex-1 max-w-xs">
                        <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500" />
                        <input
                            type="text"
                            value={search}
                            onChange={e => setSearch(e.target.value)}
                            placeholder="Search downloads..."
                            className="w-full bg-slate-800/50 border border-slate-700/50 rounded-lg pl-8 pr-7 py-1.5 text-xs text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-cyan-500/40"
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
                <div className="flex items-center gap-1">
                    {(['all', 'Downloading', 'Paused', 'Done', 'Error'] as StatusFilter[]).map(s => {
                        const count = statusCounts[s] || 0;
                        if (s !== 'all' && count === 0) return null;
                        const labels: Record<StatusFilter, string> = { all: 'All', Downloading: 'Active', Paused: 'Paused', Done: 'Complete', Error: 'Failed' };
                        const colors: Record<StatusFilter, string> = {
                            all: 'text-slate-300 bg-slate-700/40',
                            Downloading: 'text-cyan-300 bg-cyan-500/15',
                            Paused: 'text-amber-300 bg-amber-500/15',
                            Done: 'text-emerald-300 bg-emerald-500/15',
                            Error: 'text-red-300 bg-red-500/15',
                        };
                        return (
                            <button
                                key={s}
                                onClick={() => setStatusFilter(s)}
                                className={`px-2.5 py-1 rounded-full text-xs font-medium transition-colors ${
                                    statusFilter === s ? colors[s] + ' ring-1 ring-current/30' : 'text-slate-500 hover:text-slate-300'
                                }`}
                            >
                                {labels[s]} {count > 0 && <span className="ml-1 opacity-60">{count}</span>}
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
