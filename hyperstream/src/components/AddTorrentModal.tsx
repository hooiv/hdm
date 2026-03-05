import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
    Magnet, X, AlertCircle, FileDown, FolderOpen,
    UploadCloud, Loader2, CheckCircle2, PauseCircle,
} from 'lucide-react';
import type { AddTorrentResult } from '../types';
import { error as logError } from '../utils/logger';

// ── helpers ────────────────────────────────────────────────────────────────

const MAX_TORRENT_FILE_SIZE_BYTES = 8 * 1024 * 1024;

function isValidTorrentSource(input: string): boolean {
    const trimmed = input.trim();
    if (!trimmed) return false;

    // Support raw BTv1 info hash input for power users.
    if (/^[a-fA-F0-9]{40}$/.test(trimmed)) {
        return true;
    }

    if (trimmed.startsWith('magnet:')) {
        try {
            const url = new URL(trimmed);
            if (url.protocol !== 'magnet:') return false;
            return url.searchParams
                .getAll('xt')
                .some(v => v.toLowerCase().startsWith('urn:btih:'));
        } catch {
            return false;
        }
    }

    try {
        const url = new URL(trimmed);
        return url.protocol === 'http:' || url.protocol === 'https:';
    } catch {
        return false;
    }
}

function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
            const result = reader.result;
            if (typeof result !== 'string') {
                reject(new Error('Unexpected file read result'));
                return;
            }
            const commaIdx = result.indexOf(',');
            resolve(commaIdx >= 0 ? result.slice(commaIdx + 1) : result);
        };
        reader.onerror = () => reject(reader.error ?? new Error('Failed to read file'));
        reader.readAsDataURL(file);
    });
}

function formatUnknownError(err: unknown): string {
    if (typeof err === 'string') return err;
    if (err instanceof Error) return err.message;
    if (typeof err === 'object' && err !== null) {
        const maybeMessage = (err as { message?: unknown }).message;
        if (typeof maybeMessage === 'string') return maybeMessage;
        try {
            return JSON.stringify(err);
        } catch {
            return String(err);
        }
    }
    return String(err);
}

function formatWarningMessage(warnings: string[]): string {
    if (warnings.length === 0) return '';
    if (warnings.length === 1) return warnings[0];
    return `${warnings[0]} (+${warnings.length - 1} more)`;
}

// ── props ──────────────────────────────────────────────────────────────────

export interface AddTorrentModalProps {
    isOpen: boolean;
    onClose: () => void;
    /** Called when a magnet/HTTP URL or raw BTv1 info-hash is submitted. */
    onAdd: (
        magnet: string,
        savePath: string,
        paused: boolean,
        initialPriority: 'high' | 'normal' | 'low',
        pinned: boolean,
    ) => Promise<AddTorrentResult>;
}

// ── component ──────────────────────────────────────────────────────────────

type Mode = 'magnet' | 'file';
type PriorityOption = 'high' | 'normal' | 'low';

