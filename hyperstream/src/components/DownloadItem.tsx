import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ZipPreviewModal } from './ZipPreviewModal';
import { Segment } from '../types';
import { ThreadVisualizer } from './ThreadVisualizer';
import { motion, AnimatePresence } from 'framer-motion';
import { Folder, Play, Pause, Trash2, FileText, ChevronDown, Archive, ArrowUp, ArrowDown, HardDrive, Cloud, Film, Music, Share2, Shield, Link, Globe, RefreshCw } from 'lucide-react';
import P2PShareModal from './P2PShareModal';

export interface DownloadTask {
    id: string;
    filename: string;
    url: string;
    progress: number; // 0-100
    downloaded: number; // bytes
    total: number; // bytes
    speed: number; // bytes/sec
    status: 'Downloading' | 'Paused' | 'Error' | 'Done';
    segments?: Segment[];
}

interface DownloadItemProps {
    task: DownloadTask;
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
}

const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

const formatSpeed = (bytesPerSec: number) => {
    return formatBytes(bytesPerSec) + '/s';
};

const formatETA = (remainingBytes: number, speed: number) => {
    if (speed <= 0) return '--:--';
    const seconds = Math.floor(remainingBytes / speed);
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) {
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins}m ${secs}s`;
    }
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${mins}m`;
};

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
export const DownloadItem = React.memo<DownloadItemProps>(({ task, onPause, onResume, onDelete, onMoveUp, onMoveDown }) => {
    const [showPreview, setShowPreview] = useState(false);
    const [isExpanded, setIsExpanded] = useState(false);
    const [showP2PShare, setShowP2PShare] = useState(false);
    const [shareUrl, setShareUrl] = useState<string | null>(null);
    const [scrubbing, setScrubbing] = useState(false);
    const [checkingWayback, setCheckingWayback] = useState(false);

    // Derived values
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed) : '--:--';

    // Memoize category calculation
    const category = React.useMemo(() => getFileCategory(task.filename), [task.filename]);

    // Helper to check if mountable
    const isMountable = ['zip', 'iso'].includes(task.filename.split('.').pop()?.toLowerCase() || '');

    const handleOpenFolder = React.useCallback(async () => {
        await invoke('open_folder', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}` });
    }, [task.filename]);

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
                            {task.progress.toFixed(1)}%
                        </div>
                    </div>

                    <div className="flex justify-between mt-1 text-[10px] text-slate-500 font-medium tracking-wide">
                        <span>{formatBytes(task.downloaded)} <span className="text-slate-600">/</span> {formatBytes(task.total)}</span>
                        <span className="text-cyan-600/70">ETA: {eta}</span>
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

                    <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-400 hover:text-cyan-400 rounded-md transition-colors" onClick={handleOpenFolder} title="Open Folder">
                        <Folder size={16} />
                    </motion.button>

                    {task.status === 'Downloading' && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-amber-400 hover:text-amber-300 rounded-md transition-colors" onClick={() => onPause(task.id)} title="Pause">
                            <Pause size={16} />
                        </motion.button>
                    )}

                    {(task.status === 'Paused' || task.status === 'Error') && (
                        <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-emerald-400 hover:text-emerald-300 rounded-md transition-colors" onClick={() => onResume(task.id)} title="Resume">
                            <Play size={16} />
                        </motion.button>
                    )}

                    <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(220,38,38,0.2)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-red-400 rounded-md transition-colors" onClick={() => onDelete && onDelete(task.id)} title="Cancel">
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
                        <div className="p-4">
                            {/* Thread Visualization */}
                            <ThreadVisualizer
                                segments={task.segments || []}
                                totalSize={task.total}
                            />

                            {/* Advanced Actions Toolbar */}
                            {task.status === 'Done' && (
                                <div className="mt-4 pt-3 border-t border-slate-700/30 flex flex-wrap gap-2">

                                    {/* Archive Preview */}
                                    {(task.filename.endsWith('.zip') || task.filename.endsWith('.jar')) && (
                                        <button
                                            onClick={(e) => { e.stopPropagation(); setShowPreview(true); }}
                                            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-blue-500/10 text-blue-400 border border-blue-500/20 hover:bg-blue-500/20"
                                        >
                                            <Archive size={14} /> Browse Content
                                        </button>
                                    )}

                                    {/* Mount Drive */}
                                    {isMountable && (
                                        <button
                                            onClick={async (e) => {
                                                e.stopPropagation();
                                                try {
                                                    const port = await invoke('mount_drive', { id: task.id, path: task.filename });
                                                    alert(`Mounted on WebDAV Port: ${port}.\n\nUse 'Map Network Drive' to http://127.0.0.1:${port}`);
                                                } catch (err) {
                                                    alert("Mount failed: " + err);
                                                }
                                            }}
                                            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-green-500/10 text-green-400 border border-green-500/20 hover:bg-green-500/20"
                                        >
                                            <HardDrive size={14} /> Mount Drive
                                        </button>
                                    )}

                                    {/* Cloud Upload */}
                                    <button
                                        onClick={async (e) => {
                                            e.stopPropagation();
                                            if (!confirm("Upload to configured Cloud Storage?")) return;
                                            try {
                                                alert("Upload started... please wait.");
                                                const result = await invoke('upload_to_cloud', { path: task.filename, targetName: null });
                                                alert("Success: " + result);
                                            } catch (err) {
                                                alert("Upload failed: " + err);
                                            }
                                        }}
                                        className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20"
                                    >
                                        <Cloud size={14} /> Upload to Cloud
                                    </button>

                                    {/* Media Tools */}
                                    {(['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(task.filename.split('.').pop()?.toLowerCase() || '')) && (
                                        <>
                                            <button
                                                onClick={async (e) => {
                                                    e.stopPropagation();
                                                    try {
                                                        alert("Generating Preview (WebP)...");
                                                        await invoke('process_media', { path: task.filename, action: 'preview' });
                                                        alert("Preview Generated!");
                                                    } catch (err) {
                                                        alert("Media Process Failed: " + err);
                                                    }
                                                }}
                                                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20"
                                            >
                                                <Film size={14} /> Smart Preview
                                            </button>

                                            <button
                                                onClick={async (e) => {
                                                    e.stopPropagation();
                                                    try {
                                                        alert("Extracting Audio (MP3)...");
                                                        await invoke('process_media', { path: task.filename, action: 'audio' });
                                                        alert("Audio Extracted!");
                                                    } catch (err) {
                                                        alert("Media Process Failed: " + err);
                                                    }
                                                }}
                                                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 hover:bg-indigo-500/20"
                                            >
                                                <Music size={14} /> Extract Audio
                                            </button>
                                        </>
                                    )}

                                    {/* Scrub Metadata - for images & PDFs */}
                                    {['jpg', 'jpeg', 'png', 'pdf'].includes(task.filename.split('.').pop()?.toLowerCase() || '') && (
                                        <button
                                            disabled={scrubbing}
                                            onClick={async (e) => {
                                                e.stopPropagation();
                                                setScrubbing(true);
                                                try {
                                                    const result: any = await invoke('scrub_metadata', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}` });
                                                    alert(`✅ Metadata scrubbed!\nRemoved: ${result.fields_removed.length} fields (${result.bytes_removed} bytes)`);
                                                } catch (err) {
                                                    alert('Scrub failed: ' + err);
                                                } finally {
                                                    setScrubbing(false);
                                                }
                                            }}
                                            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50"
                                        >
                                            <Shield size={14} /> {scrubbing ? 'Scrubbing...' : 'Scrub Metadata'}
                                        </button>
                                    )}

                                    {/* Share via Link (Ephemeral Server) */}
                                    <button
                                        onClick={async (e) => {
                                            e.stopPropagation();
                                            try {
                                                const result: any = await invoke('start_ephemeral_share', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}`, timeoutMins: 60 });
                                                setShareUrl(result.url);
                                                navigator.clipboard?.writeText(result.url);
                                                alert(`🔗 Share link copied!\n${result.url}\n\nExpires in 1 hour.`);
                                            } catch (err) {
                                                alert('Share failed: ' + err);
                                            }
                                        }}
                                        className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20"
                                    >
                                        <Link size={14} /> Share via Link
                                    </button>

                                    {shareUrl && (
                                        <div className="w-full mt-2 p-2 bg-cyan-500/5 border border-cyan-500/20 rounded-md text-xs text-cyan-400 font-mono break-all">
                                            🔗 {shareUrl}
                                        </div>
                                    )}
                                </div>
                            )}

                            {/* Actions for Error/Paused downloads */}
                            {(task.status === 'Error' || task.status === 'Paused') && (
                                <div className="mt-4 pt-3 border-t border-slate-700/30 flex flex-wrap gap-2">
                                    <button
                                        onClick={async (e) => {
                                            e.stopPropagation();
                                            const newUrl = prompt("Enter the new URL to refresh this download:");
                                            if (newUrl && newUrl.trim() !== "") {
                                                try {
                                                    await invoke('refresh_download_url', { id: task.id, newUrl: newUrl.trim() });
                                                    alert('✅ Download URL refreshed successfully. Click Resume to retry.');
                                                } catch (err) {
                                                    alert('Refresh failed: ' + err);
                                                }
                                            }
                                        }}
                                        className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20"
                                    >
                                        <RefreshCw size={14} /> Refresh Address
                                    </button>

                                    {task.status === 'Error' && (
                                        <button
                                            disabled={checkingWayback}
                                            onClick={async (e) => {
                                                e.stopPropagation();
                                                setCheckingWayback(true);
                                                try {
                                                    const snapshot: any = await invoke('check_wayback_availability', { url: task.url });
                                                    if (snapshot) {
                                                        const downloadUrl: string = await invoke('get_wayback_url', { waybackUrl: snapshot.url });
                                                        if (confirm(`Found in Wayback Machine!\n\nArchived: ${snapshot.timestamp}\n\nUse archived URL to retry download?`)) {
                                                            await invoke('refresh_download_url', { id: task.id, newUrl: downloadUrl });
                                                            alert('✅ URL refreshed with Wayback archive. Click Resume to retry.');
                                                        }
                                                    } else {
                                                        alert('❌ No archived version found in the Wayback Machine.');
                                                    }
                                                } catch (err) {
                                                    alert('Wayback check failed: ' + err);
                                                } finally {
                                                    setCheckingWayback(false);
                                                }
                                            }}
                                            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
                                        >
                                            <Globe size={14} /> {checkingWayback ? 'Searching...' : '🕸 Try Wayback Machine'}
                                        </button>
                                    )}
                                </div>
                            )}

                            {/* More Details Grid */}
                            <div className="grid grid-cols-3 gap-3 text-xs text-slate-500 mt-3 p-3 bg-slate-900/50 rounded-lg border border-slate-700/30">
                                <div>ID: <span className="text-slate-300 font-mono ml-1">{task.id.split('_').pop()}</span></div>
                                <div>Threads: <span className="text-slate-300 ml-1">{(task.segments || []).filter(s => s.state === 'Downloading').length}</span></div>
                                <div>Server: <span className="text-slate-300 ml-1">Multi-Threaded</span></div>
                            </div>
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {/* Modals */}
            {showPreview && (
                <ZipPreviewModal
                    isOpen={showPreview}
                    filePath={`C:\\Users\\aditya\\Desktop\\${task.filename}`}
                    url={task.url}
                    onClose={() => setShowPreview(false)}
                    isPartial={task.status === 'Downloading' || task.status === 'Paused'}
                />
            )}

            <P2PShareModal
                isOpen={showP2PShare}
                onClose={() => setShowP2PShare(false)}
                downloadId={task.id}
                downloadName={task.filename}
            />
        </motion.div>
    );
});
