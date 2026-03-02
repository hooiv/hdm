import React from 'react';
import { Segment } from '../types';
import { motion, AnimatePresence } from 'framer-motion';

interface ThreadVisualizerProps {
    segments: Segment[];
    totalSize: number;
}

export const ThreadVisualizer = React.memo<ThreadVisualizerProps>(({ segments, totalSize }) => {
    const effectiveTotal = totalSize > 0 ? totalSize : segments.reduce((acc, seg) => Math.max(acc, seg.end_byte), 0);

    return (
        <div className="cyber-hud-container mt-2">

            {/* HUD Header */}
            <div className="flex justify-between items-end mb-1 px-1">
                <span className="text-[9px] font-mono text-cyan-500/70 uppercase tracking-[0.2em]">
                    SYS.ACQUISITION.THREADS // {segments.length}
                </span>
                <span className="text-[9px] font-mono text-fuchsia-500/50 uppercase tracking-[0.1em]">
                    BLOCK ALLOCATION MAP
                </span>
            </div>

            {/* The Main Visualizer Block */}
            <div className="relative h-8 w-full bg-slate-950 border border-cyan-900/40 rounded-sm overflow-hidden 
                            shadow-[inset_0_0_20px_rgba(0,0,0,0.8)] cyber-grid-bg">

                {/* CRT Scanline Overlay */}
                <div className="absolute inset-0 pointer-events-none crt-scanlines z-20 opacity-20" />

                <AnimatePresence>
                    {segments.map((seg) => {
                        const widthPercent = ((seg.end_byte - seg.start_byte) / effectiveTotal) * 100;
                        const leftPercent = (seg.start_byte / effectiveTotal) * 100;
                        const progressPercent = ((Math.min(seg.downloaded_cursor, seg.end_byte) - seg.start_byte) / (seg.end_byte - seg.start_byte)) * 100;

                        // Cyberpunk Color Palette
                        let baseColor = 'rgba(15, 23, 42, 0.4)'; // Slate 900
                        let fillColor = 'rgba(71, 85, 105, 0.5)'; // Idle Slate
                        let glow = 'none';
                        let border = 'rgba(56, 189, 248, 0.1)';

                        if (seg.state === 'Downloading') {
                            fillColor = '#06b6d4'; // Cyan 500
                            glow = '0 0 15px rgba(6, 182, 212, 0.6)';
                            border = 'rgba(6, 182, 212, 0.8)';
                        } else if (seg.state === 'Paused') {
                            fillColor = '#f59e0b'; // Amber 500
                            glow = '0 0 10px rgba(245, 158, 11, 0.4)';
                            border = 'rgba(245, 158, 11, 0.6)';
                        } else if (seg.state === 'Complete') {
                            fillColor = '#10b981'; // Emerald 500
                            glow = '0 0 8px rgba(16, 185, 129, 0.3)';
                            border = 'rgba(16, 185, 129, 0.4)';
                        } else if (seg.state === 'Error') {
                            fillColor = '#f43f5e'; // Rose 500
                            glow = '0 0 15px rgba(244, 63, 94, 0.6)';
                            border = 'rgba(244, 63, 94, 0.8)';
                        }

                        return (
                            <motion.div
                                key={seg.id}
                                layoutId={`seg-${seg.id}`}
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1 }}
                                exit={{ opacity: 0 }}
                                className="absolute top-0 bottom-0 box-border group"
                                style={{
                                    left: `${leftPercent}%`,
                                    width: `${widthPercent}%`,
                                    backgroundColor: baseColor,
                                    borderRight: `1px solid ${border}`,
                                    zIndex: seg.state === 'Downloading' ? 10 : 5
                                }}
                            >
                                {/* Active Data Stream Fill */}
                                <motion.div
                                    animate={{
                                        width: `${progressPercent}%`,
                                        backgroundColor: fillColor,
                                        boxShadow: glow
                                    }}
                                    transition={{ type: 'tween', ease: 'linear', duration: 0.1 }}
                                    className={`absolute left-0 top-0 bottom-0 opacity-80 mix-blend-screen
                                                ${seg.state === 'Downloading' ? 'data-stream-anim' : ''}`}
                                />

                                {/* Glitch Overlay on Error */}
                                {seg.state === 'Error' && <div className="absolute inset-0 bg-red-500/20 animate-pulse mix-blend-overlay" />}

                                {/* HUD Readout on Hover */}
                                <div className="absolute inset-0 opacity-0 group-hover:opacity-100 bg-black/80 transition-opacity z-30 flex flex-col justify-center items-center backdrop-blur-sm pointer-events-none p-1">
                                    <span className="text-[8px] font-mono text-cyan-400">ID: {seg.id}</span>
                                    {seg.state === 'Downloading' && (
                                        <span className="text-[7px] font-mono text-white">{(seg.speed_bps / 1024 / 1024).toFixed(2)} MB/s</span>
                                    )}
                                </div>
                            </motion.div>
                        );
                    })}
                </AnimatePresence>
            </div>

            {/* Local Styles for Effects */}
            <style>{`
                .cyber-hud-container {
                    background: linear-gradient(180deg, rgba(8, 14, 23, 0.7) 0%, rgba(3, 7, 18, 0.9) 100%);
                    padding: 8px;
                    border-radius: 6px;
                    border: 1px solid rgba(6, 182, 212, 0.15);
                    box-shadow: 0 4px 20px rgba(0,0,0,0.5);
                }
                .cyber-grid-bg {
                    background-image: 
                        linear-gradient(rgba(6, 182, 212, 0.05) 1px, transparent 1px),
                        linear-gradient(90deg, rgba(6, 182, 212, 0.05) 1px, transparent 1px);
                    background-size: 8px 8px;
                }
                .crt-scanlines {
                    background: linear-gradient(
                        to bottom,
                        rgba(255,255,255,0),
                        rgba(255,255,255,0) 50%,
                        rgba(0,0,0,0.2) 50%,
                        rgba(0,0,0,0.2)
                    );
                    background-size: 100% 4px;
                }
                .data-stream-anim {
                    background-image: repeating-linear-gradient(
                        -45deg,
                        transparent,
                        transparent 4px,
                        rgba(255, 255, 255, 0.15) 4px,
                        rgba(255, 255, 255, 0.15) 8px
                    );
                    background-size: 16px 16px;
                    animation: stream-pan 1s linear infinite;
                }
                @keyframes stream-pan {
                    from { background-position: 0 0; }
                    to { background-position: 16px 0; }
                }
            `}</style>
        </div>
    );
});
