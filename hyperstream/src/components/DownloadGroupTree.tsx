/**
 * DownloadGroupTree Component
 * 
 * Main UI component for displaying and managing download groups in tree/list view.
 * 
 * Features:
 * - Expand/collapse groups
 * - Real-time progress updates via useGroupMetrics
 * - Context menus on groups and members
 * - Create new group/member forms
 * - Dark glassmorphism theme
 * - Framer Motion animations
 * - lucide-react icons
 */

import React, { useState, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
    ChevronDown,
    ChevronRight,
    Pause,
    Play,
    Trash2,
    Plus,
    AlertCircle,
    CheckCircle2,
    Clock,
    Loader2,
} from 'lucide-react';
import { useDownloadGroups, useGroupMetrics, useGroupMembers } from '../hooks/useDownloadGroups';
import { GroupProgressBar } from './GroupProgressBar';
import type { GroupResponse, MemberResponse } from '../types';

interface DownloadGroupTreeProps {
    /** Optional CSS class name */
    className?: string;
}

interface ExpandedState {
    [groupId: string]: boolean;
}

/**
 * Helper: Truncate long URLs for display
 */
const truncateUrl = (url: string, maxLength: number = 60): string => {
    if (url.length <= maxLength) return url;
    return url.substring(0, maxLength - 3) + '...';
};

/**
 * Helper: Get state color
 */
const getStateColor = (state: string): string => {
    switch (state.toLowerCase()) {
        case 'completed':
            return 'bg-green-500/20 border-green-500/40 text-green-400';
        case 'downloading':
            return 'bg-blue-500/20 border-blue-500/40 text-blue-400';
        case 'error':
            return 'bg-red-500/20 border-red-500/40 text-red-400';
        case 'paused':
            return 'bg-amber-500/20 border-amber-500/40 text-amber-400';
        default:
            return 'bg-slate-500/20 border-slate-500/40 text-slate-400';
    }
};

/**
 * Helper: Get state icon
 */
const getStateIcon = (state: string) => {
    switch (state.toLowerCase()) {
        case 'completed':
            return <CheckCircle2 className="w-4 h-4" />;
        case 'downloading':
            return <Loader2 className="w-4 h-4 animate-spin" />;
        case 'error':
            return <AlertCircle className="w-4 h-4" />;
        case 'paused':
            return <Pause className="w-4 h-4" />;
        default:
            return <Clock className="w-4 h-4" />;
    }
};

/**
 * Single group row component
 */
