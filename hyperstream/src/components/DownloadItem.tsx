import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ZipPreviewModal } from './ZipPreviewModal';
import { Segment } from '../types';
import { ThreadVisualizer } from './ThreadVisualizer';
import { motion, AnimatePresence } from 'framer-motion';
import { Folder, Play, Pause, Trash2, FileText, ChevronDown, Archive, ArrowUp, ArrowDown } from 'lucide-react';

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
const getFileCategory = (filename: string): { icon: string; label: string; color: string } => {
    const ext = filename.split('.').pop()?.toLowerCase() || '';

    const categories: Record<string, { icon: string; label: string; color: string }> = {
        // Video
        mp4: { icon: '🎬', label: 'Video', color: '#ef4444' },
        mkv: { icon: '🎬', label: 'Video', color: '#ef4444' },
        avi: { icon: '🎬', label: 'Video', color: '#ef4444' },
        mov: { icon: '🎬', label: 'Video', color: '#ef4444' },
        webm: { icon: '🎬', label: 'Video', color: '#ef4444' },
        // Audio
        mp3: { icon: '🎵', label: 'Audio', color: '#8b5cf6' },
        flac: { icon: '🎵', label: 'Audio', color: '#8b5cf6' },
        wav: { icon: '🎵', label: 'Audio', color: '#8b5cf6' },
        aac: { icon: '🎵', label: 'Audio', color: '#8b5cf6' },
        // Archives
        zip: { icon: '📦', label: 'Archive', color: '#f59e0b' },
        rar: { icon: '📦', label: 'Archive', color: '#f59e0b' },
        '7z': { icon: '📦', label: 'Archive', color: '#f59e0b' },
        tar: { icon: '📦', label: 'Archive', color: '#f59e0b' },
        gz: { icon: '📦', label: 'Archive', color: '#f59e0b' },
        // Programs
        exe: { icon: '⚙️', label: 'Program', color: '#22c55e' },
        msi: { icon: '⚙️', label: 'Program', color: '#22c55e' },
        dmg: { icon: '⚙️', label: 'Program', color: '#22c55e' },
        // Documents
        pdf: { icon: '📄', label: 'Document', color: '#3b82f6' },
        doc: { icon: '📄', label: 'Document', color: '#3b82f6' },
        docx: { icon: '📄', label: 'Document', color: '#3b82f6' },
        // Images
        jpg: { icon: '🖼️', label: 'Image', color: '#ec4899' },
        jpeg: { icon: '🖼️', label: 'Image', color: '#ec4899' },
        png: { icon: '🖼️', label: 'Image', color: '#ec4899' },
        gif: { icon: '🖼️', label: 'Image', color: '#ec4899' },
        // ISO
        iso: { icon: '💿', label: 'Disk Image', color: '#14b8a6' },
    };

    return categories[ext] || { icon: '📄', label: 'File', color: '#64748b' };
};

