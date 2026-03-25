import { useState, useMemo } from 'react';
import type { DownloadTask } from '../types';

export type SortField = 'name' | 'size' | 'speed' | 'progress' | 'status';
export type SortDir = 'asc' | 'desc';

const STATUS_ORDER: Record<string, number> = { Downloading: 0, Paused: 1, Error: 2, Done: 3 };

export const useSort = (tasks: DownloadTask[]) => {
    const [sortField, setSortField] = useState<SortField | null>(null);
    const [sortDir, setSortDir] = useState<SortDir>('asc');

    const toggleSort = (field: SortField) => {
        if (sortField === field) {
            if (sortDir === 'asc') setSortDir('desc');
            else { setSortField(null); setSortDir('asc'); }
        } else {
            setSortField(field);
            setSortDir(field === 'speed' || field === 'size' ? 'desc' : 'asc');
        }
    };

    const sortedTasks = useMemo(() => {
        if (!sortField) return tasks;

        const dir = sortDir === 'asc' ? 1 : -1;
        return [...tasks].sort((a, b) => {
            switch (sortField) {
                case 'name': return dir * a.filename.localeCompare(b.filename);
                case 'size': return dir * ((a.total || 0) - (b.total || 0));
                case 'speed': return dir * ((a.speed || 0) - (b.speed || 0));
                case 'progress': return dir * ((a.progress || 0) - (b.progress || 0));
                case 'status': return dir * ((STATUS_ORDER[a.status] ?? 9) - (STATUS_ORDER[b.status] ?? 9));
                default: return 0;
            }
        });
    }, [tasks, sortField, sortDir]);

    return {
        sortField,
        sortDir,
        toggleSort,
        sortedTasks,
    };
};
