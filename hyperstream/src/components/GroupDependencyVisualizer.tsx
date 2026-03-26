/**
 * Dependency Graph Visualizer
 * 
 * Visualizes download group dependencies as an interactive DAG:
 * - Shows member relationships
 * - Highlights critical paths
 * - Displays execution phases
 * - Interactive node selection
 */

import React, { useState, useMemo } from 'react';
import { motion } from 'framer-motion';
import {
    ChevronDown,
    AlertCircle,
    CheckCircle2,
    Clock,
    Zap,
} from 'lucide-react';

interface Member {
    id: string;
    url: string;
    progress_percent: number;
    state: string;
    dependencies: string[];
}

interface GroupDependencyVisualizerProps {
    members: Map<string, Member>;
    onMemberClick?: (memberId: string) => void;
    className?: string;
}

/**
 * Helper: Get color for member state
 */
const getStateColor = (state: string): string => {
    switch (state.toLowerCase()) {
        case 'completed':
            return 'bg-green-500/20 text-green-400 border-green-500/40';
        case 'downloading':
            return 'bg-blue-500/20 text-blue-400 border-blue-500/40';
        case 'error':
            return 'bg-red-500/20 text-red-400 border-red-500/40';
        case 'paused':
            return 'bg-amber-500/20 text-amber-400 border-amber-500/40';
        default:
            return 'bg-slate-500/20 text-slate-400 border-slate-500/40';
    }
};

/**
 * Helper: Calculate positions for DAG layout
 */
const calculateDAGLayout = (members: Map<string, Member>) => {
    const layout = new Map<string, { x: number; y: number; depth: number }>();
    const depths = new Map<string, number>();

    // Calculate depth for each member
    const calculateDepth = (id: string, visited = new Set<string>()): number => {
        if (visited.has(id)) return 0; // Cycle detection
        if (depths.has(id)) return depths.get(id)!;

        visited.add(id);
        const member = members.get(id);
        if (!member || member.dependencies.length === 0) {
            depths.set(id, 0);
            return 0;
        }

        const maxDepDepth = Math.max(
            ...member.dependencies.map((dep) => calculateDepth(dep, visited)),
            0
        );
        const depth = maxDepDepth + 1;
        depths.set(id, depth);
        return depth;
    };

    members.forEach((_, id) => calculateDepth(id));

    // Group members by depth level
    const levels = new Map<number, string[]>();
    depths.forEach((depth, id) => {
        if (!levels.has(depth)) levels.set(depth, []);
        levels.get(depth)!.push(id);
    });

    // Calculate positions
    let yOffset = 0;
    const maxDepth = Math.max(...Array.from(depths.values()));

    levels.forEach((ids, depth) => {
        const xStep = 100 / (maxDepth + 1);
        const ySpace = 60;

        ids.forEach((id, index) => {
            const x = (depth + 1) * xStep;
            const y = yOffset + index * ySpace;
            layout.set(id, { x, y, depth });
        });

        yOffset += ids.length * ySpace;
    });

    return layout;
};

/**
 * Individual node component
 */
const DependencyNode: React.FC<{
    member: Member;
    position: { x: number; y: number };
    onSelect: () => void;
}> = ({ member, position, onSelect }) => {
    const [isHovered, setIsHovered] = useState(false);

    const getIcon = () => {
        switch (member.state.toLowerCase()) {
            case 'completed':
                return <CheckCircle2 className="w-4 h-4" />;
            case 'downloading':
                return <motion.div animate={{ rotate: 360 }} transition={{ duration: 2, repeat: Infinity }}>
                    <Zap className="w-4 h-4" />
                </motion.div>;
            case 'error':
                return <AlertCircle className="w-4 h-4" />;
            case 'paused':
                return <Clock className="w-4 h-4" />;
            default:
                return <Clock className="w-4 h-4" />;
        }
    };

    return (
        <motion.g
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.3 }}
            onMouseEnter={() => setIsHovered(true)}
            onMouseLeave={() => setIsHovered(false)}
            style={{ cursor: 'pointer' }}
            onClick={onSelect}
        >
            {/* Node circle */}
            <motion.circle
                cx={position.x}
                cy={position.y}
                r={isHovered ? 35 : 28}
                fill="currentColor"
                className={getStateColor(member.state)}
                animate={{ r: isHovered ? 35 : 28 }}
                transition={{ duration: 0.2 }}
            />

            {/* Progress ring */}
            <circle
                cx={position.x}
                cy={position.y}
                r={28}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                className="text-slate-600"
                opacity="0.3"
            />

            {/* Progress indicator */}
            <motion.circle
                cx={position.x}
                cy={position.y}
                r={28}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeDasharray={`${(member.progress_percent / 100) * (2 * Math.PI * 28)} ${2 * Math.PI * 28}`}
                className={getStateColor(member.state)}
                opacity="0.8"
            />

            {/* Label */}
            <text
                x={position.x}
                y={position.y}
                textAnchor="middle"
                dominantBaseline="middle"
                className="text-xs font-bold select-none pointer-events-none"
                fill="currentColor"
            >
                {(member.progress_percent.toFixed(0))}%
            </text>

            {/* Tooltip on hover */}
            {isHovered && (
                <g>
                    <rect
                        x={position.x + 35}
                        y={position.y - 25}
                        width="200"
                        height="50"
                        rx="4"
                        fill="rgb(15, 23, 42)"
                        stroke="rgb(30, 58, 138)"
                        strokeWidth="1"
                    />
                    <text
                        x={position.x + 40}
                        y={position.y - 10}
                        className="text-xs font-mono"
                        fill="rgb(206, 217, 224)"
                    >
                        {member.id.substring(0, 20)}
                    </text>
                    <text
                        x={position.x + 40}
                        y={position.y + 5}
                        className="text-xs"
                        fill="rgb(148, 163, 184)"
                    >
                        {member.state} - {member.progress_percent.toFixed(1)}%
                    </text>
                </g>
            )}
        </motion.g>
    );
};

