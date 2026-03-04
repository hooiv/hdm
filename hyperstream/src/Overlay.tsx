import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DownloadProgressPayload, SavedDownload, DownloadTask } from "./types";
import { toTaskStatus } from "./types";
import { debug } from "./utils/logger";
import { formatSpeed } from "./utils/formatters";


export default function Overlay() {
    const [tasks, setTasks] = useState<DownloadTask[]>([]);

    useEffect(() => {
        // Initial fetch – populate existing downloads so overlay isn't blank
        invoke<SavedDownload[]>('get_downloads')
            .then((data) => {
                if (data.length > 0) {
                    const initial: DownloadTask[] = data.map(d => ({
                        id: d.id,
                        filename: d.filename,
                        progress: d.total_size > 0 ? (d.downloaded_bytes / d.total_size) * 100 : 0,
                        downloaded: d.downloaded_bytes,
                        total: d.total_size,
                        speed: 0,
                        status: toTaskStatus(d.status),
                    }));
                    setTasks(initial);
                }
            })
            .catch(e => debug("Overlay load error", e));

        // Listen for progress updates and patch existing entries, or add new ones
        const unlistenProgress = listen<DownloadProgressPayload>('download_progress', (event) => {
            const { id, downloaded, total } = event.payload;
            setTasks(prev => {
                const exists = prev.some(t => t.id === id);
                if (!exists) {
                    // New download appeared — add it
                    return [...prev, {
                        id,
                        filename: id,
                        downloaded,
                        total,
                        progress: total > 0 ? (downloaded / total) * 100 : 0,
                        speed: 0,
                        status: 'Downloading' as const,
                        lastUpdate: Date.now(),
                    }];
                }
                return prev.map(t => {
                    if (t.id === id) {
                        const now = Date.now();
                        const timeDiff = (now - (t.lastUpdate || now)) / 1000;
                        const bytesDiff = downloaded - t.downloaded;
                        const speed = timeDiff > 0 && bytesDiff > 0 ? bytesDiff / timeDiff : 0;
                        let status = t.status;
                        if (total > 0 && downloaded >= total) {
                            status = 'Done';
                        }
                        const updated: DownloadTask = { ...t, downloaded, total, progress: total > 0 ? (downloaded / total) * 100 : 0, speed, lastUpdate: now, status };
                        if (status === 'Done') {
                            // schedule removal in overlay as well
                            setTimeout(() => {
                                setTasks(curr => curr.filter(x => x.id !== id));
                            }, 30000);
                        }
                        return updated;
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
