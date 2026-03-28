/// Download History Analytics Dashboard
///
/// Displays comprehensive analytics on past downloads with insights,
/// recommendations, and performance patterns

import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
    TrendingUp,
    BarChart3,
    Clock,
    AlertCircle,
    FileType,
    Zap,
    Target,
    CheckCircle,
    XCircle,
    Download,
    Eye,
    RefreshCw,
} from 'lucide-react';

interface FileTypeInsight {
    file_type: string;
    total_downloads: number;
    successful: number;
    success_rate: number;
    avg_speed_mbps: number;
    avg_duration_seconds: number;
    common_failure_reasons: [string, number][];
}

interface TimeWindowInsight {
    hour_of_day: number;
    downloads_in_window: number;
    success_rate: number;
    avg_speed_mbps: number;
    peak_hours: number[];
    low_hours: number[];
}

interface MirrorAnalytic {
    mirror_host: string;
    total_downloads: number;
    successful: number;
    success_rate: number;
    avg_speed_mbps: number;
    is_cdn: boolean;
    failure_count: number;
    reliability_trend: string;
}

interface Recommendation {
    recommendation_id: string;
    title: string;
    description: string;
    category: string;
    expected_improvement: number;
    confidence: number;
    action: string;
}

interface AnalyticsSnapshot {
    total_downloads: number;
    successful_downloads: number;
    overall_success_rate: number;
    total_bytes_downloaded: number;
    total_time_seconds: number;
    avg_speed_mbps: number;
    avg_duration_seconds: number;
    total_retries: number;
    file_type_insights: FileTypeInsight[];
    time_window_insights: TimeWindowInsight[];
    mirror_analytics: MirrorAnalytic[];
    recommendations: Recommendation[];
    failure_patterns: [string, number][];
    best_time_to_download: string;
    worst_mirror: string | null;
    best_mirror: string | null;
}

