import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Package, X, Search, Download, CheckSquare, Square, Filter } from 'lucide-react';

interface LinkItem {
    url: string;
    filename: string;
    size?: string;
    selected: boolean;
}

interface BatchDownloadModalProps {
    isOpen: boolean;
    links: Array<{ url: string; filename: string }>;
    onClose: () => void;
    onDownload: (links: Array<{ url: string; filename: string }>) => void;
}

const getExtIcon = (filename: string) => {
    const ext = filename.split('.').pop()?.toLowerCase() || '';
    const icons: Record<string, string> = {
        zip: '📦', rar: '📦', '7z': '📦', tar: '📦', gz: '📦',
        mp4: '🎬', mkv: '🎬', avi: '🎬', mov: '🎬', webm: '🎬',
        mp3: '🎵', flac: '🎵', wav: '🎵', aac: '🎵',
        pdf: '📄', doc: '📄', docx: '📄', xls: '📊', xlsx: '📊',
        exe: '⚙️', msi: '⚙️', dmg: '⚙️',
        iso: '💿', img: '💿',
        jpg: '🖼️', jpeg: '🖼️', png: '🖼️', gif: '🖼️',
    };
    return icons[ext] || '📄';
};

export const BatchDownloadModal: React.FC<BatchDownloadModalProps> = ({
    isOpen,
    links,
    onClose,
    onDownload,
}) => {
    const [items, setItems] = useState<LinkItem[]>(
        links.map(l => ({ ...l, selected: true }))
    );
    const [filter, setFilter] = useState('');
    const [typeFilter, setTypeFilter] = useState<string>('all');

    useEffect(() => {
        setItems(links.map(l => ({ ...l, selected: true })));
    }, [links]);

    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') { e.preventDefault(); onClose(); }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, onClose]);

    const filteredItems = items.filter(item => {
        const matchesSearch = item.filename.toLowerCase().includes(filter.toLowerCase()) ||
            item.url.toLowerCase().includes(filter.toLowerCase());
        if (typeFilter === 'all') return matchesSearch;

        const ext = item.filename.split('.').pop()?.toLowerCase() || '';
        const typeMap: Record<string, string[]> = {
            video: ['mp4', 'mkv', 'avi', 'mov', 'webm'],
            audio: ['mp3', 'flac', 'wav', 'aac'],
            archive: ['zip', 'rar', '7z', 'tar', 'gz'],
            document: ['pdf', 'doc', 'docx', 'xls', 'xlsx'],
            program: ['exe', 'msi', 'dmg'],
            image: ['jpg', 'jpeg', 'png', 'gif', 'webp'],
        };
        return matchesSearch && typeMap[typeFilter]?.includes(ext);
    });

    const selectedCount = items.filter(i => i.selected).length;

    const toggleAll = (selected: boolean) => {
        // Build a Set of visible indices for O(1) lookup
        const visibleIndices = new Set(
            items.map((item, i) => {
                const matchesSearch = item.filename.toLowerCase().includes(filter.toLowerCase()) ||
                    item.url.toLowerCase().includes(filter.toLowerCase());
                if (typeFilter === 'all') return matchesSearch ? i : -1;
                const ext = item.filename.split('.').pop()?.toLowerCase() || '';
                const typeMap: Record<string, string[]> = {
                    video: ['mp4', 'mkv', 'avi', 'mov', 'webm'],
                    audio: ['mp3', 'flac', 'wav', 'aac'],
                    archive: ['zip', 'rar', '7z', 'tar', 'gz'],
                    document: ['pdf', 'doc', 'docx', 'xls', 'xlsx'],
                    program: ['exe', 'msi', 'dmg'],
                    image: ['jpg', 'jpeg', 'png', 'gif', 'webp'],
                };
                return (matchesSearch && typeMap[typeFilter]?.includes(ext)) ? i : -1;
            }).filter(i => i >= 0)
        );
        setItems(items.map((item, i) =>
            visibleIndices.has(i) ? { ...item, selected } : item
        ));
    };

    const toggleItem = (index: number) => {
        setItems(items.map((item, i) =>
            i === index ? { ...item, selected: !item.selected } : item
        ));
    };

    const handleDownload = () => {
        const selected = items.filter(i => i.selected).map(i => ({
            url: i.url,
            filename: i.filename,
        }));
        onDownload(selected);
        onClose();
    };

    return (
        <AnimatePresence>
            {isOpen && (
            <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
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
                            <div className="w-8 h-8 rounded-lg bg-violet-500/10 border border-violet-500/20 flex items-center justify-center">
                                <Package size={16} className="text-violet-400" />
                            </div>
                            <h3 className="text-lg font-semibold text-white">Batch Download</h3>
                            <span className="text-xs text-slate-500">{items.length} files</span>
                        </div>
                        <button onClick={onClose} className="p-1.5 text-slate-500 hover:text-white hover:bg-white/10 rounded-lg transition-colors">
                            <X size={16} />
                        </button>
                    </div>

                    {/* Toolbar */}
                    <div className="flex items-center gap-3 px-6 py-3 border-b border-white/5 flex-shrink-0">
                        <div className="relative flex-1">
                            <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
                            <input
                                type="text"
                                placeholder="Search files..."
                                value={filter}
                                onChange={(e) => setFilter(e.target.value)}
                                className="w-full bg-black/30 border border-white/10 rounded-lg pl-9 pr-3 py-2 text-sm text-white placeholder-slate-600 focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 transition-colors"
                            />
                        </div>
                        <div className="relative">
                            <Filter size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500 pointer-events-none" />
                            <select
                                value={typeFilter}
                                onChange={(e) => setTypeFilter(e.target.value)}
                                className="appearance-none bg-black/30 border border-white/10 rounded-lg pl-9 pr-8 py-2 text-sm text-white focus:border-cyan-500/50 focus:outline-none cursor-pointer"
                            >
                                <option value="all">All Types</option>
                                <option value="video">Videos</option>
                                <option value="audio">Audio</option>
                                <option value="archive">Archives</option>
                                <option value="document">Documents</option>
                                <option value="program">Programs</option>
                                <option value="image">Images</option>
                            </select>
                        </div>
                        <div className="flex items-center gap-1">
                            <button
                                onClick={() => toggleAll(true)}
                                className="p-2 text-slate-400 hover:text-cyan-400 hover:bg-white/5 rounded-lg transition-colors"
                                title="Select All"
                            >
                                <CheckSquare size={16} />
                            </button>
                            <button
                                onClick={() => toggleAll(false)}
                                className="p-2 text-slate-400 hover:text-cyan-400 hover:bg-white/5 rounded-lg transition-colors"
                                title="Deselect All"
                            >
                                <Square size={16} />
                            </button>
                        </div>
                    </div>

                    {/* File List */}
                    <div className="flex-1 overflow-y-auto custom-scrollbar p-3 space-y-1">
                        {filteredItems.length === 0 ? (
                            <div className="text-center py-12 text-slate-500">
                                <Search size={28} className="mx-auto mb-2 opacity-30" />
                                <p className="text-sm">No files found</p>
                            </div>
                        ) : (
                            filteredItems.map((item) => {
                                // Find the actual index in the original items array
                                const originalIndex = items.indexOf(item);
                                return (
                                <motion.div
                                    key={`${item.url}-${originalIndex}`}
                                    initial={{ opacity: 0, y: 4 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    transition={{ delay: Math.min(filteredItems.indexOf(item) * 0.02, 0.5) }}
                                    className={`flex items-center gap-3 px-3 py-2.5 rounded-xl cursor-pointer transition-colors ${
                                        item.selected
                                            ? 'bg-cyan-500/10 border border-cyan-500/20'
                                            : 'bg-white/[0.02] border border-transparent hover:bg-white/5'
                                    }`}
                                    onClick={() => toggleItem(originalIndex)}
                                >
                                    <input
                                        type="checkbox"
                                        checked={item.selected}
                                        onChange={() => toggleItem(originalIndex)}
                                        onClick={(e) => e.stopPropagation()}
                                        className="rounded border-white/20 bg-transparent text-cyan-500 focus:ring-cyan-500/30 flex-shrink-0"
                                    />
                                    <span className="text-base flex-shrink-0">{getExtIcon(item.filename)}</span>
                                    <div className="flex-1 min-w-0">
                                        <div className="text-sm font-medium text-slate-200 truncate">{item.filename}</div>
                                        <div className="text-[11px] text-slate-600 truncate">{item.url}</div>
                                    </div>
                                </motion.div>
                                );
                            })
                        )}
                    </div>

                    {/* Footer */}
                    <div className="flex items-center justify-between px-6 py-4 border-t border-white/5 flex-shrink-0">
                        <span className="text-xs text-slate-500">
                            {selectedCount} of {items.length} selected
                        </span>
                        <div className="flex items-center gap-2">
                            <button
                                onClick={onClose}
                                className="px-4 py-2 text-sm text-slate-400 hover:text-white hover:bg-white/5 rounded-lg transition-colors"
                            >
                                Cancel
                            </button>
                            <motion.button
                                whileHover={{ scale: 1.02 }}
                                whileTap={{ scale: 0.98 }}
                                onClick={handleDownload}
                                disabled={selectedCount === 0}
                                className="flex items-center gap-2 px-5 py-2 bg-gradient-to-r from-cyan-500 to-blue-600 hover:from-cyan-400 hover:to-blue-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-sm font-semibold rounded-lg transition-all shadow-[0_0_15px_rgba(6,182,212,0.3)]"
                            >
                                <Download size={14} />
                                Download {selectedCount} Files
                            </motion.button>
                        </div>
                    </div>
                </motion.div>
            </motion.div>
            )}
        </AnimatePresence>
    );
};
