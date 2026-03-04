import React, { useState, useEffect, useMemo } from 'react';

import { X, Minus, Square, Copy } from 'lucide-react';

// Safely get appWindow — getCurrentWindow() may fail outside Tauri webview
function getAppWindow() {
    try {
        const { getCurrentWindow } = require('@tauri-apps/api/window');
        return getCurrentWindow();
    } catch {
        return null;
    }
}

export const TitleBar: React.FC = () => {
    const [isMaximized, setIsMaximized] = useState(false);
    const appWindow = useMemo(() => getAppWindow(), []);

    useEffect(() => {
        if (!appWindow) return;
        const checkMaximized = async () => {
            setIsMaximized(await appWindow.isMaximized());
        };

        // Listen for resize events to update UI state if external force changes it
        const unlisten = appWindow.onResized(async () => {
            checkMaximized();
        });

        checkMaximized();

        return () => {
            unlisten.then(f => f());
        }
    }, []);

    const handleMinimize = () => appWindow?.minimize();
    const handleMaximize = async () => {
        if (!appWindow) return;
        if (await appWindow.isMaximized()) {
            appWindow.unmaximize();
            setIsMaximized(false);
        } else {
            appWindow.maximize();
            setIsMaximized(true);
        }
    };
    const handleClose = () => appWindow?.close();

    return (
        <div
            data-tauri-drag-region
            className="h-10 bg-slate-900/80 backdrop-blur-md border-b border-white/5 flex items-center justify-between px-4 select-none fixed top-0 left-0 right-0 z-[100]"
        >
            {/* Logo / Title Area */}
            <div className="flex items-center gap-2 pointer-events-none text-slate-400">
                <div className="w-5 h-5 bg-gradient-to-br from-blue-500 to-violet-600 rounded-md flex items-center justify-center text-[10px] font-bold text-white shadow-lg shadow-blue-500/20">
                    H
                </div>
                <span className="text-xs font-bold tracking-wider">HYPERSTREAM</span>
            </div>

            {/* Window Controls */}
            <div className="flex items-center gap-1">
                <button
                    onClick={handleMinimize}
                    className="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded-md transition-all"
                    title="Minimize"
                >
                    <Minus size={14} />
                </button>
                <button
                    onClick={handleMaximize}
                    className="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded-md transition-all"
                    title={isMaximized ? "Restore" : "Maximize"}
                >
                    {isMaximized ? <Copy size={14} className="rotate-180" /> : <Square size={14} />}
                </button>
                <button
                    onClick={handleClose}
                    className="p-1.5 text-slate-400 hover:text-white hover:bg-red-500/80 rounded-md transition-all group"
                    title="Close"
                >
                    <X size={14} className="group-hover:text-white" />
                </button>
            </div>
        </div>
    );
};
