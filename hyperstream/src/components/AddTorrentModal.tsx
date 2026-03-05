import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Magnet, X, AlertCircle } from 'lucide-react';

interface AddTorrentModalProps {
    isOpen: boolean;
    onClose: () => void;
    onAdd: (magnet: string) => void;
}

export const AddTorrentModal: React.FC<AddTorrentModalProps> = ({ isOpen, onClose, onAdd }) => {
    const [magnetLink, setMagnetLink] = useState('');
    const [validationError, setValidationError] = useState('');

    // Reset state when modal opens
    useEffect(() => {
        if (isOpen) {
            setMagnetLink('');
            setValidationError('');
        }
    }, [isOpen]);

    // Close on Escape key
    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                e.preventDefault();
                onClose();
            }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, onClose]);

    if (!isOpen) return null;

    const trimmed = magnetLink.trim();
    const isMagnet = trimmed.startsWith('magnet:?xt=urn:btih:');
    const isUrl = trimmed.startsWith('http://') || trimmed.startsWith('https://');
    const isValid = trimmed && (isMagnet || isUrl);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (!trimmed) {
            setValidationError('Please enter a magnet link or torrent URL');
            return;
        }
        if (!isValid) {
            setValidationError('Invalid format. Must be a magnet link (magnet:?xt=urn:btih:...) or HTTP(S) URL');
            return;
        }
        try {
            onAdd(trimmed);
            setMagnetLink('');
            onClose();
        } catch (err) {
            setValidationError(`Failed to add torrent: ${err}`);
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        onClick={onClose}
                        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                    />
                    <motion.div
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="torrent-modal-title"
                        initial={{ scale: 0.95, opacity: 0, y: 10 }}
                        animate={{ scale: 1, opacity: 1, y: 0 }}
                        exit={{ scale: 0.95, opacity: 0, y: 10 }}
                        className="relative w-full max-w-md bg-slate-900/90 border border-slate-700/50 rounded-xl shadow-2xl p-6 overflow-hidden"
                    >
                        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-orange-500 to-red-500" />

                        <div className="flex justify-between items-center mb-6">
                            <h2 id="torrent-modal-title" className="text-xl font-bold bg-gradient-to-r from-white to-slate-400 bg-clip-text text-transparent flex items-center gap-2">
                                <Magnet className="text-orange-500" size={24} />
                                Add Torrent
                            </h2>
                            <button
                                onClick={onClose}
                                aria-label="Close dialog"
                                className="text-slate-400 hover:text-white transition-colors"
                            >
                                <X size={20} />
                            </button>
                        </div>

                        <form onSubmit={handleSubmit} className="space-y-4">
                            <div className="space-y-1">
                                <label htmlFor="magnet-input" className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                    Magnet Link / Torrent URL
                                </label>
                                <div className="relative group">
                                    <Magnet
                                        className="absolute left-3 top-3 text-slate-500 group-focus-within:text-orange-500 transition-colors"
                                        size={18}
                                    />
                                    <input
                                        id="magnet-input"
                                        type="text"
                                        value={magnetLink}
                                        onChange={e => { setMagnetLink(e.target.value); setValidationError(''); }}
                                        placeholder="magnet:?xt=urn:btih:..."
                                        autoFocus
                                        aria-required="true"
                                        aria-describedby={validationError ? "magnet-error" : undefined}
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg py-2.5 pl-10 pr-4 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-orange-500/50 focus:ring-1 focus:ring-orange-500/50 transition-all font-mono text-sm"
                                    />
                                </div>
                            </div>

                            {validationError && (
                                <div id="magnet-error" className="flex items-center gap-2 text-xs text-red-400 bg-red-900/20 border border-red-800/30 rounded-lg px-3 py-2" role="alert">
                                    <AlertCircle size={14} />
                                    {validationError}
                                </div>
                            )}

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
                                    disabled={!trimmed}
                                    className={`flex-1 py-2.5 rounded-lg font-bold text-white shadow-lg transition-all flex items-center justify-center gap-2 text-sm ${
                                        !trimmed
                                            ? "opacity-50 cursor-not-allowed bg-slate-700"
                                            : "bg-gradient-to-r from-orange-600 to-red-600 shadow-orange-900/20 hover:shadow-orange-900/40"
                                    }`}
                                >
                                    Add Torrent
                                </button>
                            </div>
                        </form>
                    </motion.div>
                </div>
            )}
        </AnimatePresence>
    );
};
