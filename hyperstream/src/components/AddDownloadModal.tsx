import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Download, X, File, Link as LinkIcon, AlertCircle } from 'lucide-react';

interface AddDownloadModalProps {
    isOpen: boolean;
    onClose: () => void;
    onStart: (url: string, filename: string, force?: boolean) => void;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({ isOpen, onClose, onStart }) => {
    const [url, setUrl] = useState('');
    const [filename, setFilename] = useState('');
    const [isForceMode, setIsForceMode] = useState(false);

    // Auto-extract filename
    React.useEffect(() => {
        if (url && !filename) {
            try {
                const parts = url.split('/');
                const last = parts[parts.length - 1].split('?')[0];
                if (last && last.includes('.')) {
                    setFilename(last);
                }
            } catch (e) { /* ignore */ }
        }
    }, [url]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (url && filename) {
            onStart(url, filename, isForceMode);
            setUrl('');
            setFilename('');
            setIsForceMode(false);
            onClose();
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
                    {/* Backdrop */}
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        onClick={onClose}
                        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                    />

                    {/* Modal */}
                    <motion.div
                        initial={{ scale: 0.95, opacity: 0, y: 10 }}
                        animate={{ scale: 1, opacity: 1, y: 0 }}
                        exit={{ scale: 0.95, opacity: 0, y: 10 }}
                        className="relative w-full max-w-md bg-slate-900/90 border border-slate-700/50 rounded-xl shadow-2xl p-6 overflow-hidden"
                    >
                        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-blue-500 to-violet-500" />

                        <div className="flex justify-between items-center mb-6">
                            <h2 className="text-xl font-bold bg-gradient-to-r from-white to-slate-400 bg-clip-text text-transparent flex items-center gap-2">
                                <Download className="text-blue-500" size={24} />
                                Add Download
                            </h2>
                            <button onClick={onClose} className="text-slate-400 hover:text-white transition-colors">
                                <X size={20} />
                            </button>
                        </div>

                        <form onSubmit={handleSubmit} className="space-y-4">
                            <div className="space-y-1">
                                <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">Download URL</label>
                                <div className="relative group">
                                    <LinkIcon className="absolute left-3 top-3 text-slate-500 group-focus-within:text-blue-500 transition-colors" size={18} />
                                    <input
                                        type="text"
                                        value={url}
                                        onChange={(e) => setUrl(e.target.value)}
                                        placeholder="https://example.com/file.zip"
                                        autoFocus
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/50 transition-all font-mono text-sm"
                                    />
                                </div>
                            </div>

                            <div className="space-y-1">
                                <label className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">Filename</label>
                                <div className="relative group">
                                    <File className="absolute left-3 top-3 text-slate-500 group-focus-within:text-violet-500 transition-colors" size={18} />
                                    <input
                                        type="text"
                                        value={filename}
                                        onChange={(e) => setFilename(e.target.value)}
                                        placeholder="file.zip"
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-violet-500/50 focus:ring-1 focus:ring-violet-500/50 transition-all font-medium text-sm"
                                    />
                                </div>
                            </div>

                            {/* Force Download Toggle (Shift Key visualizer) */}
                            <div
                                className={`flex items-center gap-3 p-3 rounded-lg border transition-all cursor-pointer ${isForceMode ? 'bg-amber-900/20 border-amber-500/30' : 'bg-slate-800/30 border-transparent hover:bg-slate-800/50'}`}
                                onClick={() => setIsForceMode(!isForceMode)}
                            >
                                <div className={`w-4 h-4 rounded border flex items-center justify-center transition-all ${isForceMode ? 'bg-amber-500 border-amber-500' : 'border-slate-600'}`}>
                                    {isForceMode && <div className="w-2 h-2 bg-white rounded-sm" />}
                                </div>
                                <div className="flex-1">
                                    <p className={`text-sm font-medium ${isForceMode ? 'text-amber-400' : 'text-slate-400'}`}>Force Download Mode</p>
                                    <p className="text-xs text-slate-500">Bypasses pre-checks. Use for problematic links.</p>
                                </div>
                                {isForceMode && <AlertCircle size={16} className="text-amber-500" />}
                            </div>

                            <div className="flex gap-3 mt-6 pt-2">
                                <button
                                    type="button"
                                    onClick={onClose}
                                    className="flex-1 py-2.5 rounded-lg border border-slate-700 text-slate-400 font-medium hover:bg-slate-800 transition-all text-sm"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    className={`flex-1 py-2.5 rounded-lg font-bold text-white shadow-lg transition-all flex items-center justify-center gap-2 text-sm
                                        ${isForceMode
                                            ? 'bg-gradient-to-r from-amber-600 to-orange-600 shadow-amber-900/20 hover:shadow-amber-900/40'
                                            : 'bg-gradient-to-r from-blue-600 to-violet-600 shadow-blue-900/20 hover:shadow-blue-900/40'
                                        }
                                    `}
                                >
                                    {isForceMode ? 'Force Start' : 'Start Download'}
                                </button>
                            </div>

                            <p className="text-center text-xs text-slate-600">
                                Tip: Hold <kbd className="font-mono bg-slate-800 px-1 rounded text-slate-500">Shift</kbd> while clicking Start to force.
                            </p>
                        </form>
                    </motion.div>
                </div>
            )}
        </AnimatePresence>
    );
};
