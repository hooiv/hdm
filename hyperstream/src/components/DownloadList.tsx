import React, { useMemo } from 'react';
import { DownloadItem } from './DownloadItem';
import type { DownloadTask } from '../types';
import { Virtuoso } from 'react-virtuoso';
import { Inbox } from 'lucide-react';
import { motion } from 'framer-motion';

interface DownloadListProps {
    tasks: DownloadTask[];
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
    downloadDir: string;
}

export const DownloadList: React.FC<DownloadListProps> = ({ tasks, onPause, onResume, onDelete, onMoveUp, onMoveDown, downloadDir }) => {

    // Item renderer
    const itemContent = useMemo(() => (_index: number, task: DownloadTask) => {
        return (
            <div style={{ paddingBottom: '8px', paddingLeft: '5px', paddingRight: '5px' }}>
                <DownloadItem
                    task={task}
                    onPause={onPause}
                    onResume={onResume}
                    onDelete={onDelete}
                    onMoveUp={onMoveUp}
                    onMoveDown={onMoveDown}
                    downloadDir={downloadDir}
                />
            </div>
        );
    }, [onPause, onResume, onDelete, onMoveUp, onMoveDown, downloadDir]);

    if (tasks.length === 0) {
        return (
            <motion.div
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                className="flex flex-col items-center justify-center h-full text-slate-500 opacity-60"
            >
                <div className="p-6 bg-slate-800/30 rounded-full mb-4 border border-slate-700/30">
                    <Inbox size={48} className="text-slate-400" />
                </div>
                <h3 className="text-lg font-semibold text-slate-300">No Downloads Yet</h3>
                <p className="text-sm max-w-xs text-center mt-2">
                    Click the "Add Download" button to start downloading files.
                </p>
            </motion.div>
        );
    }

    return (
        <Virtuoso
            style={{ height: '100%', width: '100%' }}
            data={tasks}
            itemContent={itemContent}
            computeItemKey={(_, task) => task.id}
            alignToBottom={false}
            overscan={200}
        />
    );
};
