import React from 'react';
import { Segment } from '../types';
import { motion, AnimatePresence } from 'framer-motion';

interface ThreadVisualizerProps {
    segments: Segment[];
    totalSize: number;
}

export const ThreadVisualizer = React.memo<ThreadVisualizerProps>(({ segments, totalSize }) => {
    // Determine the max end_byte to use as total size if provided totalSize is 0
    const effectiveTotal = totalSize > 0 ? totalSize : segments.reduce((acc, seg) => Math.max(acc, seg.end_byte), 0);

    return (
        <div className="thread-visualizer">
            <div className="thread-bar-container">
                <AnimatePresence>
                    {segments.map((seg) => {
                        const widthPercent = ((seg.end_byte - seg.start_byte) / effectiveTotal) * 100;
                        const leftPercent = (seg.start_byte / effectiveTotal) * 100;
                        const progressPercent = ((Math.min(seg.downloaded_cursor, seg.end_byte) - seg.start_byte) / (seg.end_byte - seg.start_byte)) * 100;

                        let color = '#64748b'; // Idle/Gray
                        let glow = 'none';

                        if (seg.state === 'Downloading') {
                            color = '#3b82f6'; // Blue
                            glow = '0 0 10px rgba(59, 130, 246, 0.5)';
                        }
                        if (seg.state === 'Paused') color = '#f59e0b'; // Orange
                        if (seg.state === 'Complete') {
                            color = '#10b981'; // Green
                            glow = '0 0 5px rgba(16, 185, 129, 0.5)';
                        }
                        if (seg.state === 'Error') color = '#ef4444'; // Red

                        return (
                            <motion.div
                                key={seg.id}
                                layoutId={`seg-${seg.id}`}
                                initial={{ opacity: 0, scaleY: 0 }}
                                animate={{
                                    opacity: 1,
                                    scaleY: 1,
                                    left: `${leftPercent}%`,
                                    width: `${widthPercent}%`,
                                    backgroundColor: 'rgba(255, 255, 255, 0.05)', // base track color
                                }}
                                exit={{ opacity: 0, scaleY: 0 }}
                                transition={{ type: 'spring', stiffness: 300, damping: 30 }}
                                className="thread-segment"
                                title={`Segment ${seg.id}\nRange: ${seg.start_byte} - ${seg.end_byte}\nSpeed: ${(seg.speed_bps / 1024 / 1024).toFixed(2)} MB/s`}
                                style={{
                                    position: 'absolute',
                                    top: 0,
                                    bottom: 0,
                                    borderRight: '1px solid rgba(0,0,0,0.5)',
                                    boxSizing: 'border-box',
                                }}
                            >
                                {/* Progress Fill */}
                                <motion.div
                                    className="segment-progress-fill"
                                    animate={{
                                        width: `${progressPercent}%`,
                                        backgroundColor: color,
                                        boxShadow: glow
                                    }}
                                    transition={{ type: 'tween', ease: 'linear', duration: 0.2 }}
                                    style={{
                                        height: '100%',
                                        position: 'absolute',
                                        left: 0,
                                        top: 0
                                    }}
                                />

                                {/* Speed Overlay (only if wide enough) */}
                                {seg.state === 'Downloading' && widthPercent > 10 && (
                                    <motion.div
                                        initial={{ opacity: 0 }}
                                        animate={{ opacity: 1 }}
                                        className="segment-speed-overlay"
                                        style={{
                                            position: 'absolute',
                                            top: '50%',
                                            left: '50%',
                                            x: '-50%',
                                            y: '-50%',
                                            fontSize: '9px',
                                            color: '#fff',
                                            textShadow: '0 1px 2px black',
                                            pointerEvents: 'none',
                                            fontWeight: 'bold',
                                            whiteSpace: 'nowrap',
                                            zIndex: 10
                                        }}
                                    >
                                        {(seg.speed_bps / 1024).toFixed(0)} KB/s
                                    </motion.div>
                                )}
                            </motion.div>
                        );
                    })}
                </AnimatePresence>
            </div>
            <div className="thread-legend">
                <div className="legend-item"><span style={{ background: '#3b82f6' }}></span> Downloading</div>
                <div className="legend-item"><span style={{ background: '#10b981' }}></span> Complete</div>
                <div className="legend-item"><span style={{ background: '#64748b' }}></span> Idle</div>
            </div>

            <style>{`
                .thread-visualizer {
                    padding: 10px;
                    background: rgba(0, 0, 0, 0.3);
                    border-radius: 8px;
                    border: 1px solid rgba(255, 255, 255, 0.05);
                    margin-top: 8px;
                }
                .thread-bar-container {
                    position: relative;
                    height: 28px;
                    background: rgba(0, 0, 0, 0.5);
                    border-radius: 4px;
                    overflow: hidden;
                    width: 100%;
                }
                .thread-legend {
                    display: flex;
                    gap: 15px;
                    margin-top: 8px;
                    font-size: 0.7rem;
                    color: #94a3b8;
                    justify-content: flex-end;
                }
                .legend-item {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                }
                .legend-item span {
                    width: 8px;
                    height: 8px;
                    border-radius: 50%;
                    display: inline-block;
                }
            `}</style>
        </div>
    );
});
