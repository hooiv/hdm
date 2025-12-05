import React, { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

interface DropTargetProps {
    onDrop: (url: string) => void;
}

export const DropTarget: React.FC<DropTargetProps> = ({ onDrop }) => {
    const [isDragging, setIsDragging] = useState(false);
    const [isVisible, setIsVisible] = useState(false);

    useEffect(() => {
        // Listen for global drag events (if Tauri supports it, or use window events)
        const unlisten = listen('tauri://file-drop', (event: any) => {
            const files = event.payload as string[];
            if (files && files.length > 0) {
                // Handle file drop (e.g., .torrent or .meta)
                console.log('Dropped files:', files);
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

    // Toggle visibility with a keyboard shortcut or menu option
    // For now, we'll make it a small floating icon that expands

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
