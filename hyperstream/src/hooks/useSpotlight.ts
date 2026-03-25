import { useState, useEffect, useRef } from 'react';
import type { DownloadTask } from '../types';

const DOWNLOAD_SPOTLIGHT_MS = 2500;

export const useSpotlight = (
    tasks: DownloadTask[],
    spotlightRequest?: { taskId: string; token: number } | null,
    search?: string,
    setSearch?: (search: string) => void,
    statusFilter?: string,
    setStatusFilter?: (status: any) => void,
    filteredTasks?: DownloadTask[],
    virtuosoRef?: React.RefObject<any>
) => {
    const [spotlightedTaskId, setSpotlightedTaskId] = useState<string | null>(null);
    const spotlightTimerRef = useRef<number | null>(null);
    const lastActivatedSpotlightTokenRef = useRef<number | null>(null);
    const lastScrolledSpotlightTokenRef = useRef<number | null>(null);

    useEffect(() => {
        if (!spotlightRequest) return;

        const task = tasks.find((candidate) => candidate.id === spotlightRequest.taskId);
        if (!task) return;

        const searchQuery = search?.trim().toLowerCase();
        const matchesSearch = !searchQuery
            || task.filename.toLowerCase().includes(searchQuery)
            || Boolean(task.url?.toLowerCase().includes(searchQuery));

        if (searchQuery && !matchesSearch) {
            setSearch?.('');
        }

        if (statusFilter !== 'all' && task.status !== statusFilter) {
            setStatusFilter?.('all');
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
    }, [search, spotlightRequest, statusFilter, tasks, setSearch, setStatusFilter]);

    useEffect(() => () => {
        if (spotlightTimerRef.current) {
            window.clearTimeout(spotlightTimerRef.current);
        }
    }, []);

    useEffect(() => {
        if (!spotlightRequest) return;

        const spotlightIndex = filteredTasks?.findIndex((task) => task.id === spotlightRequest.taskId);
        if (spotlightIndex === undefined || spotlightIndex < 0 || lastScrolledSpotlightTokenRef.current === spotlightRequest.token) {
            return;
        }

        const listHandle = virtuosoRef?.current;
        if (!listHandle) return;

        lastScrolledSpotlightTokenRef.current = spotlightRequest.token;
        listHandle.scrollToIndex({ index: spotlightIndex, align: 'center' });
    }, [filteredTasks, spotlightRequest, virtuosoRef]);

    return spotlightedTaskId;
};
