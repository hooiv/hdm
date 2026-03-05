import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { error as logError } from '../utils/logger';
import { formatBytes, formatSpeed, formatETA } from '../utils/formatters';
import { ZipPreviewModal } from './ZipPreviewModal';
import type { DownloadTask } from '../types';
import { motion, AnimatePresence } from 'framer-motion';
import { Folder, Play, Pause, Trash2, ChevronDown, FileText, ArrowUp, ArrowDown, Share2 } from 'lucide-react';
import P2PShareModal from './P2PShareModal';
import { DownloadExpandedPanel } from './DownloadExpandedPanel';

interface DownloadItemProps {
    task: DownloadTask;
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    downloadDir: string;
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
export const DownloadItem = React.memo<DownloadItemProps>(({ task, onPause, onResume, onDelete, onMoveUp, onMoveDown, downloadDir }) => {
    const [showPreview, setShowPreview] = useState(false);
    const [isExpanded, setIsExpanded] = useState(false);
    const [showP2PShare, setShowP2PShare] = useState(false);

    // Sanitize filename: strip any path separators to prevent path traversal in display/path construction
    const safeFilename = React.useMemo(() => {
        const name = task.filename || 'unknown';
        return name.replace(/[\\/]/g, '_');
    }, [task.filename]);

    // Compute full file path from settings-based download directory
    const filePath = `${downloadDir}/${safeFilename}`;

    // Derived values
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed) : task.status === 'Done' ? 'Complete' : task.status === 'Paused' ? 'Paused' : '';

    // Memoize category calculation
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
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.2 }}
            className={`relative overflow-hidden mb-3 rounded-xl border transition-all duration-300 ${task.status === 'Downloading'
                ? 'bg-slate-900/60 backdrop-blur-md border-cyan-500/20 shadow-[0_0_20px_rgba(6,182,212,0.05)]'
                : 'glass-card'
                }`}
            onClick={() => setIsExpanded(!isExpanded)}
        >
            <div className="flex items-center p-4 cursor-pointer">
                {/* Icon */}
                <div className={`mr-4 p-3 rounded-xl text-2xl ${category.bgColor} ${category.color} border border-white/5 shadow-inner backdrop-blur-sm`}>
                    <motion.div
                        whileHover={{ rotate: [0, -10, 10, 0], scale: 1.1 }}
                        transition={{ duration: 0.5 }}
                    >
                        <FileText size={22} className={category.color} strokeWidth={1.5} />
                    </motion.div>
                </div>

                {/* Info */}
                <div className="flex-1 min-w-0">
                    <div className="flex items-center mb-1.5 gap-3">
                        <div className="font-semibold text-slate-100 truncate flex-1 tracking-tight text-sm text-glow" title={task.filename}>
                            {task.filename}
                        </div>
                        <span className={`text-[9px] uppercase font-bold px-2 py-0.5 rounded-full border border-white/5 ${category.bgColor} ${category.color}`}>
                            {category.label}
                        </span>
                        {task.speed > 0 && (
                            <span className="text-[10px] font-mono text-cyan-300 bg-cyan-500/10 border border-cyan-500/20 px-2 py-0.5 rounded shadow-[0_0_10px_rgba(6,182,212,0.1)]">
                                {formatSpeed(task.speed)}
                            </span>
                        )}
                    </div>

                    <div className="text-[11px] text-slate-500 truncate mb-3 font-mono opacity-60">
                        {task.url}
                    </div>

                    <div className="flex items-center gap-3">
                        <div className="flex-1 h-1.5 bg-black/40 rounded-full overflow-hidden border border-white/5">
                            <motion.div
                                className={`h-full rounded-full relative overflow-hidden ${task.status === 'Error' ? 'bg-red-500' : 'bg-gradient-to-r from-cyan-500 to-blue-600'}`}
                                initial={{ width: 0 }}
                                animate={{ width: `${task.progress}%` }}
                                transition={{ type: "spring", stiffness: 100, damping: 20 }}
                            >
                                {task.status === 'Downloading' && <div className="absolute inset-0 animate-shimmer" />}
                            </motion.div>
                        </div>
                        <div className="text-[10px] font-bold text-slate-400 w-10 text-right">
                            {task.total > 0 ? `${task.progress.toFixed(1)}%` : '—'}
                        </div>
                    </div>

                    <div className="flex justify-between mt-1 text-[10px] text-slate-500 font-medium tracking-wide">
                        <span>{task.total > 0 ? <>{formatBytes(task.downloaded)} <span className="text-slate-600">/</span> {formatBytes(task.total)}</> : <>{formatBytes(task.downloaded)} <span className="text-slate-600">(unknown size)</span></>}</span>
                        {eta && <span className={task.status === 'Done' ? 'text-emerald-500/80' : 'text-cyan-600/70'}>{task.status === 'Done' ? '' : 'ETA: '}{eta}</span>}
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
