import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { error as logError } from '../utils/logger';
import { formatBytes, formatSpeed, formatETA } from '../utils/formatters';
import { ZipPreviewModal } from './ZipPreviewModal';
import type { DiscoveredMirror, DownloadTask } from '../types';
import { motion, AnimatePresence } from 'framer-motion';
import { Folder, Play, Pause, Trash2, ChevronDown, FileText, ArrowUp, ArrowDown, Share2, Package } from 'lucide-react';
import P2PShareModal from './P2PShareModal';
import { DownloadExpandedPanel } from './DownloadExpandedPanel';

interface DownloadItemProps {
    task: DownloadTask;
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDiscoveredMirrors?: (id: string, mirrors: DiscoveredMirror[]) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    downloadDir: string;
    isSpotlighted?: boolean;
}

// File type categories
const getFileCategory = (filename: string): { icon: string; label: string; color: string; bgColor: string } => {
    const ext = filename.split('.').pop()?.toLowerCase() || '';

    // Map colors to Tailwind classes would be ideal, but for dynamic colors we might keep hex or map to tailwind palette
    // For now, let's stick to hex for specific category colors but use Tailwind for structure.

    // We can map these to Tailwind color families
    const categories: Record<string, { icon: string; label: string; color: string; bgColor: string }> = {
        // Video
        mp4: { icon: '🎬', label: 'Video', color: 'text-red-500', bgColor: 'bg-red-500/10' },
        mkv: { icon: '🎬', label: 'Video', color: 'text-red-500', bgColor: 'bg-red-500/10' },
        avi: { icon: '🎬', label: 'Video', color: 'text-red-500', bgColor: 'bg-red-500/10' },
        mov: { icon: '🎬', label: 'Video', color: 'text-red-500', bgColor: 'bg-red-500/10' },
        webm: { icon: '🎬', label: 'Video', color: 'text-red-500', bgColor: 'bg-red-500/10' },
        // Audio
        mp3: { icon: '🎵', label: 'Audio', color: 'text-violet-500', bgColor: 'bg-violet-500/10' },
        flac: { icon: '🎵', label: 'Audio', color: 'text-violet-500', bgColor: 'bg-violet-500/10' },
        wav: { icon: '🎵', label: 'Audio', color: 'text-violet-500', bgColor: 'bg-violet-500/10' },
        aac: { icon: '🎵', label: 'Audio', color: 'text-violet-500', bgColor: 'bg-violet-500/10' },
        // Archives
        zip: { icon: '📦', label: 'Archive', color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
        rar: { icon: '📦', label: 'Archive', color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
        '7z': { icon: '📦', label: 'Archive', color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
        tar: { icon: '📦', label: 'Archive', color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
        gz: { icon: '📦', label: 'Archive', color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
        // Programs
        exe: { icon: '⚙️', label: 'Program', color: 'text-green-500', bgColor: 'bg-green-500/10' },
        msi: { icon: '⚙️', label: 'Program', color: 'text-green-500', bgColor: 'bg-green-500/10' },
        dmg: { icon: '⚙️', label: 'Program', color: 'text-green-500', bgColor: 'bg-green-500/10' },
        // Documents
        pdf: { icon: '📄', label: 'Document', color: 'text-blue-500', bgColor: 'bg-blue-500/10' },
        doc: { icon: '📄', label: 'Document', color: 'text-blue-500', bgColor: 'bg-blue-500/10' },
        docx: { icon: '📄', label: 'Document', color: 'text-blue-500', bgColor: 'bg-blue-500/10' },
        // Images
        jpg: { icon: '🖼️', label: 'Image', color: 'text-pink-500', bgColor: 'bg-pink-500/10' },
        jpeg: { icon: '🖼️', label: 'Image', color: 'text-pink-500', bgColor: 'bg-pink-500/10' },
        png: { icon: '🖼️', label: 'Image', color: 'text-pink-500', bgColor: 'bg-pink-500/10' },
        gif: { icon: '🖼️', label: 'Image', color: 'text-pink-500', bgColor: 'bg-pink-500/10' },
        // ISO
        iso: { icon: '💿', label: 'Disk Image', color: 'text-teal-500', bgColor: 'bg-teal-500/10' },
    };

    return categories[ext] || { icon: '📄', label: 'File', color: 'text-slate-400', bgColor: 'bg-slate-800' };
};

// Memoize Item to prevent re-renders of non-updating downloads
export const DownloadItem = React.memo<DownloadItemProps>(({ task, onPause, onResume, onDiscoveredMirrors, onDelete, onMoveUp, onMoveDown, downloadDir, isSpotlighted = false }) => {
    const [showPreview, setShowPreview] = useState(false);
    const [isExpanded, setIsExpanded] = useState(false);
    const [showP2PShare, setShowP2PShare] = useState(false);
    const [archiveInfo, setArchiveInfo] = useState<{ archive_type: string; is_multi_part: boolean; part_number: number | null } | null>(null);
    const [unrarMissing, setUnrarMissing] = useState(false);

    // Detect archive type for completed downloads
    useEffect(() => {
        if (task.status !== 'Done' || !downloadDir) return;
        let cancelled = false;
        const detectPath = `${downloadDir}/${(task.filename || 'unknown').replace(/[\\/]/g, '_')}`;
        invoke<{ archive_type: string; is_multi_part: boolean; part_number: number | null } | null>('detect_archive', { path: detectPath })
            .then(info => {
                if (cancelled) return;
                setArchiveInfo(info ?? null);
                if (info && info.archive_type === 'Rar') {
                    invoke<boolean>('check_unrar_available').then(ok => {
                        if (!cancelled && !ok) setUnrarMissing(true);
                    }).catch(() => {});
                }
            })
            .catch(() => {});
        return () => { cancelled = true; };
    }, [task.status, task.filename, downloadDir]);

    // Sanitize filename: strip any path separators to prevent path traversal in display/path construction
    const safeFilename = React.useMemo(() => {
        const name = task.filename || 'unknown';
        return name.replace(/[\\/]/g, '_');
    }, [task.filename]);

    // Compute full file path from settings-based download directory
    const filePath = `${downloadDir}/${safeFilename}`;

    // Derived values
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed, task.id) : task.status === 'Done' ? 'Complete' : task.status === 'Paused' ? 'Paused' : '';

    const category = React.useMemo(() => getFileCategory(safeFilename), [safeFilename]);

    // Helper to check if mountable
    // --- derived state for expanded panel passed via props or contained inside it ---

    const handleOpenFolder = React.useCallback(async () => {
        try {
            await invoke('open_folder', { path: filePath });
        } catch (err) {
            logError('Failed to open folder:', filePath, err);
        }
    }, [filePath]);

    return (
        <motion.div
            layout
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0, scale: isSpotlighted ? 1.01 : 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.2 }}
            className={`relative overflow-hidden mb-4 rounded-2xl transition-all duration-500 ${task.status === 'Downloading'
                ? 'bg-white/[0.04] backdrop-blur-xl border border-cyan-500/10 shadow-[0_20px_40px_rgba(0,0,0,0.3)]'
                : 'glass-card'
                } ${isSpotlighted ? 'ring-1 ring-cyan-400/40 border-cyan-400/20 shadow-[0_0_40px_rgba(0,242,255,0.1)]' : ''}`}
            onClick={() => setIsExpanded(!isExpanded)}
            data-testid={`download-item-${task.id}`}
            data-spotlighted={isSpotlighted ? 'true' : 'false'}
        >
            <div className="flex items-center p-5 cursor-pointer">
                {/* Icon */}
                <div className={`mr-5 p-3.5 rounded-2xl text-2xl ${category.bgColor} ${category.color} border border-white/5`}>
                    <motion.div
                        whileHover={{ rotate: [0, -10, 10, 0], scale: 1.15 }}
                        transition={{ duration: 0.5 }}
                    >
                        <FileText size={24} className={category.color} strokeWidth={1.5} />
                    </motion.div>
                </div>

                {/* Info */}
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
                                className="progress-pulse rounded-full"
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

                {/* Quick Actions */}
                <div className="ml-4 flex items-center gap-1 bg-black/20 p-1 rounded-lg border border-white/5" onClick={(e) => e.stopPropagation()}>
                    {onMoveUp && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-slate-200 rounded-md transition-colors" onClick={() => onMoveUp(task.id)} title="Move Up">
                            <ArrowUp size={14} />
                        </motion.button>
                    )}
                    {onMoveDown && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-slate-200 rounded-md transition-colors" onClick={() => onMoveDown(task.id)} title="Move Down">
                            <ArrowDown size={14} />
                        </motion.button>
                    )}

                    <div className="w-px h-4 bg-white/10 mx-1"></div>

                    <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-400 hover:text-cyan-400 rounded-md transition-colors" onClick={handleOpenFolder} title="Open Folder" aria-label="Open folder">
                        <Folder size={16} />
                    </motion.button>

                    {task.status === 'Downloading' && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-amber-400 hover:text-amber-300 rounded-md transition-colors" onClick={() => onPause(task.id)} title="Pause" aria-label="Pause download">
                            <Pause size={16} />
                        </motion.button>
                    )}

                    {(task.status === 'Paused' || task.status === 'Error') && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-emerald-400 hover:text-emerald-300 rounded-md transition-colors" onClick={() => onResume(task.id)} title="Resume" aria-label="Resume download">
                            <Play size={16} />
                        </motion.button>
                    )}

                    <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(220,38,38,0.2)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-red-400 rounded-md transition-colors" onClick={() => onDelete && window.confirm(`Delete "${task.filename}"?`) && onDelete(task.id)} title="Cancel" aria-label="Delete download">
                        <Trash2 size={16} />
                    </motion.button>

                    {/* P2P Share Button */}
                    {(task.status === 'Done' || task.status === 'Downloading') && (
                        <motion.button
                            whileHover={{ scale: 1.1, backgroundColor: "rgba(6,182,212,0.2)" }}
                            whileTap={{ scale: 0.9 }}
                            className="p-1.5 text-slate-500 hover:text-cyan-400 rounded-md transition-colors"
                            onClick={() => setShowP2PShare(true)}
                            title="Share via P2P"
                            aria-label="Share via P2P"
                        >
                            <Share2 size={16} />
                        </motion.button>
                    )}

                    <div className="w-px h-4 bg-white/10 mx-1"></div>

                    <motion.div
                        animate={{ rotate: isExpanded ? 180 : 0 }}
                        className="p-1 text-slate-500"
                    >
                        <ChevronDown size={16} />
                    </motion.div>
                </div>
            </div>

            {/* Expandable Area */}
            <AnimatePresence>
                {isExpanded && (
                    <motion.div
                        initial={{ height: 0, opacity: 0 }}
                        animate={{ height: 'auto', opacity: 1 }}
                        exit={{ height: 0, opacity: 0 }}
                        transition={{ duration: 0.3 }}
                        className="border-t border-slate-700/30 bg-slate-900/30"
                        onClick={(e) => e.stopPropagation()}
                    >
                        <DownloadExpandedPanel
                            task={task}
                            filePath={filePath}
                            onResume={onResume}
                            onDiscoveredMirrors={onDiscoveredMirrors}
                            onShowPreview={() => setShowPreview(true)}
                            onShowP2PShare={() => setShowP2PShare(true)}
                        />
                    </motion.div>
                )}
            </AnimatePresence>

            {/* Modals */}
            {showPreview && (
                <ZipPreviewModal
                    isOpen={showPreview}
                    filePath={filePath}
                    url={task.url}
                    onClose={() => setShowPreview(false)}
                    isPartial={task.status === 'Downloading' || task.status === 'Paused'}
                />
            )}

            {showP2PShare && (
                <P2PShareModal
                    isOpen={showP2PShare}
                    onClose={() => setShowP2PShare(false)}
                    downloadId={task.id}
                    downloadName={task.filename}
                />
            )}
        </motion.div>
    );
});
