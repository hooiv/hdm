import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { error as logError } from '../utils/logger';
import { ZipPreviewModal } from './ZipPreviewModal';
import type { DiscoveredMirror, DownloadTask } from '../types';
import { motion, AnimatePresence } from 'framer-motion';
import P2PShareModal from './P2PShareModal';
import { DownloadExpandedPanel } from './DownloadExpandedPanel';
import { DownloadItemIcon } from './DownloadItemIcon';
import { DownloadItemInfo } from './DownloadItemInfo';
import { DownloadItemActions } from './DownloadItemActions';

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

    const category = React.useMemo(() => getFileCategory(safeFilename), [safeFilename]);

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
                } ${isSpotlighted ? 'ring-1 ring-cyan-400/40 border-cyan-400/20 shadow-[0_0_40px_rgba(0,242,255,0.1)]' : ''} ${task.status === 'Done' ? 'opacity-60 hover:opacity-100' : ''}`}
            onClick={() => setIsExpanded(!isExpanded)}
            data-testid={`download-item-${task.id}`}
            data-spotlighted={isSpotlighted ? 'true' : 'false'}
        >
            <div className="flex items-center p-5 cursor-pointer">
                <DownloadItemIcon category={category} />
                <DownloadItemInfo task={task} archiveInfo={archiveInfo} unrarMissing={unrarMissing} />
                <DownloadItemActions
                    task={task}
                    onPause={onPause}
                    onResume={onResume}
                    onDelete={onDelete}
                    onMoveUp={onMoveUp}
                    onMoveDown={onMoveDown}
                    isExpanded={isExpanded}
                    onToggleExpand={() => setIsExpanded(!isExpanded)}
                    onShowP2PShare={() => setShowP2PShare(true)}
                    onOpenFolder={handleOpenFolder}
                />
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

