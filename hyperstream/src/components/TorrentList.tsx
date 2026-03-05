import React, { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TorrentStatus } from '../types';
import { Magnet, Play } from 'lucide-react';
import { formatSpeed } from '../utils/formatters';
import { error as logError } from '../utils/logger';

interface TorrentListProps {
    onPlay: (id: number) => void;
}

const TorrentItem: React.FC<{ status: TorrentStatus, onPlay: (id: number) => void }> = ({ status, onPlay }) => {
    return (
        <div className="relative mb-3 bg-slate-800/50 border border-slate-700/50 rounded-xl p-4 flex items-center hover:bg-slate-800/80 transition-colors">

            <div className="p-3 bg-teal-500/10 rounded-lg mr-4">
                <Magnet className="text-teal-400" size={24} />
            </div>

            <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between mb-1">
                    <div className="font-semibold text-slate-200 truncate pr-4">{status.name || 'Retrieving Metadata...'}</div>
                    <div className="text-xs font-mono text-teal-400 bg-teal-400/10 px-2 py-0.5 rounded">
                        {status.state}
                    </div>
                </div>

                <div className="flex items-center gap-4 text-xs text-slate-500 mb-2">
                    <span className="flex items-center gap-1">
                        👥 <span className="text-slate-300">{status.peers}</span> peers
                    </span>
                    <span className="flex items-center gap-1">
                        ⬇ <span className="text-slate-300">{formatSpeed(status.speed_download)}</span>
                    </span>
                    <span className="flex items-center gap-1">
                        ⬆ <span className="text-slate-300">{formatSpeed(status.speed_upload)}</span>
                    </span>
                </div>

                <div className="flex items-center gap-3">
                    <div className="flex-1 h-1.5 bg-slate-700/50 rounded-full overflow-hidden">
                        <div
                            className="h-full bg-teal-500 rounded-full transition-all duration-300 ease-out"
                            style={{ width: `${status.progress_percent}%` }}
                        />
                    </div>
                    <span className="text-xs font-medium text-slate-400 w-10 text-right">
                        {status.progress_percent.toFixed(1)}%
                    </span>
                </div>
            </div>

            <button
                onClick={() => onPlay(status.id)}
                className="ml-4 p-2.5 bg-teal-600 hover:bg-teal-500 text-white rounded-lg transition-colors shadow-lg shadow-teal-900/20"
                title="Stream"
            >
                <Play size={20} fill="currentColor" />
            </button>
        </div>
    );
};

export const TorrentList: React.FC<TorrentListProps> = ({ onPlay }) => {
    const [torrents, setTorrents] = useState<TorrentStatus[]>([]);
    const fetchingRef = useRef(false);

    useEffect(() => {
        let mounted = true;
        const fetchTorrents = async () => {
            if (fetchingRef.current) return; // skip if previous fetch still in-flight
            fetchingRef.current = true;
            try {
                const list = await invoke<TorrentStatus[]>('get_torrents');
                if (mounted) setTorrents(list);
            } catch (e) {
                logError("Failed to fetch torrents", e);
            } finally {
                fetchingRef.current = false;
            }
        };

        fetchTorrents();
        const interval = setInterval(() => {
            // Skip polling when tab is hidden to save resources
            if (!document.hidden) fetchTorrents();
        }, 1000);
        return () => { mounted = false; clearInterval(interval); };
    }, []);

    return (
        <div className="w-full h-full p-4 overflow-y-auto custom-scrollbar">
            {torrents.length > 0 ? (
                torrents.map(t => (
                    <TorrentItem key={t.id} status={t} onPlay={onPlay} />
                ))
            ) : (
                <div className="flex flex-col items-center justify-center h-64 text-slate-500">
                    <div className="p-4 bg-slate-800/50 rounded-full mb-4">
                        <Magnet size={48} className="text-slate-700" />
                    </div>
                    <p className="text-lg font-medium">No Active Torrents</p>
                    <p className="text-sm opacity-70">Add a magnet link to start streaming</p>
                </div>
            )}
        </div>
    );
};
