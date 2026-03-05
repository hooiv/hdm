import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { GrabbedFile } from '../types';
import { motion } from 'framer-motion';
import { Bug, X, Globe, CheckSquare, Square, Download, FileText, Image as ImageIcon } from 'lucide-react';

interface SpiderModalProps {
    isOpen: boolean;
    onClose: () => void;
    onDownload: (files: GrabbedFile[]) => void;
}

export const SpiderModal: React.FC<SpiderModalProps> = ({ isOpen, onClose, onDownload }) => {
    const [url, setUrl] = useState('');
    const [maxDepth, setMaxDepth] = useState(1);
    const [extensions, setExtensions] = useState({
        jpg: true,
        png: true,
        gif: false,
        mp4: true,
        mp3: false,
        zip: true,
        rar: false,
        pdf: false,
        exe: false,
        iso: false,
    });
    const [customExt, setCustomExt] = useState('');
    const [isCrawling, setIsCrawling] = useState(false);
    const [results, setResults] = useState<GrabbedFile[]>([]);
    const [selectedUrls, setSelectedUrls] = useState<Set<string>>(new Set());
    const [error, setError] = useState<string | null>(null);

    /** Basic URL validation — must be http(s) */
    const isValidUrl = /^https?:\/\/.+/.test(url.trim());

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

    const handleCrawl = async () => {
        setIsCrawling(true);
        setError(null);
        setResults([]);
        setSelectedUrls(new Set());

        const activeExtensions = Object.entries(extensions)
            .filter(([_, active]) => active)
            .map(([ext]) => ext);

        // Add custom extensions (comma-separated)
        if (customExt.trim()) {
            const custom = customExt.split(',').map(e => e.trim().replace(/^\./, '').toLowerCase()).filter(Boolean);
            activeExtensions.push(...custom);
        }

        try {
            const files = await invoke<GrabbedFile[]>('crawl_website', {
                url,
                maxDepth: Number(maxDepth),
                extensions: activeExtensions
            });
            setResults(files);
            // Auto-select all by default
            const allUrls = new Set(files.map(f => f.url));
            setSelectedUrls(allUrls);
        } catch (err: unknown) {
            setError(String(err));
        } finally {
            setIsCrawling(false);
        }
    };

    const toggleSelection = (fileUrl: string) => {
        const newSet = new Set(selectedUrls);
        if (newSet.has(fileUrl)) {
            newSet.delete(fileUrl);
        } else {
            newSet.add(fileUrl);
        }
        setSelectedUrls(newSet);
    };

    const handleDownloadSelected = () => {
        const selectedFiles = results.filter(f => selectedUrls.has(f.url));
        onDownload(selectedFiles);
        onClose();
    };

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4" role="dialog" aria-modal="true" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
            <motion.div
                className="w-full max-w-4xl h-[85vh] bg-slate-900 border border-slate-700/50 rounded-2xl shadow-2xl flex flex-col overflow-hidden"
                initial={{ scale: 0.95, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                exit={{ scale: 0.95, opacity: 0 }}
            >
                {/* Header */}
                <div className="flex items-center justify-between px-6 py-4 border-b border-slate-700/50 bg-slate-900/50 backdrop-blur-md">
                    <div className="flex items-center gap-3">
                        <div className="p-2 bg-red-500/10 rounded-lg text-red-500">
                            <Bug size={24} />
                        </div>
                        <h2 className="text-xl font-bold text-slate-200">Site Grabber</h2>
                    </div>
                    <button onClick={onClose} className="p-2 hover:bg-slate-800 rounded-lg transition-colors text-slate-400 hover:text-white">
                        <X size={24} />
                    </button>
                </div>

                {/* Body */}
                <div className="flex-1 flex flex-col p-6 overflow-hidden">

                    {/* Controls */}
                    <div className="grid gap-4 bg-slate-800/30 p-5 rounded-xl border border-slate-700/50 mb-6">
                        <div className="flex gap-4">
                            <div className="flex-1 relative">
                                <Globe className="absolute left-3 top-3 text-slate-500" size={18} />
                                <input
                                    type="text"
                                    value={url}
                                    onChange={e => setUrl(e.target.value)}
                                    placeholder="https://example.com/gallery"
                                    className={`w-full bg-slate-900 border rounded-lg pl-10 pr-4 py-2.5 text-slate-200 focus:outline-none transition-colors ${url.trim() && !isValidUrl ? 'border-red-500/50 focus:border-red-500' : 'border-slate-700 focus:border-red-500/50'}`}
                                />
                            </div>
                            <div className="w-24">
                                <input
                                    type="number"
                                    min="0" max="3"
                                    value={maxDepth}
                                    onChange={e => setMaxDepth(Number(e.target.value))}
                                    className="w-full bg-slate-900 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-red-500/50 text-center"
                                    title="Crawl Depth"
                                />
                            </div>
                        </div>

                        <div className="flex flex-wrap gap-3 items-center">
                            <span className="text-sm font-medium text-slate-500 mr-2">Extensions:</span>
                            {Object.keys(extensions).map(ext => (
                                <label key={ext} className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-700 cursor-pointer hover:border-slate-600 transition-colors">
                                    <input
                                        type="checkbox"
                                        checked={extensions[ext as keyof typeof extensions]}
                                        onChange={e => setExtensions(prev => ({ ...prev, [ext]: e.target.checked }))}
                                        className="rounded border-slate-600 text-red-500 focus:ring-offset-0 focus:ring-0 bg-slate-800"
                                    />
                                    <span className="text-xs font-bold text-slate-300 uppercase">{ext}</span>
                                </label>
                            ))}
                            <input
                                type="text"
                                value={customExt}
                                onChange={e => setCustomExt(e.target.value)}
                                placeholder="Custom (e.g. docx, 7z)"
                                className="px-3 py-1.5 bg-slate-900 border border-slate-700 rounded-lg text-xs text-slate-300 focus:outline-none focus:border-red-500/50 w-44 placeholder-slate-600"
                            />

                            <button
                                onClick={handleCrawl}
                                disabled={isCrawling || !url || !isValidUrl}
                                className={`ml-auto px-6 py-2 rounded-lg font-bold text-white transition-all shadow-lg ${isCrawling || !isValidUrl ? 'bg-slate-700 cursor-not-allowed' : 'bg-red-600 hover:bg-red-500 shadow-red-900/20'
                                    }`}
                            >
                                {isCrawling ? 'Crawling...' : 'Start Crawling'}
                            </button>
                        </div>
                    </div>

                    {/* Error */}
                    {error && (
                        <div className="p-4 bg-red-500/10 border border-red-500/20 text-red-400 rounded-xl mb-6 text-sm">
                            {error}
                        </div>
                    )}

                    {/* Results */}
                    {results.length > 0 && (
                        <div className="flex-1 flex flex-col min-h-0 bg-slate-800/20 rounded-xl border border-slate-700/50 overflow-hidden">
                            <div className="p-3 border-b border-slate-700/50 flex items-center justify-between bg-slate-900/30">
                                <span className="text-sm font-medium text-slate-300">Found {results.length} files</span>
                                <div className="flex gap-2">
                                    <button
                                        onClick={() => setSelectedUrls(new Set(results.map(f => f.url)))}
                                        className="px-3 py-1 text-xs font-medium text-slate-400 hover:text-white hover:bg-slate-700 rounded transition-colors"
                                    >
                                        Select All
                                    </button>
                                    <button
                                        onClick={() => setSelectedUrls(new Set())}
                                        className="px-3 py-1 text-xs font-medium text-slate-400 hover:text-white hover:bg-slate-700 rounded transition-colors"
                                    >
                                        Deselect All
                                    </button>
                                </div>
                            </div>

                            <div className="flex-1 overflow-y-auto p-2 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2 align-content-start custom-scrollbar">
                                {results.map((file) => {
                                    const isSelected = selectedUrls.has(file.url);
                                    return (
                                        <div
                                            key={file.url}
                                            onClick={() => toggleSelection(file.url)}
                                            className={`p-3 rounded-lg border cursor-pointer transition-all flex items-start gap-3 group ${isSelected
                                                ? 'bg-blue-500/10 border-blue-500/30'
                                                : 'bg-slate-900/40 border-slate-800 hover:border-slate-600'
                                                }`}
                                        >
                                            <div className={`p-2 rounded-lg ${isSelected ? 'bg-blue-500/20 text-blue-400' : 'bg-slate-800 text-slate-500'}`}>
                                                {file.file_type === 'image' ? <ImageIcon size={18} /> : <FileText size={18} />}
                                            </div>
                                            <div className="flex-1 min-w-0">
                                                <div className={`text-sm font-medium truncate mb-0.5 ${isSelected ? 'text-blue-300' : 'text-slate-300'}`}>
                                                    {file.filename}
                                                </div>
                                                <div className="text-xs text-slate-500 font-mono truncate opacity-70">{file.size ? `${(file.size / 1024).toFixed(1)} KB` : 'Unknown size'}</div>
                                            </div>
                                            <div className={`text-blue-500 transition-opacity ${isSelected ? 'opacity-100' : 'opacity-0 group-hover:opacity-30'}`}>
                                                {isSelected ? <CheckSquare size={18} /> : <Square size={18} />}
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>

                            <div className="p-4 border-t border-slate-700/50 bg-slate-900/50 backdrop-blur-md flex justify-end">
                                <button
                                    onClick={handleDownloadSelected}
                                    disabled={selectedUrls.size === 0}
                                    className={`px-6 py-2.5 rounded-lg font-bold text-white flex items-center gap-2 transition-all shadow-lg ${selectedUrls.size === 0
                                        ? 'bg-slate-700 opacity-50 cursor-not-allowed'
                                        : 'bg-blue-600 hover:bg-blue-500 shadow-blue-900/20'
                                        }`}
                                >
                                    <Download size={18} />
                                    Download {selectedUrls.size} Files
                                </button>
                            </div>
                        </div>
                    )}
                </div>
            </motion.div>
        </div>
    );
};
