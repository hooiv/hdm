import React, { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { debug } from '../utils/logger';

interface DropTargetProps {
    onDrop: (url: string) => void;
}

export const DropTarget: React.FC<DropTargetProps> = ({ onDrop }) => {
    const [isDragging, setIsDragging] = useState(false);
    const onDropRef = useRef(onDrop);
    useEffect(() => { onDropRef.current = onDrop; }, [onDrop]);

    useEffect(() => {
        // Listen for Tauri file-drop events and forward to handler
        const unlisten = listen<string[]>('tauri://file-drop', (event) => {
            const files = event.payload;
            if (files && files.length > 0) {
                debug('Dropped files:', files);
                files.forEach(file => {
                    onDropRef.current(file);
                });
            }
        });

        return () => {
            unlisten.then(f => f());
        };
    }, []);

    const handleDragOver = (e: React.DragEvent) => {
        e.preventDefault();
        setIsDragging(true);
    };

    const handleDragLeave = () => {
        setIsDragging(false);
    };

    const handleDrop = (e: React.DragEvent) => {
        e.preventDefault();
        setIsDragging(false);

        const url = e.dataTransfer.getData('text/plain');
        if (url && (url.startsWith('http') || url.startsWith('magnet'))) {
            onDrop(url);
        } else if (e.dataTransfer.files.length > 0) {
            // Handle local file drop if needed
        }
    };

    return (
        <div
            className={`drop-target ${isDragging ? 'dragging' : ''}`}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
        >
            <div className="drop-icon">⬇️</div>
            {isDragging && <div className="drop-hint">Drop Link Here</div>}
        </div>
    );
};
