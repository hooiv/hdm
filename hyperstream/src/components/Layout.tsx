import React, { useState, useEffect } from 'react';
import { Download as DownloadCloud, Settings, Plus, LayoutGrid, Calendar, Magnet, Globe, Zap, Search, Rss, Puzzle, History, Activity, ListOrdered, ShieldAlert, Video, Wifi, Film, Globe2, FolderTree, BarChart3 } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { TitleBar } from './TitleBar';
import { motion } from 'framer-motion';
import { formatBytes, formatSpeed } from '../utils/formatters';
import { invoke } from '@tauri-apps/api/core';

interface LayoutProps {
    children: React.ReactNode;
    onAddClick: (e: React.MouseEvent) => void;
    onAddTorrentClick: () => void;
    onScheduleClick: () => void;
    onSpiderClick: () => void;
    onCrashRecoveryClick: () => void;
    onStreamDetectorClick: () => void;
    onNetworkDiagClick: () => void;
    onSpeedAccelerationClick: () => void;
    onMediaProcessingClick: () => void;
    onIpfsClick: () => void;
    onCircuitBreakerClick: () => void;
    onSettingsClick: () => void;
    onOverlayClick: () => void;
    stats: {
        total: number;
        downloading: number;
        completed: number;
        totalBytes: number;
    };
    onSpeedLimitChange: (limit: number) => void;
    activeTab: 'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins' | 'history' | 'activity' | 'queue' | 'groups' | 'orchestrator' | 'preflight' | 'analytics';
    onTabChange: (tab: 'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins' | 'history' | 'activity' | 'queue' | 'groups' | 'orchestrator' | 'preflight' | 'analytics') => void;
    onTabIntent?: (tab: 'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins' | 'history' | 'activity' | 'queue' | 'groups' | 'orchestrator' | 'preflight' | 'analytics') => void;
    globalSpeed?: number;
}

const NavItem: React.FC<{ icon: LucideIcon; label: string; active: boolean; onClick: () => void; badge?: string | number; onIntent?: () => void }> = ({ icon: Icon, label, active, onClick, badge, onIntent }) => (
    <button
        onClick={onClick}
        onMouseEnter={onIntent}
        onFocus={onIntent}
        aria-current={active ? 'page' : undefined}
        className={`
            relative px-4 py-3 cursor-pointer flex items-center gap-3 transition-all duration-300 group rounded-xl mx-2 overflow-hidden w-[calc(100%-1rem)] text-left border-0 bg-transparent
            ${active
                ? 'bg-white/5 text-cyan-400 shadow-[0_0_20px_rgba(0,242,255,0.15)] glow-active'
                : 'text-slate-500 hover:text-slate-200 hover:bg-white/5'
            }
        `}
    >
        {active && (
            <motion.div
                layoutId="activeTabGlow"
                className="absolute inset-0 bg-cyan-400/5 rounded-xl z-0"
                transition={{ duration: 0.3 }}
            />
        )}
        <Icon size={18} className={`transition-all duration-300 z-10 ${active ? 'scale-110 drop-shadow-[0_0_8px_rgba(0,242,255,0.6)] text-cyan-400' : 'group-hover:scale-105'}`} />
        <span className={`font-semibold text-[11px] tracking-wider z-10 uppercase ${active ? 'text-cyan-100' : ''}`}>{label}</span>

        {badge && (
            <span className="ml-auto text-[10px] font-bold bg-cyan-500/20 text-cyan-300 px-2 py-0.5 rounded-full shadow-sm z-10 border border-cyan-500/20">
                {badge}
            </span>
        )}

        {active && <div className="absolute left-0 top-3 bottom-3 w-1 bg-cyan-400 rounded-r-full shadow-[0_0_10px_rgba(6,182,212,0.8)]" />}
    </button>
);