const GroupRow: React.FC<{
    group: GroupResponse;
    isExpanded: boolean;
    onExpandToggle: () => void;
    onStart: () => void;
    onPause: () => void;
    onDelete: () => void;
    metrics: ReturnType<typeof useGroupMetrics>;
    members: MemberResponse[];
}> = ({
    group,
    isExpanded,
    onExpandToggle,
    onStart,
    onPause,
    onDelete,
    metrics,
    members,
}) => {
    const [showMenu, setShowMenu] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);

    return (
        <div className="space-y-2">
            {/* Group header */}
            <div className="flex items-center gap-2 p-3 rounded-lg bg-slate-800/40 backdrop-blur-md border border-slate-700/50 hover:bg-slate-800/60 transition-colors">
                {/* Expand button */}
                <button
                    onClick={onExpandToggle}
                    className="flex-shrink-0 p-1 hover:bg-slate-700/50 rounded transition-colors"
                    title={isExpanded ? 'Collapse' : 'Expand'}
                >
                    {isExpanded ? (
                        <ChevronDown className="w-4 h-4 text-cyan-400" />
                    ) : (
                        <ChevronRight className="w-4 h-4 text-slate-500" />
                    )}
                </button>

                {/* State icon */}
                <div className={`flex-shrink-0 p-1 rounded ${getStateColor(group.state)}`}>
                    {getStateIcon(group.state)}
                </div>

                {/* Group info */}
                <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                        <h3 className="font-semibold text-slate-100 truncate">{group.name}</h3>
                        <span
                            className={`px-2 py-0.5 rounded-full text-xs font-medium border ${getStateColor(
                                group.state
                            )}`}
                        >
                            {group.state}
                        </span>
                    </div>
                    <div className="text-xs text-slate-400 mt-1">
                        {metrics.completed_count} / {metrics.total_count} downloads completed
                    </div>
                </div>

                {/* Action buttons */}
                <div className="flex items-center gap-1 flex-shrink-0">
                    {group.state === 'Downloading' ? (
                        <button
                            onClick={onPause}
                            className="p-2 hover:bg-slate-700/50 rounded transition-colors"
                            title="Pause group"
                        >
                            <Pause className="w-4 h-4 text-amber-400" />
                        </button>
                    ) : (
                        <button
                            onClick={onStart}
                            className="p-2 hover:bg-slate-700/50 rounded transition-colors"
                            title="Start group"
                        >
                            <Play className="w-4 h-4 text-green-400" />
                        </button>
                    )}

                    {/* Context menu button */}
                    <div className="relative" ref={menuRef}>
                        <button
                            onClick={() => setShowMenu(!showMenu)}
                            className="p-2 hover:bg-slate-700/50 rounded transition-colors"
                            title="More options"
                        >
                            <span className="text-slate-400">⋮</span>
                        </button>

                        {/* Context menu */}
                        <AnimatePresence>
                            {showMenu && (
                                <motion.div
                                    initial={{ opacity: 0, scale: 0.95 }}
                                    animate={{ opacity: 1, scale: 1 }}
                                    exit={{ opacity: 0, scale: 0.95 }}
                                    className="absolute right-0 mt-1 z-50 bg-slate-800/95 backdrop-blur-lg border border-slate-700/50 rounded-lg shadow-lg overflow-hidden"
                                >
                                    <button
                                        onClick={() => {
                                            onDelete();
                                            setShowMenu(false);
                                        }}
                                        className="w-full px-3 py-2 text-red-400 hover:bg-red-500/20 text-sm flex items-center gap-2 transition-colors"
                                    >
                                        <Trash2 className="w-4 h-4" />
                                        Delete
                                    </button>
                                </motion.div>
                            )}
                        </AnimatePresence>
                    </div>
                </div>
            </div>

            {/* Progress bar */}
            <div className="px-3">
                <GroupProgressBar
                    members={members}
                    overallProgress={metrics.overall_progress}
                    state={group.state}
                    completedCount={metrics.completed_count}
                    totalCount={metrics.total_count}
                    showPauseButton={false}
                />
            </div>

            {/* Expanded members list */}
            <AnimatePresence>
                {isExpanded && (
                    <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: 'auto' }}
                        exit={{ opacity: 0, height: 0 }}
                        className="pl-8 space-y-2 overflow-hidden"
                    >
                        {members.length === 0 ? (
                            <div className="text-sm text-slate-500 italic py-2">
                                No members yet. Add one to get started.
                            </div>
                        ) : (
                            members.map((member) => (
                                <MemberRow key={member.id} member={member} />
                            ))
                        )}

                        {/* Add member form */}
                        <AddMemberForm groupId={group.id} />
                    </motion.div>
                )}
            </AnimatePresence>
        </div>
    );
};

/**
 * Single member row component
 */
const MemberRow: React.FC<{ member: MemberResponse }> = ({ member }) => {
    const [showDetails, setShowDetails] = useState(false);

    return (
        <motion.div
            initial={{ opacity: 0, x: -10 }}
            animate={{ opacity: 1, x: 0 }}
            className="flex items-center gap-2 p-2 rounded-lg bg-slate-700/30 border border-slate-600/30 hover:bg-slate-700/50 transition-colors group"
        >
            {/* State icon */}
            <div className={`flex-shrink-0 p-1 rounded ${getStateColor(member.state)}`}>
                {getStateIcon(member.state)}
            </div>

            {/* URL and progress */}
            <div className="flex-1 min-w-0">
                <div className="text-sm text-slate-300 truncate font-mono">
                    {truncateUrl(member.url)}
                </div>
                <div className="flex items-center gap-2 mt-1">
                    {/* Progress bar */}
                    <div className="flex-1 h-1.5 rounded-full bg-slate-600/50">
                        <motion.div
                            className="h-full rounded-full bg-gradient-to-r from-cyan-500 to-blue-500"
                            initial={{ width: 0 }}
                            animate={{ width: `${member.progress_percent}%` }}
                            transition={{ duration: 0.3 }}
                        />
                    </div>

                    {/* Progress text */}
                    <span className="text-xs font-mono text-slate-400 flex-shrink-0">
                        {member.progress_percent.toFixed(0)}%
                    </span>
                </div>

                {/* Dependencies info */}
                {member.dependencies_count > 0 && (
                    <div className="text-xs text-amber-400 mt-1">
                        Depends on {member.dependencies_count} member(s)
                    </div>
                )}
            </div>

            {/* Details button */}
            <button
                onClick={() => setShowDetails(!showDetails)}
                className="flex-shrink-0 p-1 opacity-0 group-hover:opacity-100 hover:bg-slate-600/50 rounded transition-all"
                title="View details"
            >
                <span className="text-slate-400">→</span>
            </button>
        </motion.div>
    );
};

