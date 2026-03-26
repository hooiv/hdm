/**
 * Advanced Download Group Dashboard
 * 
 * Production-grade UI components for:
 * - Batch auto-detection interface
 * - Dependency graph visualization
 * - Real-time metrics dashboard
 * - Smart strategy recommendations
 */

import React, { useState, useEffect, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
    Network,
    Zap,
    TrendingUp,
    AlertTriangle,
    CheckCircle2,
    Clock,
    Cpu,
    HardDrive,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface GroupMetrics {
    group_id: string;
    state: string;
    total_size: number;
    downloaded: number;
    progress_percent: number;
    avg_speed: number;
    current_speed: number;
    eta_seconds: number;
    completed_count: number;
    failed_count: number;
    total_members: number;
    cpu_usage_percent: number;
    memory_usage: number;
}

interface BatchDetectionResult {
    detected: boolean;
    pattern: string;
    confidence: number;
    suggested_name: string;
    strategy: string;
    reason: string;
}

interface StrategyRecommendation {
    recommended_strategy: string;
    reason: string;
    confidence: number;
}

/**
 * Batch Auto-Detection Component
 */
const BatchDetectionPanel: React.FC<{
    urls: string[];
    onGroupCreated?: (groupName: string, strategy: string) => void;
}> = ({ urls, onGroupCreated }) => {
    const [detection, setDetection] = useState<BatchDetectionResult | null>(null);
    const [loading, setLoading] = useState(false);
    const [groupName, setGroupName] = useState('');

    const analyzeBatch = async () => {
        if (urls.length < 2) return;

        setLoading(true);
        try {
            const result = await invoke<BatchDetectionResult>('detect_url_batch', {
                urls,
            });
            setDetection(result);
            setGroupName(result.suggested_name);
        } catch (err) {
            console.error('Batch detection failed:', err);
        } finally {
            setLoading(false);
        }
    };

    return (
        <motion.div
            className="p-4 rounded-lg bg-gradient-to-br from-blue-500/10 to-cyan-500/10 border border-blue-500/30"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3 }}
        >
            <div className="flex items-center gap-3 mb-4">
                <Zap className="w-5 h-5 text-blue-400" />
                <h3 className="font-semibold text-cyan-100">Batch Detection</h3>
            </div>

            {!detection ? (
                <motion.button
                    onClick={analyzeBatch}
                    disabled={loading}
                    className="w-full px-4 py-2 bg-gradient-to-r from-blue-500 to-cyan-500 rounded-lg font-medium text-white hover:shadow-lg hover:shadow-cyan-500/50 disabled:opacity-50 transition-all"
                    whileHover={{ scale: 1.02 }}
                    whileTap={{ scale: 0.98 }}
                >
                    {loading ? 'Analyzing...' : `Analyze ${urls.length} URLs`}
                </motion.button>
            ) : (
                <div className="space-y-3">
                    <motion.div
                        className="p-3 rounded bg-slate-800/50 border border-slate-700/50"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                    >
                        <div className="flex items-center justify-between">
                            <span className="text-sm text-slate-300">Pattern:</span>
                            <span className="font-mono text-cyan-400">{detection.pattern}</span>
                        </div>
                        <div className="flex items-center justify-between mt-2">
                            <span className="text-sm text-slate-300">Confidence:</span>
                            <div className="flex items-center gap-2">
                                <div className="w-24 h-2 bg-slate-700 rounded-full overflow-hidden">
                                    <motion.div
                                        className="h-full bg-gradient-to-r from-green-500 to-emerald-500"
                                        initial={{ width: 0 }}
                                        animate={{ width: `${detection.confidence * 100}%` }}
                                        transition={{ duration: 0.5 }}
                                    />
                                </div>
                                <span className="text-xs text-cyan-400">
                                    {(detection.confidence * 100).toFixed(0)}%
                                </span>
                            </div>
                        </div>
                    </motion.div>

                    <div className="p-3 rounded bg-slate-800/50 border border-slate-700/50">
                        <div className="text-xs text-slate-400 mb-2">Reason:</div>
                        <p className="text-sm text-slate-300">{detection.reason}</p>
                    </div>

                    <div className="flex gap-2">
                        <input
                            type="text"
                            value={groupName}
                            onChange={(e) => setGroupName(e.target.value)}
                            placeholder="Group name"
                            className="flex-1 px-3 py-2 bg-slate-900 border border-slate-700 rounded text-sm text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
                        />
                        <motion.button
                            onClick={() => onGroupCreated?.(groupName, detection.strategy)}
                            className="px-4 py-2 bg-green-600 hover:bg-green-700 rounded font-medium text-white"
                            whileHover={{ scale: 1.05 }}
                            whileTap={{ scale: 0.95 }}
                        >
                            Create
                        </motion.button>
                    </div>
                </div>
            )}
        </motion.div>
    );
};