/**
 * Edge (dependency line) component
 */
const DependencyEdge: React.FC<{
    fromPos: { x: number; y: number };
    toPos: { x: number; y: number };
    isCritical?: boolean;
}> = ({ fromPos, toPos, isCritical = false }) => {
    // Bezier curve path
    const ctrlX = (fromPos.x + toPos.x) / 2;
    const ctrlY = (fromPos.y + toPos.y) / 2;

    const pathD = `M ${fromPos.x} ${fromPos.y} Q ${ctrlX} ${ctrlY} ${toPos.x} ${toPos.y}`;

    return (
        <motion.path
            d={pathD}
            fill="none"
            stroke={isCritical ? 'rgb(236, 253, 245)' : 'rgb(71, 85, 105)'}
            strokeWidth={isCritical ? 2 : 1}
            strokeOpacity={isCritical ? 0.8 : 0.4}
            initial={{ pathLength: 0 }}
            animate={{ pathLength: 1 }}
            transition={{ duration: 0.6 }}
        />
    );
};

/**
 * Main Dependency Visualizer Component
 */
export const GroupDependencyVisualizer: React.FC<GroupDependencyVisualizerProps> = ({
    members,
    onMemberClick = () => {},
    className = '',
}) => {
    const [selectedMemberId, setSelectedMemberId] = useState<string | null>(null);

    const layout = useMemo(() => calculateDAGLayout(members), [members]);

    // Calculate SVG dimensions
    const positions = Array.from(layout.values());
    const maxY = Math.max(...positions.map((p) => p.y), 300);
    const maxX = Math.max(...positions.map((p) => p.x), 300);

    // Identify critical path
    const findCriticalPath = (): Set<string> => {
        const critical = new Set<string>();
        let current = Array.from(members.values()).reduce((prev, curr) =>
            (layout.get(curr.id)?.depth ?? -1) > (layout.get(prev.id)?.depth ?? -1) ? curr : prev
        );

        critical.add(current.id);

        while (current.dependencies.length > 0) {
            const maxDepDep = current.dependencies
                .map((depId) => ({ id: depId, depth: layout.get(depId)?.depth ?? -1 }))
                .reduce((prev, curr) => (curr.depth > prev.depth ? curr : prev));

            critical.add(maxDepDep.id);
            current = members.get(maxDepDep.id)!;
        }

        return critical;
    };

    const criticalPath = useMemo(() => findCriticalPath(), [members, layout]);

    return (
        <motion.div
            className={`rounded-lg bg-slate-900/50 border border-slate-700/50 p-4 overflow-auto ${className}`}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.3 }}
        >
            <div className="flex items-center gap-2 mb-4">
                <AlertCircle className="w-5 h-5 text-amber-400" />
                <h3 className="font-semibold text-slate-100">Dependency Graph</h3>
            </div>

            {members.size === 0 ? (
                <div className="text-slate-400 text-sm py-8 text-center">
                    No members to visualize
                </div>
            ) : (
                <div className="overflow-x-auto">
                    <svg
                        width={maxX + 100}
                        height={maxY + 100}
                        className="min-w-full"
                        style={{ background: 'transparent' }}
                    >
                        {/* Draw edges first (they appear behind nodes) */}
                        {Array.from(members.values()).map((member) =>
                            member.dependencies.map((depId) => {
                                const depPos = layout.get(depId);
                                const memberPos = layout.get(member.id);
                                if (!depPos || !memberPos) return null;

                                return (
                                    <DependencyEdge
                                        key={`${depId}-${member.id}`}
                                        fromPos={{ x: depPos.x * (maxX + 100) / 100, y: depPos.y + 50 }}
                                        toPos={{
                                            x: memberPos.x * (maxX + 100) / 100,
                                            y: memberPos.y + 50,
                                        }}
                                        isCritical={
                                            criticalPath.has(depId) && criticalPath.has(member.id)
                                        }
                                    />
                                );
                            })
                        )}

                        {/* Draw nodes */}
                        {Array.from(members.values()).map((member) => {
                            const pos = layout.get(member.id);
                            if (!pos) return null;

                            return (
                                <DependencyNode
                                    key={member.id}
                                    member={member}
                                    position={{
                                        x: (pos.x * (maxX + 100)) / 100,
                                        y: pos.y + 50,
                                    }}
                                    onSelect={() => {
                                        setSelectedMemberId(member.id);
                                        onMemberClick(member.id);
                                    }}
                                />
                            );
                        })}
                    </svg>
                </div>
            )}

            {/* Legend */}
            <div className="mt-4 flex flex-wrap gap-4 text-xs">
                <div className="flex items-center gap-2">
                    <div className="w-3 h-3 rounded-full bg-green-500/50" />
                    <span className="text-slate-400">Completed</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-3 h-3 rounded-full bg-blue-500/50" />
                    <span className="text-slate-400">Downloading</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-3 h-3 rounded-full bg-amber-500/50" />
                    <span className="text-slate-400">Paused</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-3 h-3 rounded-full bg-red-500/50" />
                    <span className="text-slate-400">Error</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="h-0.5 w-4 bg-emerald-400" />
                    <span className="text-slate-400">Critical Path</span>
                </div>
            </div>
        </motion.div>
    );
};

export default GroupDependencyVisualizer;
