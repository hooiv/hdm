import React, { useState, useEffect, useRef } from 'react';
import { error as logError } from '../utils/logger';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { Calendar, Clock, Play, Trash2, Plus, X, Link as LinkIcon } from 'lucide-react';

interface ScheduleModalProps {
    isOpen: boolean;
    onClose: () => void;
}

interface ScheduledDownload {
    id: string;
    url: string;
    filename: string;
    scheduled_time: string;
    status: string;
}

interface TimeInfo {
    hour: number;
    minute: number;
    second: number;
    is_quiet_hours: boolean;
}

const generateId = () => `sched_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

export const ScheduleModal: React.FC<ScheduleModalProps> = ({ isOpen, onClose }) => {
    const [activeTab, setActiveTab] = useState<'list' | 'add'>('list');
    const [downloads, setDownloads] = useState<ScheduledDownload[]>([]);
    const [timeInfo, setTimeInfo] = useState<TimeInfo | null>(null);

    // Form State
    const [url, setUrl] = useState('');
    const [filename, setFilename] = useState('');
    const userEditedFilename = useRef(false);
    const [date, setDate] = useState('');
    const [time, setTime] = useState('');
    const [error, setError] = useState('');

    useEffect(() => {
        if (isOpen) {
            refreshData();
            const interval = setInterval(() => {
                if (!document.hidden) refreshData();
            }, 10000);
            return () => clearInterval(interval);
        }
    }, [isOpen]);

    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') { e.preventDefault(); onClose(); }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, onClose]);

    const refreshData = async () => {
        try {
            const list = await invoke<ScheduledDownload[]>('get_scheduled_downloads');
            const info = await invoke<TimeInfo>('get_time_info');
            setDownloads(list);
            setTimeInfo(info);
        } catch (err) {
            logError('Failed to fetch schedule data:', err);
        }
    };

    const handleUrlChange = (newUrl: string) => {
        setUrl(newUrl);
        // Auto-fill filename only if user hasn't manually edited it
        if (!userEditedFilename.current && newUrl) {
            const extracted = newUrl.split('/').pop()?.split('?')[0] || '';
            setFilename(extracted);
        }
    };

    const handleSchedule = async () => {
        if (!url || !date || !time) {
            setError('Please fill in all fields');
            return;
        }

        try {
            const scheduledDate = new Date(`${date}T${time}`);
            if (isNaN(scheduledDate.getTime())) {
                setError('Invalid date/time format');
                return;
            }
            if (scheduledDate.getTime() <= Date.now()) {
                setError('Scheduled time must be in the future');
                return;
            }
            const scheduledTime = scheduledDate.toISOString();
            const id = generateId();

            await invoke('schedule_download', {
                id,
                url,
                filename: filename || 'download',
                scheduledTime,
            });

            setUrl('');
            setFilename('');
            userEditedFilename.current = false;
            setDate('');
            setTime('');
            setError('');
            setActiveTab('list');
            refreshData();
        } catch (err) {
            setError('Failed to schedule download');
            logError(err);
        }
    };

    const handleForceStart = async (id: string) => {
        try {
            await invoke('force_start_scheduled_download', { id });
            await refreshData();
        } catch (err) {
            logError('Failed to force start:', err);
        }
    };

    const handleRemove = async (id: string) => {
        try {
            await invoke('remove_scheduled_download', { id });
            await refreshData();
        } catch (err) {
            logError('Failed to remove schedule:', err);
        }
    };

    const today = new Date().toISOString().split('T')[0];
    const pendingDownloads = downloads.filter(d => d.status === 'pending');

    return (
        <AnimatePresence>
            {isOpen && (
            <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
                onClick={onClose}
                role="dialog"
                aria-modal="true"
            >
                <motion.div
                    initial={{ opacity: 0, scale: 0.95, y: 20 }}
                    animate={{ opacity: 1, scale: 1, y: 0 }}
                    exit={{ opacity: 0, scale: 0.95, y: 20 }}
                    transition={{ type: 'spring', stiffness: 300, damping: 25 }}
                    className="bg-slate-900/95 backdrop-blur-xl border border-white/10 rounded-2xl shadow-2xl w-full max-w-lg mx-4 overflow-hidden"
                    onClick={e => e.stopPropagation()}
                >
                    {/* Header */}
                    <div className="flex items-center justify-between px-6 py-4 border-b border-white/5">
                        <div className="flex items-center gap-3">
                            <div className="w-8 h-8 rounded-lg bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center">
                                <Calendar size={16} className="text-cyan-400" />
                            </div>
                            <h3 className="text-lg font-semibold text-white">Scheduler</h3>
                        </div>
                        <button
                            onClick={onClose}
                            className="p-1.5 text-slate-500 hover:text-white hover:bg-white/10 rounded-lg transition-colors"
                        >
                            <X size={16} />
                        </button>
                    </div>

                    {/* Tabs */}
                    <div className="flex border-b border-white/5">
                        <button
                            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors relative ${
                                activeTab === 'list' ? 'text-cyan-400' : 'text-slate-500 hover:text-slate-300'
                            }`}
                            onClick={() => setActiveTab('list')}
                        >
                            Upcoming ({pendingDownloads.length})
                            {activeTab === 'list' && (
                                <motion.div layoutId="scheduleTab" className="absolute bottom-0 left-0 right-0 h-0.5 bg-cyan-500" />
                            )}
                        </button>
                        <button
                            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors relative ${
                                activeTab === 'add' ? 'text-cyan-400' : 'text-slate-500 hover:text-slate-300'
                            }`}
                            onClick={() => setActiveTab('add')}
                        >
                            <span className="flex items-center justify-center gap-1.5"><Plus size={14} /> Add New</span>
                            {activeTab === 'add' && (
                                <motion.div layoutId="scheduleTab" className="absolute bottom-0 left-0 right-0 h-0.5 bg-cyan-500" />
                            )}
                        </button>
                    </div>

                    {/* Time Info Banner */}
                    {timeInfo && (
                        <div className="px-6 py-2.5 bg-slate-800/50 border-b border-white/5 flex items-center justify-between text-xs">
                            <span className="text-slate-400 flex items-center gap-1.5">
                                <Clock size={12} className="text-slate-500" />
                                {timeInfo.hour.toString().padStart(2, '0')}:{timeInfo.minute.toString().padStart(2, '0')}
                            </span>
                            {timeInfo.is_quiet_hours ? (
                                <span className="text-violet-400 bg-violet-500/10 px-2 py-0.5 rounded-full border border-violet-500/20 text-[10px] font-medium">
                                    Quiet Hours Active
                                </span>
                            ) : (
                                <span className="text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-full border border-emerald-500/20 text-[10px] font-medium">
                                    Active Period
                                </span>
                            )}
                        </div>
                    )}

                    {/* Body */}
                    <div className="p-6 max-h-[60vh] overflow-y-auto custom-scrollbar">
                        {activeTab === 'list' ? (
                            <div className="space-y-2">
                                {pendingDownloads.length === 0 ? (
                                    <div className="text-center py-12 text-slate-500">
                                        <Calendar size={32} className="mx-auto mb-3 opacity-30" />
                                        <p className="text-sm">No pending scheduled downloads.</p>
                                    </div>
                                ) : (
                                    pendingDownloads.map(item => (
                                        <motion.div
                                            key={item.id}
                                            layout
                                            initial={{ opacity: 0, y: 5 }}
                                            animate={{ opacity: 1, y: 0 }}
                                            className="flex items-center gap-3 bg-white/5 border border-white/5 rounded-xl p-3 hover:bg-white/10 transition-colors"
                                        >
                                            <div className="flex-1 min-w-0">
                                                <div className="text-sm font-medium text-slate-200 truncate" title={item.filename}>
                                                    {item.filename}
                                                </div>
                                                <div className="text-[11px] text-slate-500 mt-0.5 flex items-center gap-1">
                                                    <Clock size={10} />
                                                    {new Date(item.scheduled_time).toLocaleString()}
                                                </div>
                                            </div>
                                            <div className="flex items-center gap-1 flex-shrink-0">
                                                <motion.button
                                                    whileHover={{ scale: 1.1 }}
                                                    whileTap={{ scale: 0.9 }}
                                                    className="p-1.5 text-emerald-400 hover:bg-emerald-500/10 rounded-lg transition-colors"
                                                    onClick={() => handleForceStart(item.id)}
                                                    title="Start Now"
                                                >
                                                    <Play size={14} />
                                                </motion.button>
                                                <motion.button
                                                    whileHover={{ scale: 1.1 }}
                                                    whileTap={{ scale: 0.9 }}
                                                    className="p-1.5 text-red-400 hover:bg-red-500/10 rounded-lg transition-colors"
                                                    onClick={() => handleRemove(item.id)}
                                                    title="Remove"
                                                >
                                                    <Trash2 size={14} />
                                                </motion.button>
                                            </div>
                                        </motion.div>
                                    ))
                                )}
                            </div>
                        ) : (
                            <div className="space-y-4">
                                {error && (
                                    <div className="text-sm text-red-400 bg-red-500/10 border border-red-500/20 rounded-lg px-3 py-2">
                                        {error}
                                    </div>
                                )}
                                <div>
                                    <label className="block text-xs font-medium text-slate-400 mb-1.5">URL</label>
                                    <div className="relative">
                                        <LinkIcon size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
                                        <input
                                            type="url"
                                            placeholder="https://example.com/file.zip"
                                            value={url}
                                            onChange={(e) => handleUrlChange(e.target.value)}
                                            className="w-full bg-black/30 border border-white/10 rounded-lg pl-9 pr-3 py-2.5 text-sm text-white placeholder-slate-600 focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 transition-colors"
                                        />
                                    </div>
                                </div>
                                <div>
                                    <label className="block text-xs font-medium text-slate-400 mb-1.5">Filename</label>
                                    <input
                                        type="text"
                                        placeholder="file.zip"
                                        value={filename}
                                        onChange={(e) => { userEditedFilename.current = true; setFilename(e.target.value); }}
                                        className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white placeholder-slate-600 focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 transition-colors"
                                    />
                                </div>
                                <div className="grid grid-cols-2 gap-3">
                                    <div>
                                        <label className="block text-xs font-medium text-slate-400 mb-1.5">Date</label>
                                        <input
                                            type="date"
                                            min={today}
                                            value={date}
                                            onChange={(e) => setDate(e.target.value)}
                                            className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 transition-colors [color-scheme:dark]"
                                        />
                                    </div>
                                    <div>
                                        <label className="block text-xs font-medium text-slate-400 mb-1.5">Time</label>
                                        <input
                                            type="time"
                                            value={time}
                                            onChange={(e) => setTime(e.target.value)}
                                            className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 transition-colors [color-scheme:dark]"
                                        />
                                    </div>
                                </div>
                                <motion.button
                                    whileHover={{ scale: 1.01 }}
                                    whileTap={{ scale: 0.99 }}
                                    onClick={handleSchedule}
                                    className="w-full py-2.5 bg-gradient-to-r from-cyan-500 to-blue-600 hover:from-cyan-400 hover:to-blue-500 text-white text-sm font-semibold rounded-lg transition-all shadow-[0_0_15px_rgba(6,182,212,0.3)]"
                                >
                                    Schedule Download
                                </motion.button>
                            </div>
                        )}
                    </div>
                </motion.div>
            </motion.div>
            )}
        </AnimatePresence>
    );
};
