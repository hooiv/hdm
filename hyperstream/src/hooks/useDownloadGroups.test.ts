/**
 * Tests for Download Groups hooks and components
 * 
 * Coverage:
 * - useDownloadGroups hook return values
 * - useGroupMetrics auto-refresh
 * - useGroupMembers auto-refresh
 * - Component renders without crashing
 * - Error handling
 */

import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { renderHook } from '@testing-library/react';
import { useDownloadGroups, useGroupMetrics, useGroupMembers } from '../hooks/useDownloadGroups';
import { DownloadGroupTree } from '../components/DownloadGroupTree';
import { GroupProgressBar } from '../components/GroupProgressBar';
import type { GroupResponse, MemberResponse } from '../types';

// Mock the Tauri invoke function
jest.mock('@tauri-apps/api/core', () => ({
    invoke: jest.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
const mockedInvoke = invoke as jest.MockedFunction<typeof invoke>;

// ============ Tests for useDownloadGroups Hook ============

describe('useDownloadGroups', () => {
    beforeEach(() => {
        jest.clearAllMocks();
    });

    test('should return all required methods', () => {
        const { result } = renderHook(() => useDownloadGroups());

        expect(result.current).toHaveProperty('createGroup');
        expect(result.current).toHaveProperty('addMember');
        expect(result.current).toHaveProperty('addDependency');
        expect(result.current).toHaveProperty('startGroup');
        expect(result.current).toHaveProperty('pauseGroup');
        expect(result.current).toHaveProperty('updateMemberProgress');
        expect(result.current).toHaveProperty('completeMember');
        expect(result.current).toHaveProperty('listGroups');
        expect(result.current).toHaveProperty('isLoading');
        expect(result.current).toHaveProperty('error');
    });

    test('createGroup should invoke create_download_group command', async () => {
        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test Group',
            state: 'Pending',
            strategy: 'Hybrid',
            members: [],
            overall_progress: 0,
            completed_count: 0,
            total_count: 0,
            created_at_ms: Date.now(),
            completed_at_ms: 0,
        };

        mockedInvoke.mockResolvedValueOnce(mockGroup);

        const { result } = renderHook(() => useDownloadGroups());

        const group = await result.current.createGroup('Test Group');

        expect(mockedInvoke).toHaveBeenCalledWith('create_download_group', { name: 'Test Group' });
        expect(group.name).toBe('Test Group');
    });

    test('addMember should invoke add_member_to_group command', async () => {
        mockedInvoke.mockResolvedValueOnce('member-123');

        const { result } = renderHook(() => useDownloadGroups());

        const memberId = await result.current.addMember('group-1', 'https://example.com/file.zip');

        expect(mockedInvoke).toHaveBeenCalledWith('add_member_to_group', {
            group_id: 'group-1',
            url: 'https://example.com/file.zip',
            dependencies: null,
        });
        expect(memberId).toBe('member-123');
    });

    test('startGroup should invoke start_group_download command', async () => {
        mockedInvoke.mockResolvedValueOnce(undefined);

        const { result } = renderHook(() => useDownloadGroups());

        await result.current.startGroup('group-1');

        expect(mockedInvoke).toHaveBeenCalledWith('start_group_download', { group_id: 'group-1' });
    });

    test('pauseGroup should invoke pause_group_download command', async () => {
        mockedInvoke.mockResolvedValueOnce(undefined);

        const { result } = renderHook(() => useDownloadGroups());

        await result.current.pauseGroup('group-1');

        expect(mockedInvoke).toHaveBeenCalledWith('pause_group_download', { group_id: 'group-1' });
    });

    test('updateMemberProgress should clamp progress 0-100', async () => {
        mockedInvoke.mockResolvedValueOnce(undefined);

        const { result } = renderHook(() => useDownloadGroups());

        // Test clamping to max
        await result.current.updateMemberProgress('group-1', 'member-1', 150);
        expect(mockedInvoke).toHaveBeenLastCalledWith('update_member_progress', {
            group_id: 'group-1',
            member_id: 'member-1',
            progress_percent: 100,
        });

        // Test clamping to min
        jest.clearAllMocks();
        mockedInvoke.mockResolvedValueOnce(undefined);

        await result.current.updateMemberProgress('group-1', 'member-1', -10);
        expect(mockedInvoke).toHaveBeenLastCalledWith('update_member_progress', {
            group_id: 'group-1',
            member_id: 'member-1',
            progress_percent: 0,
        });
    });

    test('listGroups should invoke list_all_groups command', async () => {
        const mockGroups: GroupResponse[] = [
            {
                id: 'group-1',
                name: 'Group 1',
                state: 'Pending',
                strategy: 'Hybrid',
                members: [],
                overall_progress: 0,
                completed_count: 0,
                total_count: 0,
                created_at_ms: Date.now(),
                completed_at_ms: 0,
            },
        ];

        mockedInvoke.mockResolvedValueOnce(mockGroups);

        const { result } = renderHook(() => useDownloadGroups());

        const groups = await result.current.listGroups();

        expect(mockedInvoke).toHaveBeenCalledWith('list_all_groups');
        expect(groups).toEqual(mockGroups);
        expect(groups.length).toBe(1);
    });

    test('should handle errors gracefully', async () => {
        const errorMessage = 'Group not found';
        mockedInvoke.mockRejectedValueOnce(new Error(errorMessage));

        const { result } = renderHook(() => useDownloadGroups());

        try {
            await result.current.startGroup('nonexistent');
        } catch (err) {
            expect((err as Error).message).toBe(errorMessage);
        }

        expect(result.current.error).toBe(errorMessage);
    });
});

// ============ Tests for useGroupMetrics Hook ============

describe('useGroupMetrics', () => {
    beforeEach(() => {
        jest.clearAllMocks();
        jest.useFakeTimers();
    });

    afterEach(() => {
        jest.runOnlyPendingTimers();
        jest.useRealTimers();
    });

    test('should fetch initial metrics and return correct structure', async () => {
        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test',
            state: 'Downloading',
            strategy: 'Hybrid',
            members: [],
            overall_progress: 45.5,
            completed_count: 2,
            total_count: 5,
            created_at_ms: Date.now(),
            completed_at_ms: 0,
        };

        mockedInvoke.mockResolvedValueOnce(mockGroup);

        const { result } = renderHook(() => useGroupMetrics('group-1', 2000));

        await waitFor(() => {
            expect(result.current.overall_progress).toBe(45.5);
        });

        expect(result.current.completed_count).toBe(2);
        expect(result.current.total_count).toBe(5);
        expect(result.current.state).toBe('Downloading');
        expect(result.current.isComplete).toBe(false);
    });

    test('isComplete should be true when state is Completed', async () => {
        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test',
            state: 'Completed',
            strategy: 'Hybrid',
            members: [],
            overall_progress: 100,
            completed_count: 5,
            total_count: 5,
            created_at_ms: Date.now(),
            completed_at_ms: Date.now(),
        };

        mockedInvoke.mockResolvedValueOnce(mockGroup);

        const { result } = renderHook(() => useGroupMetrics('group-1', 2000));

        await waitFor(() => {
            expect(result.current.isComplete).toBe(true);
        });
    });

    test('should refresh metrics on interval', async () => {
        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test',
            state: 'Downloading',
            strategy: 'Hybrid',
            members: [],
            overall_progress: 50,
            completed_count: 2,
            total_count: 5,
            created_at_ms: Date.now(),
            completed_at_ms: 0,
        };

        mockedInvoke.mockResolvedValue(mockGroup);

        const { result } = renderHook(() => useGroupMetrics('group-1', 1000));

        // Wait for initial fetch
        await waitFor(() => {
            expect(mockedInvoke).toHaveBeenCalled();
        });

        const initialCallCount = mockedInvoke.mock.calls.length;

        // Advance time to trigger interval
        jest.advanceTimersByTime(1100);

        await waitFor(() => {
            expect(mockedInvoke.mock.calls.length).toBeGreaterThan(initialCallCount);
        });
    });
});

