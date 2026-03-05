import React, { useState, useEffect, useRef } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { debug } from '../utils/logger';
import { ArrowDownToLine } from 'lucide-react';

interface DropTargetProps {
    onDrop: (url: string) => void;
}

export const DropTarget: React.FC<DropTargetProps> = ({ onDrop }) => {
    const [isDragging, setIsDragging] = useState(false);
    const onDropRef = useRef(onDrop);
    const dragCounter = useRef(0);

    useEffect(() => { onDropRef.current = onDrop; }, [onDrop]);

    useEffect(() => {
        // Tauri v2 drag-drop API via webview
        let unlistenDrop: Promise<() => void> | null = null;
        try {
            const webview = getCurrentWebview();
            if (webview) {
                unlistenDrop = webview.onDragDropEvent((event) => {
                    const payload = event.payload as { type?: string; paths?: string[] };
                    switch (payload.type) {
                        case 'over':
                            setIsDragging(true);
                            break;
                        case 'drop': {
                            const paths = payload.paths;
                            if (paths && paths.length > 0) {
                                debug('Dropped files:', paths);
                                paths.forEach(file => {
                                    onDropRef.current(file);
                                });
                            }
                            setIsDragging(false);
                            dragCounter.current = 0;
                            break;
                        }
                        case 'leave':
                        case 'cancel':
                            setIsDragging(false);
                            dragCounter.current = 0;
                            break;
                        default:
                            break;
                    }
                });
            }
        } catch {
            // Not running inside Tauri webview — native drop not available
        }

        // Track window-level drag events to show overlay
        const onDragEnter = (e: DragEvent) => {
            e.preventDefault();
            dragCounter.current++;
            setIsDragging(true);
        };
        const onDragLeave = (e: DragEvent) => {
            e.preventDefault();
            dragCounter.current--;
            if (dragCounter.current <= 0) {
                dragCounter.current = 0;
                setIsDragging(false);
            }
        };
        const onDragOver = (e: DragEvent) => {
            e.preventDefault();
        };
        const onDropWindow = (e: DragEvent) => {
            e.preventDefault();
            setIsDragging(false);
            dragCounter.current = 0;

            const url = e.dataTransfer?.getData('text/plain');
            if (url && (url.startsWith('http') || url.startsWith('magnet'))) {
                onDropRef.current(url);
            }
        };

        window.addEventListener('dragenter', onDragEnter);
        window.addEventListener('dragleave', onDragLeave);
        window.addEventListener('dragover', onDragOver);
        window.addEventListener('drop', onDropWindow);

        return () => {
            if (unlistenDrop) unlistenDrop.then(fn => fn()).catch(() => {});
            window.removeEventListener('dragenter', onDragEnter);
            window.removeEventListener('dragleave', onDragLeave);
            window.removeEventListener('dragover', onDragOver);
            window.removeEventListener('drop', onDropWindow);
        };
    }, []);

    if (!isDragging) return null;

    return (
        <div className="fixed inset-0 z-[200] bg-black/60 backdrop-blur-sm flex items-center justify-center pointer-events-none">
            <div className="flex flex-col items-center gap-4">
                <div className="w-20 h-20 rounded-full bg-cyan-500/20 border-2 border-dashed border-cyan-400/60 flex items-center justify-center animate-pulse">
                    <ArrowDownToLine size={32} className="text-cyan-400" />
                </div>
                <span className="text-lg font-bold text-white">Drop to Download</span>
                <span className="text-sm text-slate-400">Drop a URL or file here</span>
            </div>
        </div>
    );
};