// Memoize Item to prevent re-renders of non-updating downloads
export const DownloadItem = React.memo<DownloadItemProps>(({ task, onPause, onResume, onDelete, onMoveUp, onMoveDown }) => {
    const [showPreview, setShowPreview] = useState(false);
    const [isExpanded, setIsExpanded] = useState(false);

    // Derived values
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed) : '--:--';
    const category = getFileCategory(task.filename);
    const progressColor = category.color;

    const handleOpenFolder = async () => {
        await invoke('open_folder', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}` });
    };

    return (
        <motion.div
            layout
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.2 }}
            className={`download-item ${task.status === 'Downloading' ? 'active-glow' : ''}`}
            onClick={() => setIsExpanded(!isExpanded)}
            style={{ position: 'relative', overflow: 'hidden' }}
        >
            <div className="download-item-main" style={{ display: 'flex', alignItems: 'center', padding: '15px' }}>
                <div className="file-icon" style={{
                    color: category.color,
                    marginRight: '15px',
                    fontSize: '1.5rem',
                    background: `${category.color}10`,
                    padding: '10px',
                    borderRadius: '10px'
                }}>
                    <FileText size={24} />
                </div>

                <div className="file-info" style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: 'flex', alignItems: 'center', marginBottom: '5px' }}>
                        <div className="file-name" title={task.filename} style={{ fontWeight: 600, color: '#f8fafc' }}>
                            {task.filename}
                        </div>
                        <span className="file-category" style={{
                            marginLeft: '10px',
                            fontSize: '0.7em',
                            background: `${category.color}20`,
                            color: category.color,
                            padding: '2px 8px',
                            borderRadius: '12px'
                        }}>
                            {category.label}
                        </span>
                        {task.speed > 0 && (
                            <span className="speed-badge" style={{
                                marginLeft: 'auto',
                                color: '#3b82f6',
                                fontSize: '0.85em',
                                background: 'rgba(59, 130, 246, 0.1)',
                                padding: '2px 8px',
                                borderRadius: '4px'
                            }}>
                                {formatSpeed(task.speed)}
                            </span>
                        )}
                    </div>

                    <div className="file-url" style={{ fontSize: '0.8em', color: '#64748b', marginBottom: '8px' }}>
                        {task.url}
                    </div>

                    <div className="progress-section" style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                        <div className="progress-bar-bg" style={{ flex: 1, height: '6px', background: '#334155', borderRadius: '3px', overflow: 'hidden' }}>
                            <motion.div
                                className="progress-bar-fill"
                                initial={{ width: 0 }}
                                animate={{ width: `${task.progress}%` }}
                                transition={{ type: "spring", stiffness: 100, damping: 20 }}
                                style={{ height: '100%', background: `linear-gradient(90deg, ${progressColor}, #a855f7)` }}
                            />
                        </div>
                        <div className="progress-text" style={{ fontSize: '0.8em', color: '#94a3b8', width: '40px', textAlign: 'right' }}>
                            {task.progress.toFixed(1)}%
                        </div>
                    </div>

                    <div className="stats" style={{ display: 'flex', justifyContent: 'space-between', marginTop: '5px', fontSize: '0.75em', color: '#64748b' }}>
                        <span>{formatBytes(task.downloaded)} / {formatBytes(task.total)}</span>
                        <span>ETA: {eta}</span>
                    </div>
                </div>

                <div className="actions" style={{ marginLeft: '15px', display: 'flex', gap: '8px' }} onClick={(e) => e.stopPropagation()}>
                    {onMoveUp && (
                        <button className="action-btn" onClick={() => onMoveUp(task.id)} title="Move Up">
                            <ArrowUp size={18} />
                        </button>
                    )}
                    {onMoveDown && (
                        <button className="action-btn" onClick={() => onMoveDown(task.id)} title="Move Down">
                            <ArrowDown size={18} />
                        </button>
                    )}

                    <button className="action-btn" onClick={handleOpenFolder} title="Open Folder">
                        <Folder size={18} />
                    </button>

                    {task.status === 'Downloading' && (
                        <button className="action-btn" onClick={() => onPause(task.id)} title="Pause" style={{ color: '#fbbf24' }}>
                            <Pause size={18} />
                        </button>
                    )}

                    {(task.status === 'Paused' || task.status === 'Error') && (
                        <button className="action-btn" onClick={() => onResume(task.id)} title="Resume" style={{ color: '#22c55e' }}>
                            <Play size={18} />
                        </button>
                    )}

                    <button className="action-btn" onClick={() => onDelete && onDelete(task.id)} title="Cancel" style={{ color: '#ef4444' }}>
                        <Trash2 size={18} />
                    </button>

                    <motion.div
                        animate={{ rotate: isExpanded ? 180 : 0 }}
                        style={{ marginLeft: '10px', cursor: 'pointer', color: '#64748b' }}
                    >
                        <ChevronDown size={18} />
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
                        style={{ overflow: 'hidden' }}
                        onClick={(e) => e.stopPropagation()}
                    >
                        <div style={{
                            borderTop: '1px solid rgba(255,255,255,0.05)',
                            padding: '15px',
                            background: 'rgba(0,0,0,0.1)'
                        }}>
                            {/* Thread Visualization */}
                            <ThreadVisualizer
                                segments={task.segments || []}
                                totalSize={task.total}
                            />

                            {/* Zip Preview if applicable */}
                            {(task.filename.endsWith('.zip') || task.filename.endsWith('.jar')) && (
                                <div style={{ marginTop: '10px' }}>
                                    <button
                                        className="preview-btn"
                                        onClick={() => setShowPreview(true)}
                                        style={{
                                            background: 'rgba(59, 130, 246, 0.1)',
                                            color: '#3b82f6',
                                            border: '1px solid rgba(59, 130, 246, 0.3)',
                                            borderRadius: '4px',
                                            padding: '6px 12px',
                                            fontSize: '0.8rem',
                                            cursor: 'pointer',
                                            display: 'flex',
                                            alignItems: 'center',
                                            gap: '6px'
                                        }}
                                    >
                                        <Archive size={14} /> Preview Archive Content
                                    </button>
                                </div>
                            )}

                            {/* More Details Grid */}
                            <div style={{
                                display: 'grid',
                                gridTemplateColumns: 'repeat(3, 1fr)',
                                gap: '10px',
                                fontSize: '0.75rem',
                                color: '#94a3b8',
                                marginTop: '10px',
                                background: 'rgba(0,0,0,0.2)',
                                padding: '10px',
                                borderRadius: '6px'
                            }}>
                                <div>ID: <span style={{ color: '#e2e8f0' }}>{task.id.split('_').pop()}</span></div>
                                <div>Threads: <span style={{ color: '#e2e8f0' }}>{(task.segments || []).filter(s => s.state === 'Downloading').length}</span></div>
                                <div>Server: <span style={{ color: '#e2e8f0' }}>Multi-Threaded</span></div>
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
                    onClose={() => setShowPreview(false)}
                    isPartial={task.status === 'Downloading'}
                />
            )}
        </motion.div>
    );
});