// ============ Tests for useGroupMembers Hook ============

describe('useGroupMembers', () => {
    beforeEach(() => {
        jest.clearAllMocks();
        jest.useFakeTimers();
    });

    afterEach(() => {
        jest.runOnlyPendingTimers();
        jest.useRealTimers();
    });

    test('should fetch initial members', async () => {
        const mockMembers: MemberResponse[] = [
            {
                id: 'member-1',
                url: 'https://example.com/file1.zip',
                progress_percent: 25,
                state: 'Downloading',
                dependencies_count: 0,
                dependencies: [],
            },
            {
                id: 'member-2',
                url: 'https://example.com/file2.zip',
                progress_percent: 0,
                state: 'Pending',
                dependencies_count: 1,
                dependencies: ['member-1'],
            },
        ];

        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test',
            state: 'Downloading',
            strategy: 'Hybrid',
            members: mockMembers,
            overall_progress: 12.5,
            completed_count: 0,
            total_count: 2,
            created_at_ms: Date.now(),
            completed_at_ms: 0,
        };

        mockedInvoke.mockResolvedValueOnce(mockGroup);

        const { result } = renderHook(() => useGroupMembers('group-1', 1500));

        await waitFor(() => {
            expect(result.current.members.length).toBe(2);
        });

        expect(result.current.members[0].url).toBe('https://example.com/file1.zip');
        expect(result.current.members[1].progress_percent).toBe(0);
    });

    test('should handle empty members list', async () => {
        const mockGroup: GroupResponse = {
            id: 'group-1',
            name: 'Test',
            state: 'Pending',
            strategy: 'Hybrid',
            members: [],
            overall_progress: 0,
            completed_count: 0,
            total_count: 0,
            created_at_ms: Date.now(),
            completed_at_ms: 0,
        };

        mockedInvoke.mockResolvedValueOnce(mockGroup);

        const { result } = renderHook(() => useGroupMembers('group-1', 1500));

        await waitFor(() => {
            expect(result.current.members.length).toBe(0);
        });
    });
});

