import React from 'react';
import { motion } from 'framer-motion';
import { Folder, Play, Pause, Trash2, ChevronDown, ArrowUp, ArrowDown, Share2 } from 'lucide-react';
import type { DownloadTask } from '../types';

interface DownloadItemActionsProps {
    task: DownloadTask;
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    isExpanded: boolean;
    onToggleExpand: () => void;
    onShowP2PShare: () => void;
    onOpenFolder: () => void;
}

export const DownloadItemActions: React.FC<DownloadItemActionsProps> = ({
    task,
    onPause,
    onResume,
    onDelete,
    onMoveUp,
    onMoveDown,
    isExpanded,
    onToggleExpand,
    onShowP2PShare,
    onOpenFolder,
}) => {
    return (
        <div className="ml-4 flex items-center gap-1 bg-black/20 p-1 rounded-lg border border-white/5" onClick={(e) => e.stopPropagation()}>
            {onMoveUp && (
                <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-slate-200 rounded-md transition-colors" onClick={() => onMoveUp(task.id)} title="Move Up">
                    <ArrowUp size={14} />
                </motion.button>
            )}
            {onMoveDown && (
                <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-slate-200 rounded-md transition-colors" onClick={() => onMoveDown(task.id)} title="Move Down">
                    <ArrowDown size={14} />
                </motion.button>
            )}

            <div className="w-px h-4 bg-white/10 mx-1"></div>

            <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-400 hover:text-cyan-400 rounded-md transition-colors" onClick={onOpenFolder} title="Open Folder" aria-label="Open folder">
                <Folder size={16} />
            </motion.button>

            {task.status === 'Downloading' && (
                <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-amber-400 hover:text-amber-300 rounded-md transition-colors" onClick={() => onPause(task.id)} title="Pause" aria-label="Pause download">
                    <Pause size={16} />
                </motion.button>
            )}

            {(task.status === 'Paused' || task.status === 'Error') && (
                <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(255,255,255,0.1)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-emerald-400 hover:text-emerald-300 rounded-md transition-colors" onClick={() => onResume(task.id)} title="Resume" aria-label="Resume download">
                    <Play size={16} />
                </motion.button>
            )}

            <motion.button whileHover={{ scale: 1.1, backgroundColor: "rgba(220,38,38,0.2)" }} whileTap={{ scale: 0.9 }} className="p-1.5 text-slate-500 hover:text-red-400 rounded-md transition-colors" onClick={() => onDelete && window.confirm(`Delete "${task.filename}"?`) && onDelete(task.id)} title="Cancel" aria-label="Delete download">
                <Trash2 size={16} />
            </motion.button>

            {/* P2P Share Button */}
            {(task.status === 'Done' || task.status === 'Downloading') && (
                <motion.button
                    whileHover={{ scale: 1.1, backgroundColor: "rgba(6,182,212,0.2)" }}
                    whileTap={{ scale: 0.9 }}
                    className="p-1.5 text-slate-500 hover:text-cyan-400 rounded-md transition-colors"
                    onClick={onShowP2PShare}
                    title="Share via P2P"
                    aria-label="Share via P2P"
                >
                    <Share2 size={16} />
                </motion.button>
            )}

            <div className="w-px h-4 bg-white/10 mx-1"></div>

            <motion.div
                animate={{ rotate: isExpanded ? 180 : 0 }}
                className="p-1 text-slate-500"
                onClick={onToggleExpand}
            >
                <ChevronDown size={16} />
            </motion.div>
        </div>
    );
};