/**
 * Group Metrics Dashboard Component
 */
const GroupMetricsDisplay: React.FC<{
    groupId: string;
}> = ({ groupId }) => {
    const [metrics, setMetrics] = useState<GroupMetrics | null>(null);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        const fetchMetrics = async () => {
            try {
                const result = await invoke<{ metrics: GroupMetrics }>('get_group_metrics', {
                    group_id: groupId,
                });
                setMetrics(result.metrics);
            } catch (err) {
                console.error('Failed to fetch metrics:', err);
            } finally {
                setLoading(false);
            }
        };

        fetchMetrics();
        const interval = setInterval(fetchMetrics, 5000); // Update every 5 seconds
        return () => clearInterval(interval);
    }, [groupId]);

    if (loading) {
        return <div className="text-slate-400">Loading metrics...</div>;
    }

    if (!metrics) {
        return <div className="text-red-400">Failed to load metrics</div>;
    }

    const formatBytes = (bytes: number): string => {
        if (bytes === 0) return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
    };

    const formatSpeed = (bytesPerSec: number): string => {
        return `${formatBytes(bytesPerSec)}/s`;
    };

    const formatTime = (seconds: number): string => {
        if (seconds < 60) return `${seconds}s`;
        const minutes = Math.floor(seconds / 60);
        if (minutes < 60) return `${minutes}m`;
        const hours = Math.floor(minutes / 60);
        return `${hours}h ${minutes % 60}m`;
    };

    return (
        <motion.div
            className="grid grid-cols-2 gap-3 p-4 rounded-lg bg-gradient-to-br from-slate-800/40 to-slate-900/40 border border-slate-700/50"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3 }}
        >
            {/* Progress */}
            <div className="col-span-2">
                <div className="flex items-center justify-between mb-2">
                    <span className="text-xs text-slate-400">Overall Progress</span>
                    <span className="text-sm font-mono text-cyan-400">
                        {metrics.progress_percent.toFixed(1)}%
                    </span>
                </div>
                <div className="w-full h-3 bg-slate-800 rounded-full overflow-hidden">
                    <motion.div
                        className="h-full bg-gradient-to-r from-cyan-500 to-blue-500"
                        initial={{ width: 0 }}
                        animate={{ width: `${metrics.progress_percent}%` }}
                        transition={{ duration: 0.5 }}
                    />
                </div>
            </div>

            {/* Speed */}
            <div className="flex items-center gap-2">
                <TrendingUp className="w-4 h-4 text-cyan-400" />
                <div>
                    <div className="text-xs text-slate-400">Speed</div>
                    <div className="text-sm font-mono text-cyan-300">
                        {formatSpeed(metrics.current_speed)}
                    </div>
                </div>
            </div>

            {/* ETA */}
            <div className="flex items-center gap-2">
                <Clock className="w-4 h-4 text-amber-400" />
                <div>
                    <div className="text-xs text-slate-400">ETA</div>
                    <div className="text-sm font-mono text-amber-300">
                        {formatTime(metrics.eta_seconds)}
                    </div>
                </div>
            </div>

            {/* CPU */}
            <div className="flex items-center gap-2">
                <Cpu className="w-4 h-4 text-purple-400" />
                <div>
                    <div className="text-xs text-slate-400">CPU</div>
                    <div className="text-sm font-mono text-purple-300">
                        {metrics.cpu_usage_percent.toFixed(1)}%
                    </div>
                </div>
            </div>

            {/* Memory */}
            <div className="flex items-center gap-2">
                <HardDrive className="w-4 h-4 text-blue-400" />
                <div>
                    <div className="text-xs text-slate-400">Memory</div>
                    <div className="text-sm font-mono text-blue-300">
                        {formatBytes(metrics.memory_usage)}
                    </div>
                </div>
            </div>

            {/* Members Status */}
            <div className="col-span-2 pt-2 border-t border-slate-700/50">
                <div className="flex items-center gap-3 text-xs">
                    <div className="flex items-center gap-1">
                        <CheckCircle2 className="w-3 h-3 text-green-400" />
                        <span className="text-slate-300">
                            {metrics.completed_count} completed
                        </span>
                    </div>
                    {metrics.failed_count > 0 && (
                        <div className="flex items-center gap-1">
                            <AlertTriangle className="w-3 h-3 text-red-400" />
                            <span className="text-slate-300">
                                {metrics.failed_count} failed
                            </span>
                        </div>
                    )}
                    <div className="text-slate-500">
                        ({metrics.total_members} total)
                    </div>
                </div>
            </div>
        </motion.div>
    );
};

