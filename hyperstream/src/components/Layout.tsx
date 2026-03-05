import React, { useState, useEffect } from 'react';
import { Download as DownloadCloud, Settings, Plus, LayoutGrid, Calendar, Magnet, Globe, Zap, Search, Rss, Puzzle, ArrowDownToLine } from 'lucide-react';
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
    onSettingsClick: () => void;
    onOverlayClick: () => void;
    stats: {
        total: number;
        downloading: number;
        completed: number;
        totalBytes: number;
    };
    onSpeedLimitChange: (limit: number) => void;
    activeTab: 'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins';
    onTabChange: (tab: 'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins') => void;
    globalSpeed?: number;
}

const NavItem: React.FC<{ icon: LucideIcon; label: string; active: boolean; onClick: () => void; badge?: string | number }> = ({ icon: Icon, label, active, onClick, badge }) => (
    <button
        onClick={onClick}
        aria-current={active ? 'page' : undefined}
        className={`
            relative px-4 py-3 cursor-pointer flex items-center gap-3 transition-all duration-300 group rounded-xl mx-2 overflow-hidden w-[calc(100%-1rem)] text-left border-0 bg-transparent
            ${active
                ? 'bg-cyan-500/10 text-cyan-400 shadow-[0_0_15px_rgba(6,182,212,0.1)] border border-cyan-500/20'
                : 'text-slate-400 hover:text-white hover:bg-white/5 hover:pl-5'
            }
        `}
    >
        {active && (
            <motion.div
                layoutId="activeTabGlow"
                className="absolute inset-0 bg-cyan-400/5 rounded-xl z-0"
                transition={{ duration: 0.2 }}
            />
        )}
        <Icon size={18} className={`transition-all duration-300 z-10 ${active ? 'scale-110 drop-shadow-[0_0_5px_rgba(6,182,212,0.5)]' : 'group-hover:scale-105'}`} />
        <span className="font-medium text-xs tracking-wide z-10">{label}</span>

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
    onSettingsClick,
    onOverlayClick,
    stats,
    onSpeedLimitChange,
    activeTab,
    onTabChange,
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
        <div className="flex flex-col h-screen bg-[#020617] text-slate-200 font-sans selection:bg-cyan-500/30 overflow-hidden rounded-xl border border-white/5 shadow-2xl aurora-bg">
            <TitleBar />

            <div className="flex flex-1 pt-10 overflow-hidden relative">
                {/* Sidebar */}
                <div className="w-64 flex flex-col z-10 pt-4 bg-slate-900/20 backdrop-blur-md border-r border-white/5">

                    {/* Sidebar Header */}
                    <div className="pt-2 pb-6 px-6">
                        <div className="glass-card rounded-xl p-4 border border-white/5 shadow-inner">
                            <h2 className="text-[10px] uppercase font-bold text-slate-500 mb-2 tracking-widest">Downloaded</h2>
                            <div className="text-2xl font-bold text-white tracking-tight text-glow">
                                {formatBytes(stats.totalBytes)}
                            </div>
                            <div className="text-[10px] text-slate-500 mt-1">
                                {stats.completed} of {stats.total} completed
                            </div>
                            <div className="w-full bg-black/40 h-1.5 rounded-full mt-3 overflow-hidden">
                                <motion.div
                                    className="h-full bg-gradient-to-r from-cyan-500 to-blue-500 relative overflow-hidden shadow-[0_0_10px_rgba(6,182,212,0.5)]"
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
                        />
                        <NavItem
                            icon={Rss}
                            label="Feeds"
                            active={activeTab === 'feeds'}
                            onClick={() => onTabChange('feeds')}
                        />
                        <NavItem
                            icon={Search}
                            label="Discover"
                            active={activeTab === 'search'}
                            onClick={() => onTabChange('search')}
                        />
                        <NavItem
                            icon={Puzzle}
                            label="Plugins"
                            active={activeTab === 'plugins'}
                            onClick={() => onTabChange('plugins')}
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
                    {/* Top Bar */}
                    <header className="h-16 border-b border-white/5 flex items-center justify-between px-6 bg-slate-900/10 backdrop-blur-sm z-20">
                        <div className="flex items-center gap-4">
                            <h1 className="text-lg font-bold text-white tracking-tight drop-shadow-sm">
                                {activeTab === 'downloads' && 'My Downloads'}
                                {activeTab === 'torrents' && 'Torrent Manager'}
                                {activeTab === 'feeds' && 'RSS Feeds'}
                                {activeTab === 'search' && 'Search & Discover'}
                                {activeTab === 'plugins' && 'Plugin Editor'}
                            </h1>
                            {activeTab === 'downloads' && (
                                <span className="bg-white/5 text-slate-300 text-[10px] px-2.5 py-1 rounded-full font-bold border border-white/10 shadow-sm backdrop-blur-md">
                                    {stats.downloading > 0 ? `${stats.downloading} Downloading` : `${stats.total} Total`}
                                </span>
                            )}
                        </div>

                        <div className="flex items-center gap-3">
                            {/* Live Speed Indicator */}
                            {(globalSpeed ?? 0) > 0 && (
                                <div className="flex items-center gap-2 bg-cyan-500/10 border border-cyan-500/20 rounded-lg px-3 py-1.5">
                                    <ArrowDownToLine size={14} className="text-cyan-400 animate-pulse" />
                                    <span className="text-xs font-mono font-bold text-cyan-300">{formatSpeed(globalSpeed ?? 0)}</span>
                                </div>
                            )}
                            <div className="flex items-center bg-black/20 rounded-lg p-1 border border-white/5 hover:border-white/10 transition-colors">
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
                                className="bg-gradient-to-r from-cyan-600 to-blue-600 hover:from-cyan-500 hover:to-blue-500 text-white px-4 py-2 rounded-lg text-xs font-bold transition-all flex items-center gap-2 shadow-[0_0_20px_rgba(6,182,212,0.3)] border border-cyan-400/20"
                            >
                                <Plus size={16} strokeWidth={3} />
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
