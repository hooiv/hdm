import React from 'react';
import { motion } from 'framer-motion';
import { formatBytes, formatSpeed, formatETA } from '../utils/formatters';
import type { DownloadTask } from '../types';
import { Package } from 'lucide-react';

interface DownloadItemInfoProps {
    task: DownloadTask;
    archiveInfo: {
        archive_type: string;
        is_multi_part: boolean;
        part_number: number | null;
    } | null;
    unrarMissing: boolean;
}

export const DownloadItemInfo: React.FC<DownloadItemInfoProps> = ({ task, archiveInfo, unrarMissing }) => {
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed, task.id) : task.status === 'Done' ? 'Complete' : task.status === 'Paused' ? 'Paused' : '';

    return (
        <div className="flex-1 min-w-0">
            <div className="flex items-center mb-1.5 gap-3">
                <div className="font-bold text-slate-100 truncate flex-1 tracking-tight text-base" title={task.filename}>
                    {task.filename}
                </div>
                {task.speed > 0 && (
                    <div className="text-right ml-4">
                        <div className="text-[10px] font-bold text-slate-600 uppercase tracking-widest leading-none mb-0.5">Speed</div>
                        <div className="text-sm font-bold text-cyan-400 font-mono leading-none">
                            {formatSpeed(task.speed).split(' ')[0]}
                            <span className="text-[10px] ml-1 uppercase">{formatSpeed(task.speed).split(' ')[1]}</span>
                        </div>
                    </div>
                )}
                {task.status === 'Downloading' && task.segments && task.segments.length > 0 && (
                    <span className="text-[10px] font-mono text-violet-300 bg-violet-500/10 border border-violet-500/20 px-1.5 py-0.5 rounded" title={`${task.segments.filter(s => s.state === 'Downloading').length} active / ${task.segments.length} total segments`}>
                        ⚡ {task.segments.filter(s => s.state === 'Downloading').length}/{task.segments.length}
                    </span>
                )}
                {archiveInfo && (
                    <span
                        className={`inline-flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded border ${
                            archiveInfo.archive_type === 'Rar'
                                ? 'text-orange-300 bg-orange-500/10 border-orange-500/20'
                                : archiveInfo.archive_type === 'Zip'
                                ? 'text-amber-300 bg-amber-500/10 border-amber-500/20'
                                : archiveInfo.archive_type === 'SevenZip'
                                ? 'text-yellow-300 bg-yellow-500/10 border-yellow-500/20'
                                : 'text-slate-300 bg-slate-500/10 border-slate-500/20'
                        }`}
                        title={`${archiveInfo.archive_type} archive${archiveInfo.is_multi_part ? ` (part ${archiveInfo.part_number ?? '?'})` : ''}${unrarMissing ? ' — unrar not installed' : ''}`}
                    >
                        <Package size={10} />
                        {archiveInfo.archive_type === 'SevenZip' ? '7Z' : archiveInfo.archive_type.toUpperCase()}
                        {archiveInfo.is_multi_part && <span className="text-[8px] opacity-70">P{archiveInfo.part_number ?? '?'}</span>}
                        {unrarMissing && <span className="text-red-400" title="unrar not installed">⚠</span>}
                    </span>
                )}
                {task.integrityStatus === 'verified' && (
                    <span className="text-[10px] font-bold text-emerald-300 bg-emerald-500/10 border border-emerald-500/20 px-1.5 py-0.5 rounded" title="File integrity verified">
                        ✓ Verified
                    </span>
                )}
                {task.integrityStatus === 'failed' && (
                    <span className="text-[10px] font-bold text-red-300 bg-red-500/10 border border-red-500/20 px-1.5 py-0.5 rounded" title="Integrity check failed — file may be corrupted">
                        ✗ Integrity Failed
                    </span>
                )}
                {task.virusScanStatus === 'clean' && (
                    <span className="text-[10px] font-bold text-emerald-300 bg-emerald-500/10 border border-emerald-500/20 px-1.5 py-0.5 rounded" title="Virus scan: clean">
                        🛡 Clean
                    </span>
                )}
                {task.virusScanStatus === 'infected' && (
                    <span className="text-[10px] font-bold text-red-300 bg-red-500/10 border border-red-500/20 px-1.5 py-0.5 rounded animate-pulse" title="Threat detected!">
                        ⚠ Threat
                    </span>
                )}
            </div>

            <div className="text-[10px] text-slate-600 truncate mb-4 font-mono uppercase tracking-tighter opacity-80 mt-1">
                {task.url}
            </div>

            <div className="relative pt-1">
                <div className="progress-track overflow-hidden bg-white/5">
                    <motion.div
                        className={`progress-pulse rounded-full ${
                            task.status === 'Done' ? 'bg-emerald-500' : ''
                        }`}
                        initial={{ width: 0 }}
                        animate={{ width: `${task.progress}%` }}
                        transition={{ type: "spring", stiffness: 50, damping: 20 }}
                    >
                        {task.status === 'Downloading' && <div className="absolute inset-0 animate-shimmer" />}
                    </motion.div>
                </div>
                
                <div className="flex justify-between items-end mt-3">
                    <div className="flex items-center gap-4">
                        <div className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">
                            {task.total > 0 ? (
                                <div className="text-white text-xs font-mono">
                                    {formatBytes(task.downloaded).split(' ')[0]}
                                    <span className="text-[10px] text-slate-500 ml-1">{formatBytes(task.downloaded).split(' ')[1]} / {formatBytes(task.total)}</span>
                                </div>
                            ) : (
                                <div className="text-white text-xs font-mono">{formatBytes(task.downloaded)} <span className="text-slate-500 ml-1">UNKNOWN SIZE</span></div>
                            )}
                        </div>
                        {eta && (
                            <div className="text-[10px] font-bold text-slate-500 uppercase tracking-widest pl-4 border-l border-white/5">
                                ETA <span className={`ml-1 ${task.status === 'Done' ? 'text-emerald-400' : 'text-cyan-400'}`}>{eta}</span>
                            </div>
                        )}
                    </div>
                    <div className="text-[20px] font-black text-white/20 tracking-tighter leading-none italic">
                        {task.total > 0 ? `${Math.round(task.progress)}%` : '--'}
                    </div>
                </div>
            </div>
        </div>
    );
};
