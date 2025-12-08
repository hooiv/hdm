import React from 'react';
import { DownloadItem, DownloadTask } from './DownloadItem';
import { Virtuoso } from 'react-virtuoso';

interface DownloadListProps {
    tasks: DownloadTask[];
    onPause: (id: string) => void;
    onResume: (id: string) => void;
    onDelete?: (id: string) => void;
    onMoveUp?: (id: string) => void;
    onMoveDown?: (id: string) => void;
}

export const DownloadList: React.FC<DownloadListProps> = ({ tasks, onPause, onResume, onDelete, onMoveUp, onMoveDown }) => {
    if (tasks.length === 0) {
        return (
            <div className="empty-state">
                <p>No downloads yet. Click "+ Add Url" to start.</p>
            </div>
        );
    }

    return (
        <div className="download-list" style={{ height: '100%', width: '100%', flex: 1 }}>
            <Virtuoso
                style={{ height: '100%', width: '100%' }}
                data={tasks}
                totalCount={tasks.length}
                atBottomThreshold={50}
                followOutput={'auto'}
                itemContent={(_index, task) => (
                    <div style={{ paddingBottom: '8px' }}>
                        <DownloadItem
                            key={task.id}
                            task={task}
                            onPause={onPause}
                            onResume={onResume}
                            onDelete={onDelete}
                            onMoveUp={onMoveUp}
                            onMoveDown={onMoveDown}
                        />
                    </div>
                )}
            />
        </div>
    );
};
