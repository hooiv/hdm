import { useState, useMemo } from 'react';
import type { DownloadTask } from '../types';

export type StatusFilter = 'all' | 'Downloading' | 'Paused' | 'Done' | 'Error';

export const useFilter = (tasks: DownloadTask[]) => {
    const [search, setSearch] = useState('');
    const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');

    const filteredTasks = useMemo(() => {
        let result = tasks;
        if (statusFilter !== 'all') {
            result = result.filter(t => t.status === statusFilter);
        }
        if (search.trim()) {
            const q = search.toLowerCase();
            result = result.filter(t => t.filename.toLowerCase().includes(q) || (t.url?.toLowerCase().includes(q) ?? false));
        }
        return result;
    }, [tasks, statusFilter, search]);

    return {
        search,
        setSearch,
        statusFilter,
        setStatusFilter,
        filteredTasks,
    };
};