export const Layout: React.FC<LayoutProps> = ({
    children,
    onAddClick,
    onAddTorrentClick,
    onScheduleClick,
    onSpiderClick,
    onCrashRecoveryClick,
    onStreamDetectorClick,
    onNetworkDiagClick,
    onSpeedAccelerationClick,
    onMediaProcessingClick,
    onIpfsClick,
    onCircuitBreakerClick,
    onSettingsClick,
    onOverlayClick,
    stats,
    onSpeedLimitChange,
    activeTab,
    onTabChange,
    onTabIntent,
    globalSpeed
}) => {
    const [speedLimit, setSpeedLimit] = useState(0);

    // Sync dropdown with persisted speed limit on mount
    useEffect(() => {
        invoke<number>('get_speed_limit').then(limit => {
            // Snap to nearest dropdown value
            const options = [0, 512, 1024, 5120, 10240];
            const closest = options.reduce((prev, curr) =>
                Math.abs(curr - limit) < Math.abs(prev - limit) ? curr : prev
            );
            setSpeedLimit(closest);
        }).catch(() => {});
    }, []);

    const handleSpeedLimitChange = (value: number) => {
        setSpeedLimit(value);
        onSpeedLimitChange(value);
    };

    return (
        <div className="flex flex-col h-screen bg-[#0b0e14] text-slate-200 font-sans selection:bg-cyan-500/30 overflow-hidden border border-white/5 shadow-2xl aurora-bg">
            <TitleBar />

            <div className="flex flex-1 pt-10 overflow-hidden relative">
                {/* Sidebar - No-Line Philosophy */}
                <div className="w-64 flex flex-col z-10 pt-4 bg-black/10 backdrop-blur-md">

                    {/* Sidebar Header - Kinetic Stats */}
                    <div className="pt-2 pb-6 px-6">
                        <div className="p-2">
                            <h2 className="text-[10px] uppercase font-bold text-slate-600 mb-1 tracking-widest pl-1">Data Transit</h2>
                            <div className="display-lg text-white mb-1">
                                {formatBytes(stats.totalBytes).split(' ')[0]}
                                <span className="text-xs font-medium text-slate-500 ml-1 uppercase">{formatBytes(stats.totalBytes).split(' ')[1]}</span>
                            </div>
                            <div className="text-[10px] font-bold text-cyan-500/80 pl-1">
                                {stats.completed} / {stats.total} COMPLETED
                            </div>
                            <div className="w-full bg-white/5 h-[2px] rounded-full mt-4 overflow-hidden">
                                <motion.div
                                    className="h-full bg-cyan-400 relative overflow-hidden shadow-[0_0_10px_#00f2ff]"
                                    initial={{ width: 0 }}
                                    animate={{ width: `${stats.total > 0 ? Math.round((stats.completed / stats.total) * 100) : 0}%` }}
                                    transition={{ duration: 1.5, ease: "easeOut" }}
                                >
                                    <div className="absolute inset-0 animate-shimmer" />
                                </motion.div>
                            </div>
                        </div>
                    </div>

                    {/* Navigation */}
                    <nav className="flex-1 space-y-1 overflow-y-auto custom-scrollbar px-2">
                        <div className="px-4 mb-2 text-[10px] font-bold text-slate-600 uppercase tracking-widest pl-4">Menu</div>

                        <NavItem
                            icon={DownloadCloud}
                            label="Downloads"
                            active={activeTab === 'downloads'}
                            onClick={() => onTabChange('downloads')}
                            badge={stats.downloading > 0 ? stats.downloading : undefined}
                        />
                        <NavItem
                            icon={Magnet}
                            label="Torrents"
                            active={activeTab === 'torrents'}
                            onClick={() => onTabChange('torrents')}
                            onIntent={() => onTabIntent?.('torrents')}
                        />
                        <NavItem
                            icon={Rss}
                            label="Feeds"
                            active={activeTab === 'feeds'}
                            onClick={() => onTabChange('feeds')}
                            onIntent={() => onTabIntent?.('feeds')}
                        />
                        <NavItem
                            icon={Search}
                            label="Discover"
                            active={activeTab === 'search'}
                            onClick={() => onTabChange('search')}
                            onIntent={() => onTabIntent?.('search')}
                        />
                        <NavItem
                            icon={Puzzle}
                            label="Plugins"
                            active={activeTab === 'plugins'}
                            onClick={() => onTabChange('plugins')}
                            onIntent={() => onTabIntent?.('plugins')}
                        />
                        <NavItem
                            icon={History}
                            label="History"
                            active={activeTab === 'history'}
                            onClick={() => onTabChange('history')}
                            onIntent={() => onTabIntent?.('history')}
                        />
                        <NavItem
                            icon={Activity}
                            label="Activity Log"
                            active={activeTab === 'activity'}
                            onClick={() => onTabChange('activity')}
                            onIntent={() => onTabIntent?.('activity')}
                        />
                        <NavItem
                            icon={ListOrdered}
                            label="Queue"
                            active={activeTab === 'queue'}
                            onClick={() => onTabChange('queue')}
                            onIntent={() => onTabIntent?.('queue')}
                        />
                        <NavItem
                            icon={FolderTree}
                            label="Groups"
                            active={activeTab === 'groups'}
                            onClick={() => onTabChange('groups')}
                            onIntent={() => onTabIntent?.('groups')}
                        />
                        <NavItem
                            icon={Zap}
                            label="Orchestrator"
                            active={activeTab === 'orchestrator'}
                            onClick={() => onTabChange('orchestrator')}
                            onIntent={() => onTabIntent?.('orchestrator')}
                        />
                        <NavItem
                            icon={Globe}
                            label="Pre-Flight"
                            active={activeTab === 'preflight'}
                            onClick={() => onTabChange('preflight')}
                            onIntent={() => onTabIntent?.('preflight')}
                        />
                        <NavItem
                            icon={BarChart3}
                            label="Analytics"
                            active={activeTab === 'analytics'}
                            onClick={() => onTabChange('analytics')}
                            onIntent={() => onTabIntent?.('analytics')}
                        />

                        <div className="px-4 mt-8 mb-2 text-[10px] font-bold text-slate-600 uppercase tracking-widest pl-4">Tools</div>

                        <NavItem
                            icon={Calendar}
                            label="Scheduler"
                            active={false}
                            onClick={onScheduleClick}
                        />
                        <NavItem
                            icon={Globe}
                            label="Site Grabber"
                            active={false}
                            onClick={onSpiderClick}
                        />
                    </nav>

                    {/* Footer Actions */}
                    <div className="p-4 border-t border-white/5 space-y-2 bg-black/20">
                        <button
                            onClick={onCrashRecoveryClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <ShieldAlert size={16} className="group-hover:text-orange-400 transition-colors" />
                            <span>Crash Recovery</span>
                        </button>
                        <button
                            onClick={onStreamDetectorClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Video size={16} className="group-hover:text-cyan-400 transition-colors" />
                            <span>Stream Detector</span>
                        </button>
                        <button
                            onClick={onNetworkDiagClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Wifi size={16} className="group-hover:text-emerald-400 transition-colors" />
                            <span>Network Diagnostics</span>
                        </button>
                        <button
                            onClick={onMediaProcessingClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Film size={16} className="group-hover:text-violet-400 transition-colors" />
                            <span>Media Processing</span>
                        </button>
                        <button
                            onClick={onIpfsClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Globe2 size={16} className="group-hover:text-teal-400 transition-colors" />
                            <span>IPFS Download</span>
                        </button>
                        <button
                            onClick={onCircuitBreakerClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Zap size={16} className="group-hover:text-yellow-400 transition-colors" />
                            <span>Circuit Breaker</span>
                        </button>
                        <button
                            onClick={onOverlayClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <LayoutGrid size={16} className="group-hover:text-cyan-400 transition-colors" />
                            <span>Compact Overlay</span>
                        </button>
                        <button
                            onClick={onSettingsClick}
                            className="w-full py-2.5 px-3 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-white/5 transition-all flex items-center gap-2 group"
                        >
                            <Settings size={16} className="group-hover:text-cyan-400 transition-colors" />
                            <span>Preferences</span>
                        </button>
                    </div>
                </div>

                {/* Main Content Area */}
                <div className="flex-1 flex flex-col min-w-0 bg-transparent relative">
                    {/* Top Bar - Enhanced Glassmorphism */}
                    <header className="h-20 flex items-center justify-between px-8 bg-white/[0.02] backdrop-blur-xl z-20">
                        <div className="flex items-center gap-4">
                            <h1 className="text-xl font-bold text-white tracking-tight">
                                {activeTab === 'downloads' && 'Downloads'}
                                {activeTab === 'torrents' && 'Torrents'}
                                {activeTab === 'feeds' && 'Feeds'}
                                {activeTab === 'search' && 'Discover'}
                                {activeTab === 'plugins' && 'Plugins'}
                                {activeTab === 'history' && 'History'}
                                {activeTab === 'activity' && 'Activity'}
                                {activeTab === 'queue' && 'Queue'}
                            </h1>
                            {activeTab === 'downloads' && (
                                <span className="bg-cyan-500/10 text-cyan-400 text-[10px] px-3 py-1 rounded-full font-bold border border-cyan-500/20 tracking-widest uppercase">
                                    {stats.downloading > 0 ? `${stats.downloading} Active` : 'Idle'}
                                </span>
                            )}
                        </div>

                        <div className="flex items-center gap-3">
                            {/* Live Speed Indicator - Velocity Engine Style */}
                            {(globalSpeed ?? 0) > 0 && (
                                <div className="flex items-center gap-4 mr-4">
                                    <div className="text-right">
                                        <div className="text-[10px] font-bold text-slate-600 uppercase tracking-widest">Velocity</div>
                                        <div className="text-lg font-bold text-white font-mono leading-none">
                                            {formatSpeed(globalSpeed ?? 0).split(' ')[0]}
                                            <span className="text-[10px] text-cyan-400 ml-1 uppercase">{formatSpeed(globalSpeed ?? 0).split(' ')[1]}</span>
                                        </div>
                                    </div>
                                    <div className="h-8 w-px bg-white/5" />
                                </div>
                            )}
                            <div className="flex items-center bg-white/5 rounded-xl px-2 py-1 border border-white/5">
                                <Zap size={14} className="ml-2 text-amber-400" />
                                <select
                                    aria-label="Speed limit"
                                    className="bg-transparent border-none text-xs text-slate-300 focus:ring-0 cursor-pointer py-1 pl-2 pr-6 font-medium outline-none"
                                    value={speedLimit}
                                    onChange={(e) => handleSpeedLimitChange(parseInt(e.target.value))}
                                >
                                    <option value="0" className="bg-slate-900 text-white">Unlimited Speed</option>
                                    <option value="512" className="bg-slate-900 text-white">Limit: 512 KB/s</option>
                                    <option value="1024" className="bg-slate-900 text-white">Limit: 1 MB/s</option>
                                    <option value="5120" className="bg-slate-900 text-white">Limit: 5 MB/s</option>
                                    <option value="10240" className="bg-slate-900 text-white">Limit: 10 MB/s</option>
                                </select>
                            </div>

                            <motion.button
                                whileHover={{ scale: 1.02, backgroundColor: "rgba(255,255,255,0.1)" }}
                                whileTap={{ scale: 0.98 }}
                                onClick={onAddTorrentClick}
                                className="bg-white/5 text-slate-200 px-4 py-2 rounded-lg text-xs font-bold transition-all border border-white/10 flex items-center gap-2 hover:shadow-[0_0_15px_rgba(255,255,255,0.05)]"
                            >
                                <Magnet size={16} className="text-violet-400" />
                                Add Magnet
                            </motion.button>

                            <motion.button
                                whileHover={{ scale: 1.05 }}
                                whileTap={{ scale: 0.95 }}
                                onClick={onAddClick}
                                className="bg-cyan-500 text-black px-6 py-2.5 rounded-xl text-[11px] font-black uppercase tracking-tighter transition-all flex items-center gap-2 shadow-[0_0_20px_rgba(0,242,255,0.4)] hover:shadow-[0_0_30px_rgba(0,242,255,0.6)]"
                            >
                                <Plus size={16} strokeWidth={4} />
                                New Task
                            </motion.button>
                        </div>
                    </header>

                    {/* Content Viewport */}
                    <main className="flex-1 overflow-hidden relative p-0">
                        {children}
                    </main>
                </div>
            </div>
        </div>
    );
};