const DownloadHistoryAnalytics: React.FC = () => {
    const [analytics, setAnalytics] = useState<AnalyticsSnapshot | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [expandedMirror, setExpandedMirror] = useState<string | null>(null);
    const [expandedFileType, setExpandedFileType] = useState<string | null>(null);
    const [filterCategory, setFilterCategory] = useState<'all' | 'timing' | 'mirror' | 'file-type' | 'strategy'>('all');

    useEffect(() => {
        loadAnalytics();
    }, []);

    const loadAnalytics = async () => {
        try {
            setLoading(true);
            const result = await invoke<AnalyticsSnapshot>('get_download_analytics');
            setAnalytics(result);
            setError(null);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setLoading(false);
        }
    };

    const formatBytes = (bytes: number): string => {
        if (bytes === 0) return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
    };

    const formatDuration = (seconds: number): string => {
        if (seconds < 60) return `${Math.round(seconds)}s`;
        if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
        return `${Math.round(seconds / 3600)}h`;
    };

    const getSuccessColor = (rate: number): string => {
        if (rate >= 0.9) return 'text-green-400';
        if (rate >= 0.75) return 'text-blue-400';
        if (rate >= 0.5) return 'text-yellow-400';
        return 'text-red-400';
    };

    const getSuccessBgColor = (rate: number): string => {
        if (rate >= 0.9) return 'bg-green-500/20';
        if (rate >= 0.75) return 'bg-blue-500/20';
        if (rate >= 0.5) return 'bg-yellow-500/20';
        return 'bg-red-500/20';
    };

    const getCategoryColor = (category: string): string => {
        switch (category) {
            case 'timing':
                return 'bg-blue-500/20 border-blue-500/50';
            case 'mirror':
                return 'bg-purple-500/20 border-purple-500/50';
            case 'file-type':
                return 'bg-cyan-500/20 border-cyan-500/50';
            case 'strategy':
                return 'bg-green-500/20 border-green-500/50';
            default:
                return 'bg-gray-500/20 border-gray-500/50';
        }
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center h-96">
                <motion.div
                    animate={{ rotate: 360 }}
                    transition={{ duration: 2, repeat: Infinity }}
                    className="text-cyan-400"
                >
                    <RefreshCw size={48} />
                </motion.div>
            </div>
        );
    }

    if (error) {
        return (
            <div className="p-6 rounded-lg bg-red-500/10 border border-red-500/50">
                <div className="flex items-center gap-3 text-red-400">
                    <AlertCircle size={20} />
                    <span>{error}</span>
                </div>
            </div>
        );
    }

    if (!analytics) {
        return <div className="text-gray-400">No analytics data available</div>;
    }

    const filteredRecommendations = filterCategory === 'all'
        ? analytics.recommendations
        : analytics.recommendations.filter(r => r.category === filterCategory);

    return (
        <div className="space-y-6 p-6">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-3xl font-bold text-white">Download History Analytics</h1>
                    <p className="text-gray-400 mt-2">
                        Comprehensive insights from {analytics.total_downloads} downloads
                    </p>
                </div>
                <motion.button
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.95 }}
                    onClick={loadAnalytics}
                    className="px-4 py-2 rounded-lg bg-cyan-500/20 border border-cyan-500/50 text-cyan-400 hover:bg-cyan-500/30 transition-colors flex items-center gap-2"
                >
                    <RefreshCw size={16} />
                    Refresh
                </motion.button>
            </div>

            {/* Overall Statistics */}
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="grid grid-cols-2 lg:grid-cols-4 gap-4"
            >
                <div className="p-4 rounded-lg bg-gradient-to-br from-blue-500/20 to-blue-600/10 border border-blue-500/30">
                    <div className="flex items-center justify-between">
                        <div>
                            <p className="text-gray-400 text-sm">Total Downloads</p>
                            <p className="text-2xl font-bold text-blue-400">
                                {analytics.total_downloads}
                            </p>
                        </div>
                        <Download className="text-blue-400/50" size={32} />
                    </div>
                </div>

                <div className={`p-4 rounded-lg bg-gradient-to-br border border-opacity-30`}
                    style={{
                        backgroundImage: `linear-gradient(to bottom right, ${analytics.overall_success_rate >= 0.9 ? 'rgba(34, 197, 94, 0.2)' : analytics.overall_success_rate >= 0.75 ? 'rgba(59, 130, 246, 0.2)' : 'rgba(245, 158, 11, 0.2)'}, ${analytics.overall_success_rate >= 0.9 ? 'rgba(34, 197, 94, 0.1)' : analytics.overall_success_rate >= 0.75 ? 'rgba(59, 130, 246, 0.1)' : 'rgba(245, 158, 11, 0.1)'})`,
                        borderColor: analytics.overall_success_rate >= 0.9 ? 'rgba(34, 197, 94, 0.3)' : analytics.overall_success_rate >= 0.75 ? 'rgba(59, 130, 246, 0.3)' : 'rgba(245, 158, 11, 0.3)',
                    }}>
                    <div className="flex items-center justify-between">
                        <div>
                            <p className="text-gray-400 text-sm">Success Rate</p>
                            <p className={`text-2xl font-bold ${getSuccessColor(analytics.overall_success_rate)}`}>
                                {Math.round(analytics.overall_success_rate * 100)}%
                            </p>
                        </div>
                        <CheckCircle className={`${getSuccessColor(analytics.overall_success_rate)}`} size={32} />
                    </div>
                </div>

                <div className="p-4 rounded-lg bg-gradient-to-br from-purple-500/20 to-purple-600/10 border border-purple-500/30">
                    <div className="flex items-center justify-between">
                        <div>
                            <p className="text-gray-400 text-sm">Avg Speed</p>
                            <p className="text-2xl font-bold text-purple-400">
                                {analytics.avg_speed_mbps.toFixed(2)} Mbps
                            </p>
                        </div>
                        <Zap className="text-purple-400/50" size={32} />
                    </div>
                </div>

                <div className="p-4 rounded-lg bg-gradient-to-br from-green-500/20 to-green-600/10 border border-green-500/30">
                    <div className="flex items-center justify-between">
                        <div>
                            <p className="text-gray-400 text-sm">Total Data</p>
                            <p className="text-2xl font-bold text-green-400">
                                {formatBytes(analytics.total_bytes_downloaded)}
                            </p>
                        </div>
                        <BarChart3 className="text-green-400/50" size={32} />
                    </div>
                </div>
            </motion.div>

            {/* Smart Recommendations */}
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: 0.1 }}
                className="p-6 rounded-lg bg-gradient-to-br from-cyan-500/10 to-blue-500/10 border border-cyan-500/30"
            >
                <div className="flex items-center justify-between mb-4">
                    <div className="flex items-center gap-3">
                        <Target className="text-cyan-400" size={24} />
                        <h2 className="text-xl font-bold text-white">Smart Recommendations</h2>
                    </div>
                    <div className="flex gap-2 flex-wrap justify-end">
                        {['all', 'timing', 'mirror', 'file-type', 'strategy'].map(cat => (
                            <motion.button
                                key={cat}
                                whileHover={{ scale: 1.05 }}
                                whileTap={{ scale: 0.95 }}
                                onClick={() => setFilterCategory(cat as any)}
                                className={`px-3 py-1 rounded-full text-xs font-medium transition-all ${
                                    filterCategory === cat
                                        ? 'bg-cyan-500 text-black'
                                        : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
                                }`}
                            >
                                {cat.charAt(0).toUpperCase() + cat.slice(1)}
                            </motion.button>
                        ))}
                    </div>
                </div>

                {filteredRecommendations.length === 0 ? (
                    <p className="text-gray-400">No recommendations for this filter</p>
                ) : (
                    <div className="space-y-3">
                        {filteredRecommendations.map((rec, idx) => (
                            <motion.div
                                key={rec.recommendation_id}
                                initial={{ opacity: 0, x: -20 }}
                                animate={{ opacity: 1, x: 0 }}
                                transition={{ delay: idx * 0.05 }}
                                className={`p-4 rounded-lg border flex items-start gap-4 ${getCategoryColor(rec.category)}`}
                            >
                                <div className="mt-1">
                                    <Target size={20} className="text-cyan-400" />
                                </div>
                                <div className="flex-1">
                                    <p className="font-semibold text-white">{rec.title}</p>
                                    <p className="text-gray-300 text-sm mt-1">{rec.description}</p>
                                    <p className="text-cyan-400 text-sm mt-2">💡 {rec.action}</p>
                                    <div className="flex gap-4 mt-2 text-xs text-gray-400">
                                        <span>📈 Expected improvement: {Math.round(rec.expected_improvement * 100)}%</span>
                                        <span>⭐ Confidence: {Math.round(rec.confidence * 100)}%</span>
                                    </div>
                                </div>
                            </motion.div>
                        ))}
                    </div>
                )}
            </motion.div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* Best/Worst Times */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.2 }}
                    className="p-6 rounded-lg bg-gradient-to-br from-indigo-500/10 to-purple-500/10 border border-indigo-500/30"
                >
                    <div className="flex items-center gap-3 mb-4">
                        <Clock className="text-indigo-400" size={24} />
                        <h3 className="text-lg font-bold text-white">Best Download Times</h3>
                    </div>
                    
                    {analytics.time_window_insights.length > 0 ? (
                        <div className="space-y-3">
                            {analytics.time_window_insights.slice(0, 5).map((tw, idx) => (
                                <div key={idx} className="flex items-center justify-between">
                                    <span className="text-gray-300">{String(tw.hour_of_day).padStart(2, '0')}:00</span>
                                    <div className="flex-1 mx-4 h-2 bg-gray-700 rounded-full overflow-hidden">
                                        <motion.div
                                            initial={{ width: 0 }}
                                            animate={{ width: `${tw.success_rate * 100}%` }}
                                            transition={{ duration: 0.5 }}
                                            className="h-full bg-gradient-to-r from-green-500 to-cyan-500"
                                        />
                                    </div>
                                    <span className="text-indigo-400 font-semibold">
                                        {Math.round(tw.success_rate * 100)}%
                                    </span>
                                </div>
                            ))}
                        </div>
                    ) : (
                        <p className="text-gray-400">No time window data</p>
                    )}
                </motion.div>

                {/* Best / Worst Mirrors */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.3 }}
                    className="p-6 rounded-lg bg-gradient-to-br from-orange-500/10 to-red-500/10 border border-orange-500/30"
                >
                    <div className="flex items-center gap-3 mb-4">
                        <FileType className="text-orange-400" size={24} />
                        <h3 className="text-lg font-bold text-white">Top Mirrors</h3>
                    </div>

                    {analytics.mirror_analytics.length > 0 ? (
                        <div className="space-y-3">
                            {analytics.mirror_analytics.slice(0, 3).map((mirror, idx) => (
                                <motion.div
                                    key={idx}
                                    whileHover={{ scale: 1.02 }}
                                    onClick={() => setExpandedMirror(expandedMirror === mirror.mirror_host ? null : mirror.mirror_host)}
                                    className="p-3 rounded-lg bg-gray-800/50 border border-gray-700 cursor-pointer hover:border-orange-500/50 transition-all"
                                >
                                    <div className="flex items-center justify-between">
                                        <div className="flex-1">
                                            <p className="font-semibold text-gray-200 truncate">{mirror.mirror_host}</p>
                                            <p className="text-sm text-gray-400">
                                                {mirror.successful} / {mirror.total_downloads} successful
                                            </p>
                                        </div>
                                        <div className="text-right">
                                            <p className={`font-semibold ${getSuccessColor(mirror.success_rate)}`}>
                                                {Math.round(mirror.success_rate * 100)}%
                                            </p>
                                            <p className="text-sm text-gray-400">
                                                {mirror.avg_speed_mbps.toFixed(2)} Mbps
                                            </p>
                                        </div>
                                    </div>

                                    <AnimatePresence>
                                        {expandedMirror === mirror.mirror_host && (
                                            <motion.div
                                                initial={{ opacity: 0, height: 0 }}
                                                animate={{ opacity: 1, height: 'auto' }}
                                                exit={{ opacity: 0, height: 0 }}
                                                className="mt-3 pt-3 border-t border-gray-700 space-y-2 text-sm"
                                            >
                                                <div className="flex justify-between text-gray-300">
                                                    <span>Failures:</span>
                                                    <span className="text-red-400">{mirror.failure_count}</span>
                                                </div>
                                                <div className="flex justify-between text-gray-300">
                                                    <span>Trend:</span>
                                                    <span className="text-blue-400">{mirror.reliability_trend}</span>
                                                </div>
                                                <div className="flex justify-between text-gray-300">
                                                    <span>CDN:</span>
                                                    <span>{mirror.is_cdn ? '✓ Yes' : '✗ No'}</span>
                                                </div>
                                            </motion.div>
                                        )}
                                    </AnimatePresence>
                                </motion.div>
                            ))}
                        </div>
                    ) : (
                        <p className="text-gray-400">No mirror data</p>
                    )}
                </motion.div>
            </div>

            {/* File Type Analysis */}
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: 0.4 }}
                className="p-6 rounded-lg bg-gradient-to-br from-cyan-500/10 to-teal-500/10 border border-cyan-500/30"
            >
                <div className="flex items-center gap-3 mb-4">
                    <FileType className="text-cyan-400" size={24} />
                    <h3 className="text-lg font-bold text-white">File Type Insights</h3>
                </div>

                {analytics.file_type_insights.length > 0 ? (
                    <div className="space-y-3">
                        {analytics.file_type_insights.map((ft, idx) => (
                            <motion.div
                                key={ft.file_type}
                                whileHover={{ scale: 1.01 }}
                                onClick={() => setExpandedFileType(expandedFileType === ft.file_type ? null : ft.file_type)}
                                className="p-4 rounded-lg bg-gray-800/50 border border-gray-700 cursor-pointer hover:border-cyan-500/50 transition-all"
                            >
                                <div className="flex items-center justify-between">
                                    <div className="flex-1">
                                        <p className="font-semibold text-gray-200">.{ft.file_type}</p>
                                        <p className="text-sm text-gray-400">
                                            {ft.total_downloads} downloads · {ft.successful} successful
                                        </p>
                                    </div>
                                    <div className="text-right">
                                        <p className={`font-semibold ${getSuccessColor(ft.success_rate)}`}>
                                            {Math.round(ft.success_rate * 100)}%
                                        </p>
                                        <p className="text-sm text-gray-400">
                                            {ft.avg_speed_mbps.toFixed(2)} Mbps avg
                                        </p>
                                    </div>
                                </div>

                                <AnimatePresence>
                                    {expandedFileType === ft.file_type && (
                                        <motion.div
                                            initial={{ opacity: 0, height: 0 }}
                                            animate={{ opacity: 1, height: 'auto' }}
                                            exit={{ opacity: 0, height: 0 }}
                                            className="mt-3 pt-3 border-t border-gray-700 space-y-2 text-sm"
                                        >
                                            <div className="flex justify-between text-gray-300">
                                                <span>Avg Duration:</span>
                                                <span>{formatDuration(ft.avg_duration_seconds)}</span>
                                            </div>
                                            {ft.common_failure_reasons.length > 0 && (
                                                <div>
                                                    <p className="text-gray-400 mb-2">Common Issues:</p>
                                                    <div className="space-y-1">
                                                        {ft.common_failure_reasons.slice(0, 3).map((reason, idx) => (
                                                            <p key={idx} className="text-gray-500 text-xs">
                                                                • {reason[0]} ({reason[1]} times)
                                                            </p>
                                                        ))}
                                                    </div>
                                                </div>
                                            )}
                                        </motion.div>
                                    )}
                                </AnimatePresence>
                            </motion.div>
                        ))}
                    </div>
                ) : (
                    <p className="text-gray-400">No file type data</p>
                )}
            </motion.div>

            {/* Failure Patterns */}
            {analytics.failure_patterns.length > 0 && (
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.5 }}
                    className="p-6 rounded-lg bg-gradient-to-br from-red-500/10 to-orange-500/10 border border-red-500/30"
                >
                    <div className="flex items-center gap-3 mb-4">
                        <AlertCircle className="text-red-400" size={24} />
                        <h3 className="text-lg font-bold text-white">Common Failure Patterns</h3>
                    </div>

                    <div className="space-y-2">
                        {analytics.failure_patterns.slice(0, 5).map((pattern, idx) => (
                            <div key={idx} className="flex items-center gap-4">
                                <span className="text-gray-300 flex-1">{pattern[0]}</span>
                                <div className="w-24 h-2 bg-gray-700 rounded-full overflow-hidden">
                                    <motion.div
                                        initial={{ width: 0 }}
                                        animate={{ width: `${(pattern[1] / analytics.failure_patterns[0][1]) * 100}%` }}
                                        transition={{ duration: 0.5 }}
                                        className="h-full bg-gradient-to-r from-red-500 to-orange-500"
                                    />
                                </div>
                                <span className="text-red-400 font-semibold w-12 text-right">{pattern[1]}</span>
                            </div>
                        ))}
                    </div>
                </motion.div>
            )}
        </div>
    );
};

export default DownloadHistoryAnalytics;
