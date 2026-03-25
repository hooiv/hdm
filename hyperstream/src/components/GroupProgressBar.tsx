/**
 * GroupProgressBar Component
 * 
 * Displays unified progress bar for entire download group with:
 * - Overall progress percentage
 * - Completion counter
 * - Segmented progress bar (one segment per member)
 * - Hover tooltips with per-member details
 * - Color coding based on member status
 * - Optional pause button overlay
 */

import React, { useMemo } from 'react';
import { motion } from 'framer-motion';
import { Pause, CheckCircle2, AlertCircle, Clock } from 'lucide-react';
import type { MemberResponse, GroupState } from '../types';

interface GroupProgressBarProps {
    /** Array of group members with progress info */
    members: MemberResponse[];
    /** Overall group progress (0-100) */
    overallProgress: number;
    /** Current group state */
    state: GroupState | string;
    /** Completed member count */
    completedCount: number;
    /** Total member count */
    totalCount: number;
    /** Optional pause button click handler */
    onPauseClick?: () => void;
    /** Show pause button overlay */
    showPauseButton?: boolean;
    /** Optional CSS class name */
    className?: string;
}

/**
 * Helper: Get color for a member based on state
 */
const getSegmentColor = (memberState: string): string => {
    switch (memberState.toLowerCase()) {
        case 'completed':
            return 'bg-gradient-to-r from-green-500 to-emerald-500';
        case 'downloading':
            return 'bg-gradient-to-r from-blue-500 to-cyan-500';
        case 'paused':
            return 'bg-gradient-to-r from-amber-500 to-orange-500';
        case 'error':
            return 'bg-gradient-to-r from-red-500 to-rose-500';
        default:
            return 'bg-gradient-to-r from-gray-400 to-slate-500';
    }
};

/**
 * Helper: Get status icon based on state
 */
const getStatusIcon = (state: GroupState | string) => {
    switch (state.toLowerCase()) {
        case 'completed':
            return <CheckCircle2 className="w-4 h-4 text-green-400" />;
        case 'error':
            return <AlertCircle className="w-4 h-4 text-red-400" />;
        case 'downloading':
            return (
                <motion.div animate={{ rotate: 360 }} transition={{ duration: 2, repeat: Infinity }}>
                    <Clock className="w-4 h-4 text-blue-400" />
                </motion.div>
            );
        default:
            return <Clock className="w-4 h-4 text-gray-400" />;
    }
};

/**
 * GroupProgressBar Component
 */
export const GroupProgressBar: React.FC<GroupProgressBarProps> = ({
    members,
    overallProgress,
    state,
    completedCount,
    totalCount,
    onPauseClick,
    showPauseButton = true,
    className = '',
}) => {
    // Calculate segment widths based on member count
    const segmentWidths = useMemo(() => {
        if (members.length === 0) return [];
        return members.map(() => 100 / members.length);
    }, [members]);

    // Create tooltip content for a member
    const getMemberTooltip = (member: MemberResponse): string => {
        const url = member.url.length > 50 ? member.url.substring(0, 47) + '...' : member.url;
        return `${url}\n${member.progress_percent.toFixed(1)}% - ${member.state}`;
    };

    return (
        <div className={`space-y-3 ${className}`}>
            {/* Header with status and counter */}
            <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                    {getStatusIcon(state)}
                    <span className="text-sm font-medium text-slate-200">
                        {state === 'Completed' ? 'Completed' : 'In Progress'}
                    </span>
                </div>
                <div className="text-sm font-mono text-cyan-400">
                    {completedCount} / {totalCount} completed
                </div>
            </div>

            {/* Progress percentage label */}
            <div className="flex items-center justify-between">
                <span className="text-xs text-slate-400">Overall Progress</span>
                <span className="text-xs font-mono text-cyan-300">
                    {Math.round(overallProgress)}%
                </span>
            </div>

            {/* Main progress bar with segments */}
            <div className="relative">
                {/* Background bar */}
                <div
                    className="h-3 rounded-full bg-slate-800/50 backdrop-blur-sm border border-slate-700/50 overflow-hidden"
                >
                    {/* Segmented progress */}
                    <div className="flex h-full w-full">
                        {members.map((member, idx) => {
                            const memberProgress = Math.min(
                                (member.progress_percent / 100) * segmentWidths[idx],
                                segmentWidths[idx]
                            );

                            return (
                                <div
                                    key={member.id}
                                    className="relative flex-1"
                                    title={getMemberTooltip(member)}
                                >
                                    {/* Segment background */}
                                    <div className="h-full w-full bg-slate-700/30" />

                                    {/* Segment progress */}
                                    <motion.div
                                        className={`absolute top-0 left-0 h-full ${getSegmentColor(member.state)}`}
                                        initial={{ width: 0 }}
                                        animate={{
                                            width: `${memberProgress}%`,
                                        }}
                                        transition={{ duration: 0.5, ease: 'easeOut' }}
                                    />
                                </div>
                            );
                        })}
                    </div>

                    {/* Overall progress overlay */}
                    <motion.div
                        className="absolute top-0 left-0 h-full bg-gradient-to-r from-cyan-500/20 to-blue-500/20 pointer-events-none"
                        initial={{ width: 0 }}
                        animate={{
                            width: `${overallProgress}%`,
                        }}
                        transition={{ duration: 0.5, ease: 'easeOut' }}
                    />
                </div>

                {/* Pause button overlay (optional) */}
                {showPauseButton && onPauseClick && state.toLowerCase() === 'downloading' && (
                    <motion.button
                        onClick={onPauseClick}
                        className="absolute right-2 top-1/2 transform -translate-y-1/2 p-1 rounded-full bg-slate-600/60 hover:bg-slate-500/80 transition-colors"
                        whileHover={{ scale: 1.1 }}
                        whileTap={{ scale: 0.95 }}
                        title="Pause download"
                    >
                        <Pause className="w-3 h-3 text-slate-200" />
                    </motion.button>
                )}
            </div>

            {/* Member details row (optional) */}
            {members.length > 0 && (
                <div className="text-xs text-slate-500 flex gap-2 flex-wrap mt-2">
                    <span>
                        {members.filter((m) => m.state === 'Completed').length} completed
                    </span>
                    <span>•</span>
                    <span>
                        {members.filter((m) => m.state === 'Downloading').length} downloading
                    </span>
                    {members.filter((m) => m.state === 'Error').length > 0 && (
                        <>
                            <span>•</span>
                            <span className="text-red-400">
                                {members.filter((m) => m.state === 'Error').length} errors
                            </span>
                        </>
                    )}
                </div>
            )}
        </div>
    );
};

export default GroupProgressBar;
