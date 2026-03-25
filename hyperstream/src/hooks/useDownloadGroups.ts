/**
 * React hooks for Download Groups management.
 * 
 * Provides:
 * - useDownloadGroups(): Main hook with 8 methods for group operations
 * - useGroupMetrics(): Auto-updating metrics with refresh interval
 * - useGroupMembers(): Auto-updating members with refresh interval
 */

import { useCallback, useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { GroupResponse, MemberResponse } from '../types';

// ============ useDownloadGroups Hook ============

/**
 * Main hook for download groups management.
 * 
 * Provides methods for:
 * - Creating groups
 * - Adding members with dependencies
 * - Starting/pausing groups
 * - Updating progress and completion
 * - Listing all groups
 */
export function useDownloadGroups() {
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const createGroup = useCallback(async (name: string): Promise<GroupResponse> => {
        setIsLoading(true);
        setError(null);
        try {
            const response = await invoke<GroupResponse>('create_download_group', { name });
            return response;
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
            throw new Error(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const addMember = useCallback(
        async (groupId: string, url: string): Promise<string> => {
            setIsLoading(true);
            setError(null);
            try {
                const memberId = await invoke<string>('add_member_to_group', {
                    group_id: groupId,
                    url,
                    dependencies: null,
                });
                return memberId;
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                setError(errorMsg);
                throw new Error(errorMsg);
            } finally {
                setIsLoading(false);
            }
        },
        []
    );

    const addDependency = useCallback(
        async (groupId: string, dependentId: string, prerequisiteId: string): Promise<void> => {
            setIsLoading(true);
            setError(null);
            try {
                await invoke<void>('add_group_dependency', {
                    group_id: groupId,
                    dependent_id: dependentId,
                    prerequisite_id: prerequisiteId,
                });
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                setError(errorMsg);
                throw new Error(errorMsg);
            } finally {
                setIsLoading(false);
            }
        },
        []
    );

    const startGroup = useCallback(async (groupId: string): Promise<void> => {
        setIsLoading(true);
        setError(null);
        try {
            await invoke<void>('start_group_download', { group_id: groupId });
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
            throw new Error(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const pauseGroup = useCallback(async (groupId: string): Promise<void> => {
        setIsLoading(true);
        setError(null);
        try {
            await invoke<void>('pause_group_download', { group_id: groupId });
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
            throw new Error(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const updateMemberProgress = useCallback(
        async (groupId: string, memberId: string, percent: number): Promise<void> => {
            setError(null);
            try {
                const clampedPercent = Math.min(Math.max(percent, 0), 100);
                await invoke<void>('update_member_progress', {
                    group_id: groupId,
                    member_id: memberId,
                    progress_percent: clampedPercent,
                });
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                setError(errorMsg);
                throw new Error(errorMsg);
            }
        },
        []
    );

    const completeMember = useCallback(
        async (groupId: string, memberId: string): Promise<void> => {
            setIsLoading(true);
            setError(null);
            try {
                await invoke<void>('complete_group_member', {
                    group_id: groupId,
                    member_id: memberId,
                });
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                setError(errorMsg);
                throw new Error(errorMsg);
            } finally {
                setIsLoading(false);
            }
        },
        []
    );

    const listGroups = useCallback(async (): Promise<GroupResponse[]> => {
        setIsLoading(true);
        setError(null);
        try {
            const groups = await invoke<GroupResponse[]>('list_all_groups');
            return groups;
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
            throw new Error(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, []);

    return {
        createGroup,
        addMember,
        addDependency,
        startGroup,
        pauseGroup,
        updateMemberProgress,
        completeMember,
        listGroups,
        isLoading,
        error,
    };
}

// ============ useGroupMetrics Hook ============

interface GroupMetrics {
    overall_progress: number;
    completed_count: number;
    total_count: number;
    state: string;
    isComplete: boolean;
}

/**
 * Hook for auto-updating group metrics.
 * 
 * Fetches metrics at specified interval and auto-refreshes.
 * Returns current metrics and cleanup on unmount.
 * 
 * @param groupId - ID of the group to monitor
 * @param refreshIntervalMs - Refresh interval in milliseconds (default: 2000)
 */
export function useGroupMetrics(
    groupId: string,
    refreshIntervalMs: number = 2000
): GroupMetrics & { isLoading: boolean; error: string | null } {
    const [metrics, setMetrics] = useState<GroupMetrics>({
        overall_progress: 0,
        completed_count: 0,
        total_count: 0,
        state: 'Pending',
        isComplete: false,
    });
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const intervalRef = useRef<NodeJS.Timeout | null>(null);

    const fetchMetrics = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const group = await invoke<GroupResponse>('get_group_details', {
                group_id: groupId,
            });

            setMetrics({
                overall_progress: group.overall_progress,
                completed_count: group.completed_count,
                total_count: group.total_count,
                state: group.state,
                isComplete: group.state === 'Completed',
            });
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, [groupId]);

    useEffect(() => {
        // Initial fetch
        fetchMetrics();

        // Set up interval for refresh
        intervalRef.current = setInterval(fetchMetrics, refreshIntervalMs);

        // Cleanup on unmount
        return () => {
            if (intervalRef.current) {
                clearInterval(intervalRef.current);
            }
        };
    }, [groupId, refreshIntervalMs, fetchMetrics]);

    return { ...metrics, isLoading, error };
}

// ============ useGroupMembers Hook ============

/**
 * Hook for auto-updating group members with progress.
 * 
 * Fetches all members at specified interval and auto-refreshes.
 * Returns current members array and cleanup on unmount.
 * 
 * @param groupId - ID of the group to monitor
 * @param refreshIntervalMs - Refresh interval in milliseconds (default: 1500)
 */
export function useGroupMembers(
    groupId: string,
    refreshIntervalMs: number = 1500
): {
    members: MemberResponse[];
    isLoading: boolean;
    error: string | null;
} {
    const [members, setMembers] = useState<MemberResponse[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const intervalRef = useRef<NodeJS.Timeout | null>(null);

    const fetchMembers = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const group = await invoke<GroupResponse>('get_group_details', {
                group_id: groupId,
            });

            setMembers(group.members || []);
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
        } finally {
            setIsLoading(false);
        }
    }, [groupId]);

    useEffect(() => {
        // Initial fetch
        fetchMembers();

        // Set up interval for refresh
        intervalRef.current = setInterval(fetchMembers, refreshIntervalMs);

        // Cleanup on unmount
        return () => {
            if (intervalRef.current) {
                clearInterval(intervalRef.current);
            }
        };
    }, [groupId, refreshIntervalMs, fetchMembers]);

    return { members, isLoading, error };
}