/**
 * Add member form component
 */
const AddMemberForm: React.FC<{ groupId: string }> = ({ groupId }) => {
    const [showForm, setShowForm] = useState(false);
    const [url, setUrl] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const { addMember } = useDownloadGroups();

    const handleAddMember = async () => {
        if (!url.trim()) return;

        setIsLoading(true);
        try {
            await addMember(groupId, url);
            setUrl('');
            setShowForm(false);
        } catch (err) {
            console.error('Failed to add member:', err);
        } finally {
            setIsLoading(false);
        }
    };

    return (
        <div>
            {!showForm ? (
                <button
                    onClick={() => setShowForm(true)}
                    className="w-full px-3 py-2 rounded-lg bg-slate-700/30 border border-dashed border-slate-600/50 hover:bg-slate-700/50 transition-colors text-slate-400 text-sm flex items-center justify-center gap-2"
                >
                    <Plus className="w-4 h-4" />
                    Add member
                </button>
            ) : (
                <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    exit={{ opacity: 0, height: 0 }}
                    className="flex gap-2"
                >
                    <input
                        type="text"
                        value={url}
                        onChange={(e) => setUrl(e.target.value)}
                        placeholder="Enter download URL..."
                        className="flex-1 px-3 py-2 rounded-lg bg-slate-700/50 border border-slate-600/50 text-slate-200 placeholder-slate-500 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/30"
                        onKeyDown={(e) => {
                            if (e.key === 'Enter') handleAddMember();
                            if (e.key === 'Escape') setShowForm(false);
                        }}
                        autoFocus
                    />
                    <button
                        onClick={handleAddMember}
                        disabled={isLoading}
                        className="px-3 py-2 rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 border border-cyan-500/40 text-cyan-400 text-sm disabled:opacity-50 transition-colors flex items-center gap-1"
                    >
                        {isLoading ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                            <Plus className="w-4 h-4" />
                        )}
                    </button>
                    <button
                        onClick={() => setShowForm(false)}
                        className="px-3 py-2 rounded-lg hover:bg-slate-600/30 transition-colors"
                    >
                        ✕
                    </button>
                </motion.div>
            )}
        </div>
    );
};

/**
 * Main DownloadGroupTree Component
 */