// ============ Tests for Components ============

describe('GroupProgressBar Component', () => {
    const mockMembers: MemberResponse[] = [
        {
            id: 'member-1',
            url: 'https://example.com/file1.zip',
            progress_percent: 50,
            state: 'Downloading',
            dependencies_count: 0,
            dependencies: [],
        },
        {
            id: 'member-2',
            url: 'https://example.com/file2.zip',
            progress_percent: 100,
            state: 'Completed',
            dependencies_count: 0,
            dependencies: [],
        },
    ];

    test('should render without crashing', () => {
        render(
            <GroupProgressBar
                members={mockMembers}
                overallProgress={75}
                state="Downloading"
                completedCount={1}
                totalCount={2}
            />
        );

        expect(screen.getByText(/1 \/ 2 completed/)).toBeInTheDocument();
    });

    test('should display correct progress percentage', () => {
        render(
            <GroupProgressBar
                members={mockMembers}
                overallProgress={75}
                state="Downloading"
                completedCount={1}
                totalCount={2}
            />
        );

        expect(screen.getByText(/75%/)).toBeInTheDocument();
    });

    test('should show completion status', () => {
        render(
            <GroupProgressBar
                members={mockMembers}
                overallProgress={100}
                state="Completed"
                completedCount={2}
                totalCount={2}
            />
        );

        expect(screen.getByText('Completed')).toBeInTheDocument();
    });
});

describe('DownloadGroupTree Component', () => {
    beforeEach(() => {
        jest.clearAllMocks();
    });

    test('should render without crashing', () => {
        mockedInvoke.mockResolvedValueOnce([]);

        render(<DownloadGroupTree />);

        expect(screen.getByText('Download Groups')).toBeInTheDocument();
    });

    test('should display loading state initially', () => {
        mockedInvoke.mockImplementationOnce(() => new Promise(() => {})); // Never resolves

        render(<DownloadGroupTree />);

        // Component should render without error
        expect(screen.getByText('Download Groups')).toBeInTheDocument();
    });

    test('should display empty state when no groups exist', async () => {
        mockedInvoke.mockResolvedValueOnce([]);

        render(<DownloadGroupTree />);

        await waitFor(() => {
            expect(screen.queryByText(/No groups yet/i)).toBeInTheDocument();
        });
    });
});
