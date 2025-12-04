import React from 'react';
import { DownloadItem, DownloadTask } from './DownloadItem';

interface DownloadListProps {
    tasks: DownloadTask[];
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
}

export const DownloadList: React.FC<DownloadListProps> = ({ tasks, onPause, onResume, onDelete }) => {
    if (tasks.length === 0) {
        return (
            <div className="empty-state">
                <p>No downloads yet. Click "+ Add Url" to start.</p>
            </div>
        );
    }

    return (
        <div className="download-list">
            {tasks.map(task => (
                <DownloadItem key={task.id} task={task} onPause={onPause} onResume={onResume} onDelete={onDelete} />
            ))}
        </div>
    );
};