export const DownloadGroupTree: React.FC<DownloadGroupTreeProps> = ({ className = '' }) => {
    const [groups, setGroups] = useState<GroupResponse[]>([]);
    const [expandedState, setExpandedState] = useState<ExpandedState>({});
    const [showNewGroupForm, setShowNewGroupForm] = useState(false);
    const [newGroupName, setNewGroupName] = useState('');
    const [isLoadingGroups, setIsLoadingGroups] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const { createGroup, startGroup, pauseGroup, listGroups } = useDownloadGroups();

    // Load groups on mount
    React.useEffect(() => {
        loadGroups();
    }, []);

    const loadGroups = async () => {
        setIsLoadingGroups(true);
        setError(null);
        try {
            const loadedGroups = await listGroups();
            setGroups(loadedGroups);
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            setError(errorMsg);
        } finally {
            setIsLoadingGroups(false);
        }
    };

    const handleCreateGroup = async () => {
        if (!newGroupName.trim()) return;

        try {
            const newGroup = await createGroup(newGroupName);
            setGroups([...groups, newGroup]);
            setNewGroupName('');
            setShowNewGroupForm(false);
            setExpandedState({ ...expandedState, [newGroup.id]: true });
        } catch (err) {
            console.error('Failed to create group:', err);
        }
    };

    const toggleExpanded = (groupId: string) => {
        setExpandedState((prev) => ({
            ...prev,
            [groupId]: !prev[groupId],
        }));
    };

    return (
        <div
            className={`w-full max-w-4xl mx-auto p-4 space-y-4 ${className}`}
        >
            {/* Header */}
            <div className="flex items-center justify-between">
                <h2 className="text-xl font-bold text-slate-100">Download Groups</h2>
                <button
                    onClick={loadGroups}
                    disabled={isLoadingGroups}
                    className="px-3 py-1 rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 border border-cyan-500/40 text-cyan-400 text-sm disabled:opacity-50 transition-colors"
                >
                    Refresh
                </button>
            </div>

            {/* Error display */}
            {error && (
                <div className="p-3 rounded-lg bg-red-500/20 border border-red-500/40 text-red-300 flex items-center gap-2">
                    <AlertCircle className="w-4 h-4 flex-shrink-0" />
                    <span className="text-sm">{error}</span>
                    <button onClick={() => setError(null)} className="ml-auto text-red-300 hover:text-red-200">
                        ✕
                    </button>
                </div>
            )}

            {/* New group form */}
            {!showNewGroupForm ? (
                <button
                    onClick={() => setShowNewGroupForm(true)}
                    className="w-full px-4 py-3 rounded-lg bg-slate-700/30 border border-dashed border-slate-600/50 hover:bg-slate-700/50 transition-colors text-slate-400 font-medium flex items-center justify-center gap-2"
                >
                    <Plus className="w-5 h-5" />
                    Create new group
                </button>
            ) : (
                <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    exit={{ opacity: 0, height: 0 }}
                    className="flex gap-2 p-3 rounded-lg bg-slate-800/40 border border-slate-700/50"
                >
                    <input
                        type="text"
                        value={newGroupName}
                        onChange={(e) => setNewGroupName(e.target.value)}
                        placeholder="Group name (e.g., 'OS Updates')"
                        className="flex-1 px-3 py-2 rounded-lg bg-slate-700/50 border border-slate-600/50 text-slate-200 placeholder-slate-500 focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/30"
                        onKeyDown={(e) => {
                            if (e.key === 'Enter') handleCreateGroup();
                            if (e.key === 'Escape') setShowNewGroupForm(false);
                        }}
                        autoFocus
                    />
                    <button
                        onClick={handleCreateGroup}
                        className="px-4 py-2 rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 border border-cyan-500/40 text-cyan-400 font-medium transition-colors"
                    >
                        Create
                    </button>
                    <button
                        onClick={() => setShowNewGroupForm(false)}
                        className="px-3 py-2 rounded-lg hover:bg-slate-600/30 transition-colors"
                    >
                        Cancel
                    </button>
                </motion.div>
            )}

            {/* Groups list */}
            <div className="space-y-3">
                {isLoadingGroups ? (
                    <div className="flex items-center justify-center py-8 text-slate-400">
                        <Loader2 className="w-5 h-5 animate-spin mr-2" />
                        Loading groups...
                    </div>
                ) : groups.length === 0 ? (
                    <div className="text-center py-8 text-slate-500 italic">
                        No groups yet. Create one to get started!
                    </div>
                ) : (
                    groups.map((group) => (
                        <GroupRowWithHooks
                            key={group.id}
                            group={group}
                            isExpanded={expandedState[group.id] || false}
                            onExpandToggle={() => toggleExpanded(group.id)}
                            onStart={() => startGroup(group.id).catch(console.error)}
                            onPause={() => pauseGroup(group.id).catch(console.error)}
                            onDelete={() => {
                                setGroups(groups.filter((g) => g.id !== group.id));
                            }}
                        />
                    ))
                )}
            </div>
        </div>
    );
};

/**
 * Wrapper component that connects GroupRow to hooks
 */
const GroupRowWithHooks: React.FC<{
    group: GroupResponse;
    isExpanded: boolean;
    onExpandToggle: () => void;
    onStart: () => void;
    onPause: () => void;
    onDelete: () => void;
}> = ({ group, isExpanded, onExpandToggle, onStart, onPause, onDelete }) => {
    const metrics = useGroupMetrics(group.id, 1500);
    const { members } = useGroupMembers(group.id, 1500);

    return (
        <GroupRow
            group={group}
            isExpanded={isExpanded}
            onExpandToggle={onExpandToggle}
            onStart={onStart}
            onPause={onPause}
            onDelete={onDelete}
            metrics={metrics}
            members={members}
        />
    );
};

export default DownloadGroupTree;