export const AddTorrentModal: React.FC<AddTorrentModalProps> = ({ isOpen, onClose, onAdd }) => {
    const [mode, setMode] = useState<Mode>('magnet');
    const [magnetLink, setMagnetLink] = useState('');
    const [savePath, setSavePath] = useState('');
    const [startPaused, setStartPaused] = useState(false);
    const [initialPriority, setInitialPriority] = useState<PriorityOption>('normal');
    const [pinTorrent, setPinTorrent] = useState(false);
    const [validationError, setValidationError] = useState('');
    const [validationWarning, setValidationWarning] = useState('');
    const [isDragOver, setIsDragOver] = useState(false);
    const [selectedFile, setSelectedFile] = useState<File | null>(null);
    const [isAdding, setIsAdding] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);

    // Reset on open
    useEffect(() => {
        if (isOpen) {
            setMode('magnet');
            setMagnetLink('');
            setSavePath('');
            setStartPaused(false);
            setInitialPriority('normal');
            setPinTorrent(false);
            setValidationError('');
            setValidationWarning('');
            setSelectedFile(null);
            setIsAdding(false);
        }
    }, [isOpen]);

    // Escape key
    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); onClose(); } };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, onClose]);

    // ── file handlers
    const processFile = useCallback((file: File) => {
        if (!file.name.toLowerCase().endsWith('.torrent')) {
            setValidationError('Only .torrent files are supported');
            setValidationWarning('');
            return;
        }
        if (file.size > MAX_TORRENT_FILE_SIZE_BYTES) {
            setValidationError('Torrent file is too large (max 8 MB)');
            setValidationWarning('');
            return;
        }
        setSelectedFile(file);
        setValidationError('');
        setValidationWarning('');
        setMode('file');
    }, []);

    const handleDrop = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        setIsDragOver(false);
        const file = e.dataTransfer.files[0];
        if (file) processFile(file);
    }, [processFile]);

    const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setIsDragOver(true); };
    const handleDragLeave = () => setIsDragOver(false);

    const handleFileInput = (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (file) processFile(file);
        // Reset input so same file can be re-selected
        if (fileInputRef.current) fileInputRef.current.value = '';
    };

    // ── submit
    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setValidationError('');
        setValidationWarning('');

        if (mode === 'file') {
            if (!selectedFile) {
                setValidationError('No .torrent file selected');
                return;
            }
            setIsAdding(true);
            try {
                const base64Data = await fileToBase64(selectedFile);
                const result = await invoke<AddTorrentResult>('add_torrent_file', {
                    base64Data,
                    savePath: savePath.trim() || null,
                    paused: startPaused,
                    onlyFiles: null,
                    initialPriority: initialPriority === 'normal' ? null : initialPriority,
                    pinned: pinTorrent ? true : null,
                });
                if (result.warnings.length > 0) {
                    setSelectedFile(null);
                    setValidationWarning(`Torrent added with warning: ${formatWarningMessage(result.warnings)}`);
                    return;
                }
                onClose();
            } catch (err) {
                logError('add_torrent_file failed', err);
                const msg = formatUnknownError(err);
                setValidationError(`Failed to add torrent: ${msg}`);
            } finally {
                setIsAdding(false);
            }
        } else {
            const trimmed = magnetLink.trim();
            if (!trimmed) {
                setValidationError('Please enter a magnet link or torrent URL');
                return;
            }
            if (!isValidTorrentSource(trimmed)) {
                setValidationError('Must be a magnet link with BTIH, HTTP(S) URL, or a 40-char info hash');
                return;
            }
            setIsAdding(true);
            try {
                const result = await onAdd(trimmed, savePath.trim(), startPaused, initialPriority, pinTorrent);
                if (result.warnings.length > 0) {
                    setMagnetLink('');
                    setValidationWarning(`Torrent added with warning: ${formatWarningMessage(result.warnings)}`);
                    return;
                }
                onClose();
            } catch (err) {
                logError('add_magnet_link failed', err);
                const msg = formatUnknownError(err);
                setValidationError(`Failed to add torrent: ${msg}`);
            } finally {
                setIsAdding(false);
            }
        }
    };

    if (!isOpen) return null;

    const canSubmit = (mode === 'file' ? !!selectedFile : !!magnetLink.trim()) && !isAdding;

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
                        className="relative w-full max-w-md bg-slate-900/95 border border-slate-700/50 rounded-xl shadow-2xl p-6 overflow-hidden"
                        onDrop={handleDrop}
                        onDragOver={handleDragOver}
                        onDragLeave={handleDragLeave}
                    >
                        {/* Top gradient accent */}
                        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-orange-500 to-red-500" />

                        {/* Drop overlay */}
                        <AnimatePresence>
                            {isDragOver && (
                                <motion.div
                                    initial={{ opacity: 0 }}
                                    animate={{ opacity: 1 }}
                                    exit={{ opacity: 0 }}
                                    className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-slate-900/90 rounded-xl border-2 border-dashed border-orange-500/60"
                                >
                                    <UploadCloud size={40} className="text-orange-400 mb-2" />
                                    <p className="text-orange-300 font-semibold">Drop .torrent file here</p>
                                </motion.div>
                            )}
                        </AnimatePresence>

                        {/* Header */}
                        <div className="flex justify-between items-center mb-5">
                            <h2 id="torrent-modal-title" className="text-xl font-bold text-white flex items-center gap-2">
                                <Magnet className="text-orange-500" size={22} />
                                Add Torrent
                            </h2>
                            <button onClick={onClose} aria-label="Close" className="text-slate-400 hover:text-white transition-colors">
                                <X size={20} />
                            </button>
                        </div>

                        {/* Mode tabs */}
                        <div className="flex gap-1 mb-5 p-1 bg-slate-800/60 rounded-lg">
                            {(['magnet', 'file'] as Mode[]).map(m => (
                                <button
                                    key={m}
                                    type="button"
                                    onClick={() => { setMode(m); setValidationError(''); setValidationWarning(''); }}
                                    className={`flex-1 py-2 text-sm font-semibold rounded-md transition-all flex items-center justify-center gap-2 ${
                                        mode === m
                                            ? 'bg-slate-700 text-white shadow'
                                            : 'text-slate-500 hover:text-slate-300'
                                    }`}
                                >
                                    {m === 'magnet' ? <Magnet size={15} /> : <FileDown size={15} />}
                                    {m === 'magnet' ? 'Magnet / URL' : '.torrent File'}
                                </button>
                            ))}
                        </div>

                        <form onSubmit={handleSubmit} className="space-y-4">
                            {/* Magnet input */}
                            {mode === 'magnet' && (
                                <div className="space-y-1">
                                    <label htmlFor="magnet-input" className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                        Magnet Link / Torrent URL
                                    </label>
                                    <div className="relative group">
                                        <Magnet className="absolute left-3 top-3 text-slate-500 group-focus-within:text-orange-500 transition-colors" size={16} />
                                        <input
                                            id="magnet-input"
                                            type="text"
                                            value={magnetLink}
                                            onChange={e => { setMagnetLink(e.target.value); setValidationError(''); setValidationWarning(''); }}
                                            placeholder="magnet:?xt=urn:btih:…"
                                            autoFocus
                                            aria-required="true"
                                            className="w-full bg-slate-800/60 border border-slate-700 rounded-lg py-2.5 pl-10 pr-3 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-orange-500/60 focus:ring-1 focus:ring-orange-500/30 transition-all font-mono text-sm"
                                        />
                                    </div>
                                </div>
                            )}

                            {/* File picker */}
                            {mode === 'file' && (
                                <div>
                                    <input
                                        ref={fileInputRef}
                                        type="file"
                                        accept=".torrent"
                                        onChange={handleFileInput}
                                        className="hidden"
                                        id="torrent-file-input"
                                    />
                                    {selectedFile ? (
                                        <div className="flex items-center gap-3 bg-emerald-900/20 border border-emerald-700/30 rounded-lg px-4 py-3">
                                            <CheckCircle2 size={18} className="text-emerald-400 shrink-0" />
                                            <div className="flex-1 min-w-0">
                                                <p className="text-sm text-slate-200 truncate font-medium">{selectedFile.name}</p>
                                                <p className="text-xs text-slate-500 mt-0.5">
                                                    {(selectedFile.size / 1024).toFixed(1)} KB
                                                </p>
                                            </div>
                                            <button
                                                type="button"
                                                onClick={() => { setSelectedFile(null); setValidationError(''); setValidationWarning(''); }}
                                                className="text-slate-500 hover:text-red-400 transition-colors shrink-0"
                                            >
                                                <X size={16} />
                                            </button>
                                        </div>
                                    ) : (
                                        <label
                                            htmlFor="torrent-file-input"
                                            className="flex flex-col items-center justify-center gap-2 py-8 border-2 border-dashed border-slate-700 hover:border-orange-500/50 rounded-lg cursor-pointer transition-colors group"
                                        >
                                            <UploadCloud size={28} className="text-slate-600 group-hover:text-orange-500 transition-colors" />
                                            <p className="text-sm text-slate-400">
                                                <span className="text-orange-400 font-semibold">Browse</span> or drop a .torrent file
                                            </p>
                                        </label>
                                    )}
                                </div>
                            )}

                            {/* Save path */}
                            <div className="space-y-1">
                                <label htmlFor="save-path" className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                    Save Path <span className="normal-case font-normal text-slate-600">(optional)</span>
                                </label>
                                <div className="relative group">
                                    <FolderOpen className="absolute left-3 top-3 text-slate-500 group-focus-within:text-orange-500 transition-colors" size={15} />
                                    <input
                                        id="save-path"
                                        type="text"
                                        value={savePath}
                                        onChange={e => setSavePath(e.target.value)}
                                        placeholder="Default download directory"
                                        className="w-full bg-slate-800/60 border border-slate-700 rounded-lg py-2.5 pl-10 pr-3 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-orange-500/60 focus:ring-1 focus:ring-orange-500/30 transition-all text-sm"
                                    />
                                </div>
                            </div>

                            {/* Start paused toggle */}
                            <label className="flex items-center gap-3 cursor-pointer group" htmlFor="start-paused">
                                <div className={`relative w-10 h-5 rounded-full transition-colors ${startPaused ? 'bg-amber-600' : 'bg-slate-700'}`}>
                                    <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${startPaused ? 'translate-x-5' : 'translate-x-0.5'}`} />
                                </div>
                                <input
                                    id="start-paused"
                                    type="checkbox"
                                    className="hidden"
                                    checked={startPaused}
                                    onChange={e => setStartPaused(e.target.checked)}
                                />
                                <div className="flex items-center gap-1.5">
                                    <PauseCircle size={14} className={startPaused ? 'text-amber-400' : 'text-slate-600'} />
                                    <span className="text-sm text-slate-400 group-hover:text-slate-300 transition-colors">
                                        Start paused
                                    </span>
                                </div>
                            </label>

                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                                <div className="space-y-1">
                                    <label htmlFor="initial-priority" className="text-xs uppercase font-semibold text-slate-500 tracking-wider ml-1">
                                        Initial Priority
                                    </label>
                                    <select
                                        id="initial-priority"
                                        value={initialPriority}
                                        onChange={(e) => setInitialPriority(e.target.value as PriorityOption)}
                                        className="w-full bg-slate-800/60 border border-slate-700 rounded-lg py-2.5 px-3 text-slate-200 focus:outline-none focus:border-orange-500/60 focus:ring-1 focus:ring-orange-500/30 transition-all text-sm"
                                    >
                                        <option value="high">High</option>
                                        <option value="normal">Normal</option>
                                        <option value="low">Low</option>
                                    </select>
                                </div>

                                <label className="flex items-center gap-3 cursor-pointer group self-end pb-1" htmlFor="pin-torrent">
                                    <div className={`relative w-10 h-5 rounded-full transition-colors ${pinTorrent ? 'bg-cyan-600' : 'bg-slate-700'}`}>
                                        <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${pinTorrent ? 'translate-x-5' : 'translate-x-0.5'}`} />
                                    </div>
                                    <input
                                        id="pin-torrent"
                                        type="checkbox"
                                        className="hidden"
                                        checked={pinTorrent}
                                        onChange={e => setPinTorrent(e.target.checked)}
                                    />
                                    <span className="text-sm text-slate-400 group-hover:text-slate-300 transition-colors">
                                        Pin torrent after add
                                    </span>
                                </label>
                            </div>

                            {/* Error message */}
                            {validationError && (
                                <div className="flex items-center gap-2 text-xs text-red-400 bg-red-900/20 border border-red-800/30 rounded-lg px-3 py-2" role="alert">
                                    <AlertCircle size={14} className="shrink-0" />
                                    {validationError}
                                </div>
                            )}
                            {validationWarning && (
                                <div className="flex items-center gap-2 text-xs text-amber-300 bg-amber-900/20 border border-amber-700/30 rounded-lg px-3 py-2" role="status">
                                    <AlertCircle size={14} className="shrink-0" />
                                    {validationWarning}
                                </div>
                            )}

                            {/* Actions */}
                            <div className="flex gap-3 pt-1">
                                <button
                                    type="button"
                                    onClick={onClose}
                                    className="flex-1 py-2.5 rounded-lg border border-slate-700 text-slate-400 font-medium hover:bg-slate-800 transition-all text-sm"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    disabled={!canSubmit}
                                    className={`flex-1 py-2.5 rounded-lg font-bold text-white shadow-lg transition-all flex items-center justify-center gap-2 text-sm ${
                                        !canSubmit
                                            ? 'opacity-50 cursor-not-allowed bg-slate-700'
                                            : 'bg-gradient-to-r from-orange-600 to-red-600 shadow-orange-900/20 hover:from-orange-500 hover:to-red-500'
                                    }`}
                                >
                                    {isAdding ? (
                                        <><Loader2 size={15} className="animate-spin" /> Adding…</>
                                    ) : (
                                        'Add Torrent'
                                    )}
                                </button>
                            </div>
                        </form>
                    </motion.div>
                </div>
            )}
        </AnimatePresence>
    );
};
