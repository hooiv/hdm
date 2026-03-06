import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { debug, warn, error as logError } from '../utils/logger';
import { motion, AnimatePresence } from 'framer-motion';
import { Archive, X, File, Folder, AlertTriangle, Loader2, Search, Download, PackageOpen, Trash2, Check } from 'lucide-react';

interface ZipEntry {
    name: string;
    is_directory: boolean;
    compressed_size: number;
    uncompressed_size: number;
    compression_method: string;
}

interface ZipPreviewData {
    total_files: number;
    total_directories: number;
    total_compressed_size: number;
    total_uncompressed_size: number;
    entries: ZipEntry[];
}

interface ZipPreviewModalProps {
    filePath: string;
    url?: string;
    isOpen: boolean;
    onClose: () => void;
    isPartial?: boolean;
}

const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
};

export const ZipPreviewModal: React.FC<ZipPreviewModalProps> = ({ filePath, url, isOpen, onClose, isPartial = false }) => {
    const [data, setData] = useState<ZipPreviewData | null>(null);
    const [loading, setLoading] = useState(false);
    const [errMsg, setError] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');
    const [extracting, setExtracting] = useState<string | null>(null);
    const [extracted, setExtracted] = useState<Set<string>>(new Set());
    const [extractAllDone, setExtractAllDone] = useState(false);

    const filteredEntries = data?.entries.filter(e =>
        !searchQuery || e.name.toLowerCase().includes(searchQuery.toLowerCase())
    ) ?? [];

    const handleExtractSingle = async (entryName: string) => {
        const destDir = filePath.replace(/[/\\][^/\\]+$/, '');
        setExtracting(entryName);
        try {
            if (url && isPartial) {
                await invoke('download_zip_entry', { url, entryName, destPath: `${destDir}/${entryName.split('/').pop()}` });
            } else {
                await invoke('extract_single_file', { zipPath: filePath, entryName, destPath: `${destDir}/${entryName.split('/').pop()}` });
            }
            setExtracted(prev => new Set(prev).add(entryName));
        } catch (err) {
            logError('Failed to extract file:', err);
        } finally {
            setExtracting(null);
        }
    };

    const handleExtractAll = async () => {
        const destDir = filePath.replace(/[/\\][^/\\]+$/, '');
        setExtracting('__all__');
        try {
            const count = await invoke<number>('extract_zip_all', { zipPath: filePath, destDir });
            setExtractAllDone(true);
            debug(`Extracted ${count} files`);
        } catch (err) {
            logError('Failed to extract all:', err);
        } finally {
            setExtracting(null);
        }
    };

    const handleCleanup = async () => {
        if (!window.confirm('Delete the archive file after extraction?')) return;
        try {
            await invoke('cleanup_archive', { archivePath: filePath });
            onClose();
        } catch (err) {
            logError('Failed to cleanup archive:', err);
        }
    };

    const loadPreview = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            if (isPartial && url) {
                try {
                    debug("Attempting remote ZIP preview...");
                    const result = await invoke<ZipPreviewData>('preview_zip_remote', { url });
                    setData(result);
                    return;
                } catch (remoteErr) {
                    warn("Remote preview failed, falling back to local partial read:", remoteErr);
                }
            }

            if (isPartial) {
                const bytes = await invoke<number[]>('read_zip_last_bytes', { path: filePath, length: 65536 });
                const result = await invoke<ZipPreviewData>('preview_zip_partial', { data: bytes });
                setData(result);
            } else {
                const result = await invoke<ZipPreviewData>('preview_zip_file', { path: filePath });
                setData(result);
            }
        } catch (err) {
            logError('Failed to preview ZIP:', err);
            setError(typeof err === 'string' ? err : 'Failed to load preview');
        } finally {
            setLoading(false);
        }
    }, [filePath, url, isPartial]);

    useEffect(() => {
        if (isOpen) {
            loadPreview();
        }
    }, [isOpen, loadPreview]);

    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') { e.preventDefault(); onClose(); }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, onClose]);

    return (
        <AnimatePresence>
            {isOpen && (
            <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
                onClick={onClose}
                role="dialog"
                aria-modal="true"
            >
                <motion.div
                    initial={{ opacity: 0, scale: 0.95, y: 20 }}
                    animate={{ opacity: 1, scale: 1, y: 0 }}
                    exit={{ opacity: 0, scale: 0.95, y: 20 }}
                    transition={{ type: 'spring', stiffness: 300, damping: 25 }}
                    className="bg-slate-900/95 backdrop-blur-xl border border-white/10 rounded-2xl shadow-2xl w-full max-w-2xl mx-4 flex flex-col max-h-[80vh]"
                    onClick={e => e.stopPropagation()}
                >
                    {/* Header */}
                    <div className="flex items-center justify-between px-6 py-4 border-b border-white/5 flex-shrink-0">
                        <div className="flex items-center gap-3">
                            <div className="w-8 h-8 rounded-lg bg-amber-500/10 border border-amber-500/20 flex items-center justify-center">
                                <Archive size={16} className="text-amber-400" />
                            </div>
                            <div>
                                <h3 className="text-lg font-semibold text-white">ZIP Preview</h3>
                                {isPartial && (
                                    <span className="text-[10px] text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded border border-amber-500/20 font-medium">
                                        Partial
                                    </span>
                                )}
                            </div>
                        </div>
                        <button
                            onClick={onClose}
                            className="p-1.5 text-slate-500 hover:text-white hover:bg-white/10 rounded-lg transition-colors"
                        >
                            <X size={16} />
                        </button>
                    </div>

                    {/* Body */}
                    <div className="flex-1 overflow-y-auto custom-scrollbar p-6">
                        {loading && (
                            <div className="flex flex-col items-center justify-center py-16 text-slate-400">
                                <Loader2 size={28} className="animate-spin mb-3 text-cyan-400" />
                                <p className="text-sm">Loading archive structure...</p>
                            </div>
                        )}

                        {errMsg && (
                            <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-center">
                                <AlertTriangle size={24} className="mx-auto mb-2 text-red-400" />
                                <p className="text-sm text-red-400">{errMsg}</p>
                                {isPartial && (
                                    <p className="text-[11px] text-slate-500 mt-1">
                                        Partial preview requires the end of the file to be downloaded.
                                    </p>
                                )}
                            </div>
                        )}

                        {data && (
                            <div className="space-y-4">
                                {/* Stats */}
                                <div className="grid grid-cols-3 gap-3">
                                    <div className="bg-white/5 border border-white/5 rounded-xl p-3 text-center">
                                        <div className="text-lg font-bold text-white">{data.total_files}</div>
                                        <div className="text-[10px] text-slate-500 uppercase tracking-wider mt-0.5">Files</div>
                                    </div>
                                    <div className="bg-white/5 border border-white/5 rounded-xl p-3 text-center">
                                        <div className="text-lg font-bold text-white">{data.total_directories}</div>
                                        <div className="text-[10px] text-slate-500 uppercase tracking-wider mt-0.5">Folders</div>
                                    </div>
                                    <div className="bg-white/5 border border-white/5 rounded-xl p-3 text-center">
                                        <div className="text-lg font-bold text-white">{formatSize(data.total_uncompressed_size)}</div>
                                        <div className="text-[10px] text-slate-500 uppercase tracking-wider mt-0.5">Total Size</div>
                                    </div>
                                </div>

                                {/* Search & Actions */}
                                <div className="flex items-center gap-2">
                                    <div className="flex-1 relative">
                                        <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
                                        <input
                                            type="text"
                                            value={searchQuery}
                                            onChange={e => setSearchQuery(e.target.value)}
                                            placeholder="Search files..."
                                            className="w-full pl-9 pr-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50"
                                        />
                                    </div>
                                    {!isPartial && (
                                        <>
                                            <button
                                                onClick={handleExtractAll}
                                                disabled={extracting !== null || extractAllDone}
                                                className="px-3 py-1.5 rounded-lg text-xs font-medium text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 hover:bg-emerald-500/20 transition-colors disabled:opacity-40 flex items-center gap-1 whitespace-nowrap"
                                            >
                                                {extractAllDone ? <><Check size={12} /> Extracted</> : extracting === '__all__' ? <><Loader2 size={12} className="animate-spin" /> Extracting...</> : <><PackageOpen size={12} /> Extract All</>}
                                            </button>
                                            {extractAllDone && (
                                                <button
                                                    onClick={handleCleanup}
                                                    className="px-3 py-1.5 rounded-lg text-xs font-medium text-red-400 bg-red-500/10 border border-red-500/20 hover:bg-red-500/20 transition-colors flex items-center gap-1 whitespace-nowrap"
                                                >
                                                    <Trash2 size={12} /> Delete Archive
                                                </button>
                                            )}
                                        </>
                                    )}
                                </div>

                                {/* File Table */}
                                <div className="rounded-xl border border-white/5 overflow-hidden">
                                    <table className="w-full text-sm">
                                        <thead>
                                            <tr className="bg-white/5 text-slate-400 text-xs uppercase tracking-wider">
                                                <th className="text-left px-4 py-2.5 font-medium">Name</th>
                                                <th className="text-right px-4 py-2.5 font-medium w-24">Size</th>
                                                <th className="text-right px-4 py-2.5 font-medium w-28">Compressed</th>
                                                <th className="text-center px-2 py-2.5 font-medium w-10"></th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {filteredEntries.map((entry, idx) => (
                                                <tr
                                                    key={idx}
                                                    className="border-t border-white/[0.03] hover:bg-white/[0.03] transition-colors"
                                                >
                                                    <td className="px-4 py-2 text-slate-300">
                                                        <span className="flex items-center gap-2">
                                                            {entry.is_directory ? (
                                                                <Folder size={14} className="text-amber-400 flex-shrink-0" />
                                                            ) : (
                                                                <File size={14} className="text-slate-500 flex-shrink-0" />
                                                            )}
                                                            <span className="truncate">{entry.name}</span>
                                                        </span>
                                                    </td>
                                                    <td className="px-4 py-2 text-right text-slate-500 font-mono text-xs">
                                                        {formatSize(entry.uncompressed_size)}
                                                    </td>
                                                    <td className="px-4 py-2 text-right text-slate-500 font-mono text-xs">
                                                        {formatSize(entry.compressed_size)}
                                                    </td>
                                                    <td className="px-2 py-2 text-center">
                                                        {!entry.is_directory && (
                                                            extracted.has(entry.name) ? (
                                                                <Check size={12} className="text-emerald-400 mx-auto" />
                                                            ) : (
                                                                <button
                                                                    onClick={() => handleExtractSingle(entry.name)}
                                                                    disabled={extracting !== null}
                                                                    className="p-1 text-slate-500 hover:text-cyan-400 transition-colors disabled:opacity-30"
                                                                    title="Extract this file"
                                                                >
                                                                    {extracting === entry.name ? <Loader2 size={12} className="animate-spin" /> : <Download size={12} />}
                                                                </button>
                                                            )
                                                        )}
                                                    </td>
                                                </tr>
                                            ))}
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        )}
                    </div>
                </motion.div>
            </motion.div>
            )}
        </AnimatePresence>
    );
};
