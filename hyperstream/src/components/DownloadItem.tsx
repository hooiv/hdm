import React from 'react';

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

export const DownloadItem: React.FC<DownloadItemProps> = ({ task, onPause, onResume, onDelete }) => {
    const remainingBytes = task.total - task.downloaded;
    const eta = task.status === 'Downloading' ? formatETA(remainingBytes, task.speed) : '--:--';

    return (
        <div className="download-item">
            <div className="file-icon">📄</div>
            <div className="file-info">
                <div className="file-name">{task.filename}</div>
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
                <div className="speed">{task.status === 'Downloading' ? formatSpeed(task.speed) : '-'}</div>
                <div className="eta">{task.status === 'Downloading' ? `ETA: ${eta}` : task.status}</div>
            </div>
            <div className="actions">
                {task.status === 'Downloading' && (
                    <button onClick={() => onPause(task.id)} className="action-btn pause-btn">⏸</button>
                )}
                {task.status === 'Paused' && (
                    <button onClick={() => onResume(task.id)} className="action-btn resume-btn">▶</button>
                )}
                {(task.status === 'Done' || task.status === 'Error' || task.status === 'Paused') && onDelete && (
                    <button onClick={() => onDelete(task.id)} className="action-btn delete-btn">🗑️</button>
                )}
            </div>
        </div>
    );
};
