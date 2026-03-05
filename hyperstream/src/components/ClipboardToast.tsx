import React from 'react';
import { motion } from 'framer-motion';
import { Download, X, Clipboard } from 'lucide-react';

interface ToastProps {
    message: string;
    filename: string;
    onDownload: () => void;
    onDismiss: () => void;
}

export const ClipboardToast: React.FC<ToastProps> = ({ message, filename, onDownload, onDismiss }) => {
    return (
        <motion.div
            initial={{ opacity: 0, y: 50, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 50, scale: 0.95 }}
            transition={{ type: 'spring', stiffness: 300, damping: 25 }}
            className="fixed bottom-6 right-6 z-50 flex items-center gap-3 bg-slate-900/90 backdrop-blur-xl border border-cyan-500/20 rounded-xl px-4 py-3 shadow-[0_0_30px_rgba(6,182,212,0.15)] max-w-md"
            role="alert"
            aria-live="polite"
        >
                <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center">
                    <Clipboard size={18} className="text-cyan-400" />
                </div>
                <div className="flex-1 min-w-0">
                    <div className="text-xs font-medium text-slate-300">{message}</div>
                    <div className="text-[11px] text-cyan-400 font-mono truncate mt-0.5">{filename}</div>
                </div>
                <div className="flex items-center gap-1.5 flex-shrink-0">
                    <motion.button
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.95 }}
                        className="flex items-center gap-1.5 px-3 py-1.5 bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 text-xs font-medium rounded-lg border border-cyan-500/20 transition-colors"
                        onClick={onDownload}
                    >
                        <Download size={12} />
                        Download
                    </motion.button>
                    <motion.button
                        whileHover={{ scale: 1.1 }}
                        whileTap={{ scale: 0.9 }}
                        className="p-1.5 text-slate-500 hover:text-white hover:bg-white/10 rounded-lg transition-colors"
                        onClick={onDismiss}
                    >
                        <X size={14} />
                    </motion.button>
                </div>
            </motion.div>
    );
};
