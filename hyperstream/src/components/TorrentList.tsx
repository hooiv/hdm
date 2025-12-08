import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TorrentStatus } from '../types';

interface TorrentListProps {
    onPlay: (id: number) => void;
}

const TorrentItem: React.FC<{ status: TorrentStatus, onPlay: (id: number) => void }> = ({ status, onPlay }) => {
    const formatSpeed = (bytes: number) => {
        if (bytes === 0) return '0 B/s';
        const k = 1024;
        const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    return (
        <div className="download-item">
            <div className="file-icon" style={{ color: '#14b8a6' }}>🧲</div>

            <div className="file-info">
                <div className="file-name">{status.name}</div>
                <div className="file-url" style={{ fontSize: '0.8em', color: '#888' }}>
                    Peers: {status.peers} | State: {status.state}
                </div>
            </div>

            <div className="progress-section">
                <div className="progress-bar-bg">
                    <div className="progress-bar-fill" style={{ width: `${status.progress_percent}%`, backgroundColor: '#14b8a6' }}></div>
                </div>
                <div className="progress-text">{status.progress_percent.toFixed(1)}%</div>
            </div>

            <div className="stats">
                <div className="size" style={{ fontSize: '0.8em' }}>
                    ⬇ {formatSpeed(status.speed_download)} | ⬆ {formatSpeed(status.speed_upload)}
                </div>
            </div>

            <div className="actions">
                <button
                    onClick={() => onPlay(status.id)}
                    className="action-btn"
                    title="Stream"
                    style={{ backgroundColor: '#14b8a6', color: 'white', border: 'none', borderRadius: '4px', padding: '4px 8px', cursor: 'pointer' }}
                >
                    ▶ Stream
                </button>
            </div>
        </div>
    );
};

export const TorrentList: React.FC<TorrentListProps> = ({ onPlay }) => {
    const [torrents, setTorrents] = useState<TorrentStatus[]>([]);

    useEffect(() => {
        const fetchTorrents = async () => {
            try {
                const list = await invoke<TorrentStatus[]>('get_torrents');
                setTorrents(list);
            } catch (e) {
                console.error("Failed to fetch torrents", e);
            }
        };

        fetchTorrents();
        const interval = setInterval(fetchTorrents, 1000);
        return () => clearInterval(interval);
    }, []);

    return (
        <div className="download-list">
            {torrents.map(t => (
                <TorrentItem key={t.id} status={t} onPlay={onPlay} />
            ))}
            {torrents.length === 0 && (
                <div className="empty-state" style={{ padding: '20px', textAlign: 'center', color: '#666' }}>
                    No active torrents. Add a magnet link to start.
                </div>
            )}
        </div>
    );
};
