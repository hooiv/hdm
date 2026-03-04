import React, { useEffect, useState, useMemo } from 'react';
import { DownloadTask } from './DownloadItem';
import { Activity, Server } from 'lucide-react';

interface GlobalTelemetryProps {
    tasks: DownloadTask[];
}

export const GlobalTelemetry: React.FC<GlobalTelemetryProps> = ({ tasks }) => {
    const [history, setHistory] = useState<number[]>(Array(50).fill(0));

    // Calculate instantaneous aggregate speed
    const currentSpeed = useMemo(() => {
        return tasks.filter(t => t.status === 'Downloading').reduce((acc, t) => acc + (t.speed || 0), 0);
    }, [tasks]);

    // Active connection count (number of downloading tasks)
    const activeConnections = tasks.filter(t => t.status === 'Downloading').length;

    // Update history every 500ms
    useEffect(() => {
        const interval = setInterval(() => {
            setHistory(prev => {
                const next = [...prev.slice(1), currentSpeed];
                return next;
            });
        }, 500);
        return () => clearInterval(interval);
    }, [currentSpeed]);

    // Format speed Helper
    const formatSpeed = (bps: number) => {
        if (!bps || bps <= 0) return '0 B/s';
        const k = 1024;
        const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
        const i = Math.min(Math.floor(Math.log(bps) / Math.log(k)), sizes.length - 1);
        return parseFloat((bps / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    // Calculate SVG Path for Sparkline
    const maxSpeed = Math.max(...history, 1024 * 1024); // Minimum scale 1MB/s
    const points = history.map((val, i) => {
        const x = (i / (history.length - 1)) * 100;
        const y = 100 - (val / maxSpeed) * 100;
        return `${x},${y}`;
    }).join(' ');

    const polygonPoints = `0,100 ${points} 100,100`;

    return (
        <div className="mb-6 relative w-full h-32 rounded-xl border border-cyan-800/50 bg-slate-950 overflow-hidden shadow-[inset_0_0_30px_rgba(6,182,212,0.05)]">

            {/* Background Grid & Scanlines */}
            <div className="absolute inset-0 telemetry-bg opacity-30 pointer-events-none" />
            <div className="absolute inset-0 crt-scanlines opacity-20 pointer-events-none" />

            {/* SVG Sparkline Graph */}
            <svg className="absolute inset-0 w-full h-full pb-0 pt-6" preserveAspectRatio="none" viewBox="0 0 100 100">
                <defs>
                    <linearGradient id="glowGradient" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="rgba(6, 182, 212, 0.4)" />
                        <stop offset="100%" stopColor="rgba(6, 182, 212, 0.0)" />
                    </linearGradient>
                </defs>
                <polygon points={polygonPoints} fill="url(#glowGradient)" />
                <polyline
                    fill="none"
                    stroke="#06b6d4"
                    strokeWidth="0.5"
                    points={points}
                    style={{
                        filter: 'drop-shadow(0px 0px 4px rgba(6,182,212,0.8))'
                    }}
                />
            </svg>

            {/* HUD Overlay Stats */}
            <div className="absolute inset-0 p-4 flex justify-between items-start pointer-events-none">
                <div>
                    <div className="flex items-center gap-2 text-cyan-500 mb-1">
                        <Activity size={14} className="animate-pulse" />
                        <span className="text-[10px] font-mono font-bold tracking-[0.2em] uppercase">Global Telemetry</span>
                    </div>
                    <div className="text-2xl font-mono font-bold text-white text-glow tracking-tight">
                        {formatSpeed(currentSpeed)}
                    </div>
                </div>

                <div className="text-right flex flex-col items-end">
                    <div className="flex items-center gap-2 text-slate-400 mb-1">
                        <span className="text-[10px] font-mono tracking-[0.1em] uppercase">Active Nodes</span>
                        <Server size={14} />
                    </div>
                    <div className="text-lg font-mono font-bold text-cyan-300">
                        {activeConnections} <span className="text-xs text-slate-500 font-normal">streams</span>
                    </div>
                </div>
            </div>

            {/* Bottom Axis Label */}
            <div className="absolute bottom-1 left-2 text-[8px] font-mono text-cyan-800/80">T-25 SECONDS</div>
            <div className="absolute bottom-1 right-2 w-2 h-2 rounded-full bg-cyan-500 shadow-[0_0_10px_#06b6d4] animate-pulse" />

            <style>{`
                .telemetry-bg {
                    background-image: 
                        linear-gradient(rgba(6, 182, 212, 0.1) 1px, transparent 1px),
                        linear-gradient(90deg, rgba(6, 182, 212, 0.1) 1px, transparent 1px);
                    background-size: 20px 20px;
                }
                .crt-scanlines {
                    background: linear-gradient(to bottom, rgba(255,255,255,0), rgba(255,255,255,0) 50%, rgba(0,0,0,0.3) 50%, rgba(0,0,0,0.3));
                    background-size: 100% 4px;
                }
                .text-glow {
                    text-shadow: 0 0 10px rgba(6, 182, 212, 0.6), 0 0 20px rgba(6, 182, 212, 0.4);
                }
            `}</style>
        </div>
    );
};
