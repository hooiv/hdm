import React, { useEffect, useState, useRef } from "react";
import { safeInvoke as invoke, safeListen as listen } from "./utils/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DownloadProgressPayload, SavedDownload, DownloadTask } from "./types";
import { toTaskStatus } from "./types";
import { debug } from "./utils/logger";
import { formatSpeed, formatBytes } from "./utils/formatters";
import { X, ArrowDownToLine } from "lucide-react";

export default function Overlay() {
    const [tasks, setTasks] = useState<DownloadTask[]>([]);
    const removalTimers = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());
    const completedIds = useRef<Set<string>>(new Set());

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
                    // New download appeared — fetch its real filename asynchronously
                    invoke<SavedDownload[]>('get_downloads').then(data => {
                        const match = data.find(d => d.id === id);
                        if (match) {
                            setTasks(curr => curr.map(t =>
                                t.id === id && t.filename === 'Downloading...'
                                    ? { ...t, filename: match.filename }
                                    : t
                            ));
                        }
                    }).catch(() => {});

                    return [...prev, {
                        id,
                        filename: 'Downloading...',
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
                        // Guard against division by zero / near-zero timeDiff
                        const speed = timeDiff > 0.05 && bytesDiff > 0 ? bytesDiff / timeDiff : t.speed || 0;
                        let status = t.status;
                        if (total > 0 && downloaded >= total) {
                            status = 'Done';
                        }
                        const updated: DownloadTask = { ...t, downloaded, total, progress: total > 0 ? (downloaded / total) * 100 : 0, speed, lastUpdate: now, status };
                        if (status === 'Done' && !completedIds.current.has(id)) {
                            completedIds.current.add(id);
                            const timer = setTimeout(() => {
                                setTasks(curr => curr.filter(x => x.id !== id));
                                removalTimers.current.delete(timer);
                                completedIds.current.delete(id);
                            }, 30000);
                            removalTimers.current.add(timer);
                        }
                        return updated;
                    }
                    return t;
                });
            });
        });

        return () => {
            unlistenProgress.then(fn => fn());
            // Clean up all pending removal timers
            removalTimers.current.forEach(t => clearTimeout(t));
            removalTimers.current.clear();
        };
    }, []);

    // Drag window frame
    const handleDragStart = (e: React.MouseEvent) => {
        if ((e.target as HTMLElement).tagName !== "BUTTON") {
            getCurrentWindow().startDragging();
        }
    };

    // Aggregate speed
    const totalSpeed = tasks.filter(t => t.status === 'Downloading').reduce((acc, t) => acc + (t.speed || 0), 0);

    return (
        <div
            className="h-screen bg-black/85 backdrop-blur-xl rounded-xl border border-white/10 text-white overflow-hidden flex flex-col select-none"
            onMouseDown={handleDragStart}
        >
            {/* Header */}
            <div className="px-3 py-2 border-b border-white/10 flex justify-between items-center bg-slate-900/50">
                <div className="flex items-center gap-2">
                    <div className="w-4 h-4 bg-gradient-to-br from-blue-500 to-violet-600 rounded flex items-center justify-center text-[8px] font-bold">
                        H
                    </div>
                    <span className="text-[11px] font-bold tracking-wider text-slate-300">HYPERSTREAM</span>
                </div>
                <div className="flex items-center gap-2">
                    {totalSpeed > 0 && (
                        <span className="text-[10px] font-mono text-cyan-400">{formatSpeed(totalSpeed)}</span>
                    )}
                    <button
                        onClick={() => getCurrentWindow().hide()}
                        className="p-1 text-slate-500 hover:text-white hover:bg-white/10 rounded transition-colors"
                    >
                        <X size={12} />
                    </button>
                </div>
            </div>

            {/* Download List */}
            <div className="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1.5">
                {tasks.map(task => (
                    <div key={task.id} className="bg-white/5 rounded-lg p-2.5 border border-white/5">
                        <div className="flex justify-between items-center mb-1.5">
                            <span className="text-[10px] font-medium text-slate-200 truncate max-w-[170px]">{task.filename}</span>
                            <span className="text-[9px] font-mono text-slate-400 shrink-0 ml-2">
                                {task.status === 'Downloading' ? formatSpeed(task.speed) : task.status === 'Done' ? 'Done' : task.status}
                            </span>
                        </div>
                        <div className="w-full h-1.5 bg-white/5 rounded-full overflow-hidden">
                            <div
                                className={`h-full rounded-full transition-all duration-300 ${
                                    task.status === 'Done' ? 'bg-emerald-500' :
                                    task.status === 'Error' ? 'bg-red-500' :
                                    'bg-gradient-to-r from-cyan-500 to-blue-600'
                                }`}
                                style={{ width: `${task.progress}%` }}
                            />
                        </div>
                        <div className="flex justify-between mt-1">
                            <span className="text-[8px] text-slate-500 font-mono">
                                {formatBytes(task.downloaded)}{task.total > 0 ? ` / ${formatBytes(task.total)}` : ''}
                            </span>
                            <span className="text-[8px] text-slate-500 font-mono">
                                {task.total > 0 ? `${task.progress.toFixed(1)}%` : ''}
                            </span>
                        </div>
                    </div>
                ))}
                {tasks.length === 0 && (
                    <div className="flex flex-col items-center justify-center h-full text-slate-500 gap-2 py-8">
                        <ArrowDownToLine size={24} className="text-slate-600" />
                        <span className="text-[11px]">No active downloads</span>
                    </div>
                )}
            </div>
        </div>
    );
}