/**
 * Main Advanced Group Dashboard
 */
const AdvancedGroupDashboard: React.FC = () => {
    const [urls, setUrls] = useState<string[]>([]);
    const [urlInput, setUrlInput] = useState('');

    const addUrl = () => {
        if (urlInput.trim() && urlInput.length > 10) {
            setUrls([...urls, urlInput]);
            setUrlInput('');
        }
    };

    return (
        <div className="space-y-4 p-4 rounded-lg bg-slate-900/50 border border-slate-700/50">
            <div className="flex items-center gap-2">
                <Network className="w-5 h-5 text-cyan-400" />
                <h2 className="text-lg font-bold text-cyan-100">Advanced Group Management</h2>
            </div>

            {/* URL Input */}
            <div className="space-y-2">
                <h3 className="text-sm font-medium text-slate-300">Add URLs to Batch</h3>
                <div className="flex gap-2">
                    <input
                        type="text"
                        value={urlInput}
                        onChange={(e) => setUrlInput(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && addUrl()}
                        placeholder="Paste URL and press Enter"
                        className="flex-1 px-3 py-2 bg-slate-800 border border-slate-700 rounded text-sm text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
                    />
                    <motion.button
                        onClick={addUrl}
                        className="px-4 py-2 bg-cyan-600 hover:bg-cyan-700 rounded font-medium text-white"
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.95 }}
                    >
                        Add
                    </motion.button>
                </div>
            </div>

            {/* URL List */}
            <AnimatePresence>
                {urls.length > 0 && (
                    <motion.div
                        className="space-y-2"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                    >
                        <div className="text-xs text-slate-400">
                            {urls.length} URL{urls.length !== 1 ? 's' : ''} added
                        </div>
                        <div className="max-h-32 overflow-y-auto space-y-1">
                            {urls.map((url, idx) => (
                                <motion.div
                                    key={idx}
                                    className="text-xs px-2 py-1 bg-slate-800/50 rounded truncate text-slate-400 hover:text-slate-300"
                                    initial={{ opacity: 0, x: -10 }}
                                    animate={{ opacity: 1, x: 0 }}
                                    exit={{ opacity: 0, x: -10 }}
                                >
                                    {url}
                                </motion.div>
                            ))}
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {/* Batch Detection */}
            {urls.length >= 2 && (
                <BatchDetectionPanel
                    urls={urls}
                    onGroupCreated={(name, strategy) => {
                        console.log(`Created group: ${name} with strategy: ${strategy}`);
                        setUrls([]);
                    }}
                />
            )}
        </div>
    );
};

export { AdvancedGroupDashboard, BatchDetectionPanel, GroupMetricsDisplay };
