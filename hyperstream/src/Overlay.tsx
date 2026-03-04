import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DownloadProgressPayload, SavedDownload } from "./types";

interface DownloadTask {
    id: string;
    filename: string;
    progress: number;
    speed: number;
    status: string;
    total: number;
    downloaded: number;
    lastUpdate?: number;
}

const formatSpeed = (bytesPerSec: number) => {
    if (!bytesPerSec || bytesPerSec <= 0) return '0 B/s';
    const k = 1024;
    const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    const i = Math.min(Math.floor(Math.log(bytesPerSec) / Math.log(k)), sizes.length - 1);
    return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

export default function Overlay() {
    const [tasks, setTasks] = useState<DownloadTask[]>([]);

    useEffect(() => {
        // Initial fetch
        invoke<SavedDownload[]>('get_downloads').then((data) => {
            // Basic fetch, mostly relying on events
            console.log("Overlay initialized", data);
        });

        // Listen for progress
        const unlistenProgress = listen<DownloadProgressPayload>('download_progress', (event) => {
            const { id, downloaded, total } = event.payload;
            setTasks(prev => {
                // If task doesn't exist, we might need to fetch full list or ignore
                // For MVP, we only update existing or rely on list sync
                return prev.map(t => {
                    if (t.id === id) {
                        const now = Date.now();
                        const timeDiff = (now - (t.lastUpdate || now)) / 1000;
                        const bytesDiff = downloaded - t.downloaded;
                        const speed = timeDiff > 0 ? bytesDiff / timeDiff : 0;

                        return { ...t, downloaded, total, progress: (downloaded / total) * 100, speed, lastUpdate: now };
                    }
                    return t;
                });
            });
        });

        return () => {
            unlistenProgress.then(f => f());
        };
    }, []);

    // Drag window frame
    const handleDragStart = (e: React.MouseEvent) => {
        if ((e.target as HTMLElement).tagName !== "BUTTON") {
            getCurrentWindow().startDragging();
        }
    };

    return (
        <div
            style={{
                height: '100vh',
                background: 'rgba(0, 0, 0, 0.85)',
                backdropFilter: 'blur(10px)',
                borderRadius: '12px',
                border: '1px solid rgba(255, 255, 255, 0.1)',
                color: 'white',
                overflow: 'hidden',
                display: 'flex',
                flexDirection: 'column',
                userSelect: 'none'
            }}
            onMouseDown={handleDragStart}
        >
            <div style={{ padding: '8px', borderBottom: '1px solid rgba(255,255,255,0.1)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span style={{ fontSize: '12px', fontWeight: 'bold' }}>HyperStream</span>
                <button
                    onClick={() => getCurrentWindow().hide()}
                    style={{ background: 'none', border: 'none', color: 'rgba(255,255,255,0.5)', cursor: 'pointer' }}
                >
                    ✕
                </button>
            </div>

            <div style={{ flex: 1, padding: '8px', overflowY: 'auto' }}>
                {tasks.map(task => (
                    <div key={task.id} style={{ marginBottom: '8px', fontSize: '10px' }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '2px' }}>
                            <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', maxWidth: '180px' }}>{task.filename}</span>
                            <span>{formatSpeed(task.speed)}</span>
                        </div>
                        <div style={{ width: '100%', height: '4px', background: 'rgba(255,255,255,0.1)', borderRadius: '2px' }}>
                            <div style={{ width: `${task.progress}%`, height: '100%', background: '#646cff', borderRadius: '2px' }} />
                        </div>
                    </div>
                ))}
                {tasks.length === 0 && (
                    <div style={{ textAlign: 'center', marginTop: '20px', color: 'rgba(255,255,255,0.4)', fontSize: '12px' }}>
                        No active downloads
                    </div>
                )}
            </div>
        </div>
    );
}
