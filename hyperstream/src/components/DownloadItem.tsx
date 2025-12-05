import React from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface DownloadTask {
    id: string;
    filename: string;
    url: string;
    progress: number; // 0-100
    downloaded: number; // bytes
    total: number; // bytes
    speed: number; // bytes/sec
    status: 'Downloading' | 'Paused' | 'Error' | 'Done';
}

interface DownloadItemProps {
    task: DownloadTask;
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
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

const getStatusBadge = (status: string) => {
    switch (status) {
        case 'Downloading':
            return <span className="status-badge status-downloading">⬇ Downloading</span>;
        case 'Paused':
            return <span className="status-badge status-paused">⏸ Paused</span>;
        case 'Done':
            return <span className="status-badge status-done">✓ Complete</span>;
        case 'Error':
            return <span className="status-badge status-error">⚠ Error</span>;
        default:
            return null;
    }
};

export const DownloadItem: React.FC<DownloadItemProps> = ({ task, onPause, onResume, onDelete }) => {
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed) : '--:--';
    const category = getFileCategory(task.filename);

    const handleOpenFile = async () => {
        await invoke('open_file', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}` });
    };

    const handleOpenFolder = async () => {
        await invoke('open_folder', { path: `C:\\Users\\aditya\\Desktop\\${task.filename}` });
    };

    return (
        <div className="download-item">
            <div className="file-icon" style={{ color: category.color }}>{category.icon}</div>
            <div className="file-info">
                <div className="file-name">
                    {task.filename}
                    <span className="file-category" style={{ background: `${category.color}20`, color: category.color }}>
                        {category.label}
                    </span>
                </div>
                <div className="file-url">{task.url}</div>
            </div>
            <div className="progress-section">
                <div className="progress-bar-bg">
                    <div
                        className="progress-bar-fill"
                        style={{ width: `${task.progress}%` }}
                    ></div>
                </div>
                <div className="progress-text">{task.progress.toFixed(1)}%</div>
            </div>
            <div className="stats">
                <div className="size">{formatBytes(task.downloaded)} / {formatBytes(task.total)}</div>
                {task.status === 'Downloading' ? (
                    <>
                        <div className="speed">{formatSpeed(task.speed)}</div>
                        <div className="eta">ETA: {eta}</div>
                    </>
                ) : (
                    getStatusBadge(task.status)
                )}
            </div>
            <div className="actions">
                {task.status === 'Downloading' && (
                    <button onClick={() => onPause(task.id)} className="action-btn pause-btn" title="Pause">⏸</button>
                )}
                {task.status === 'Paused' && (
                    <button onClick={() => onResume(task.id)} className="action-btn resume-btn" title="Resume">▶</button>
                )}
                {task.status === 'Done' && (
                    <>
                        <button onClick={handleOpenFile} className="action-btn folder-btn" title="Open File">📂</button>
                        <button onClick={handleOpenFolder} className="action-btn folder-btn" title="Open Folder">📁</button>
                    </>
                )}
                {(task.status === 'Done' || task.status === 'Error' || task.status === 'Paused') && onDelete && (
                    <button onClick={() => onDelete(task.id)} className="action-btn delete-btn" title="Delete">🗑️</button>
                )}
            </div>
        </div>
    );
};
