import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { Search, Download, Database, Loader2, HardDrive, ArrowDown, ArrowUp, Brain } from 'lucide-react';
import { useToast } from '../contexts/ToastContext';
import { AppSettings } from '../types';

let searchNextId = 0;

interface SearchResult {
    title: string;
    link: string;
    size?: string;
    seeds?: number;
    leechers?: number;
    engine: string;
}

export const SearchTab: React.FC = () => {
    const toast = useToast();
    const [query, setQuery] = useState('');
    const [results, setResults] = useState<SearchResult[]>([]);
    const [loading, setLoading] = useState(false);
    const [searched, setSearched] = useState(false);
    const [aiMode, setAiMode] = useState(false);

    const handleSearch = async (e?: React.FormEvent) => {
        if (e) e.preventDefault();
        if (!query.trim()) return;

        setLoading(true);
        setSearched(true);
        setResults([]);

        try {
            const command = aiMode ? 'perform_semantic_search' : 'perform_search';
            const data = await invoke<SearchResult[]>(command, { query });
            setResults(data);
        } catch (err) {
            console.error("Search failed", err);
            toast.error("Search failed");
        }
        setLoading(false);
    };

    const handleDownload = async (url: string, title: string) => {
        try {
            // Basic filename sanitization
            const filename = title.replace(/[^a-zA-Z0-9.-]/g, "_") + ".iso";
            const settings = await invoke<AppSettings>('get_settings');
            const downloadId = `search_${Date.now()}_${searchNextId++}`;
            const sep = settings.download_dir.includes('/') ? '/' : '\\';
            await invoke('start_download', {
                id: downloadId,
                url,
                path: `${settings.download_dir}${sep}${filename}`,
            });
        } catch (e) {
            console.error("Failed to start download", e);
            toast.error("Failed to start download");
        }
    };

    return (
        <div className="flex flex-col h-full gap-5">
            {/* Search Bar */}
            <div className="bg-slate-900/50 border border-slate-700/50 rounded-xl p-4 flex gap-4 shadow-xl">
                <div className="flex-1 relative group flex gap-2">
                    <div className="relative flex-1">
                        <Search className="absolute left-4 top-3.5 text-slate-500 group-focus-within:text-blue-500 transition-colors" size={20} />
                        <input
                            className="w-full h-12 bg-slate-800/50 border border-slate-700 rounded-lg pl-12 pr-4 text-lg text-slate-200 placeholder-slate-600 focus:outline-none focus:border-blue-500/50 focus:bg-slate-800 transition-all"
                            placeholder={aiMode ? "Ask naturally (e.g. 'financial reports from May')..." : "Search for Ubuntu, Arch Linux, etc..."}
                            value={query}
                            onChange={e => setQuery(e.target.value)}
                            onKeyDown={e => e.key === 'Enter' && handleSearch()}
                            autoFocus
                        />
                    </div>

                    <button
                        onClick={() => setAiMode(!aiMode)}
                        className={`px-4 rounded-lg border border-slate-700 transition-all flex items-center gap-2 ${aiMode ? 'bg-purple-600/20 text-purple-400 border-purple-500/50' : 'bg-slate-800/50 text-slate-500 hover:text-slate-300'}`}
                        title="Toggle AI Semantic Search"
                    >
                        <Brain size={20} />
                        <span className="text-sm font-medium hidden md:inline">AI</span>
                    </button>
                </div>
                <button
                    className="h-12 px-8 bg-blue-600 hover:bg-blue-500 text-white rounded-lg shadow-lg shadow-blue-900/20 hover:shadow-blue-500/30 font-bold transition-all flex items-center justify-center min-w-[120px]"
                    onClick={() => handleSearch()}
                    disabled={loading}
                >
                    {loading ? <Loader2 className="animate-spin" /> : 'Search'}
                </button>
            </div>

            {/* Results */}
            <div className="flex-1 bg-slate-900/50 border border-slate-700/50 rounded-xl overflow-hidden shadow-xl flex flex-col relative">
                <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center bg-slate-900/50 backdrop-blur-sm">
                    <h3 className="font-semibold text-slate-200">Results {searched && `(${results.length})`}</h3>
                    <div className={`flex items-center gap-2 text-xs font-mono px-3 py-1.5 rounded-full border ${aiMode ? 'bg-purple-900/20 text-purple-400 border-purple-500/20' : 'bg-slate-800/50 text-slate-500 border-slate-700/50'}`}>
                        {aiMode ? <Brain size={12} /> : <Database size={12} />}
                        <span>{aiMode ? 'Neural Index' : 'Lua Engine'}</span>
                    </div>
                </div>

                <div className="flex-1 overflow-y-auto custom-scrollbar">
                    {!searched && (
                        <div className="h-full flex flex-col items-center justify-center text-slate-500 gap-4 opacity-50">
                            <Search size={48} />
                            <p className="text-sm font-medium">Enter a query to search via plugins</p>
                        </div>
                    )}

                    {searched && results.length === 0 && !loading && (
                        <div className="h-full flex flex-col items-center justify-center text-slate-500 gap-4">
                            <Search size={48} className="text-slate-700" />
                            <p className="text-sm">No results found for "{query}"</p>
                        </div>
                    )}

                    <div className="divide-y divide-slate-800">
                        {results.map((item, idx) => (
                            <motion.div
                                key={idx}
                                initial={{ opacity: 0, x: -10 }}
                                animate={{ opacity: 1, x: 0 }}
                                transition={{ delay: idx * 0.05 }}
                                className="p-4 hover:bg-slate-800/40 transition-colors flex items-center justify-between group"
                            >
                                <div className="flex-1 pr-6">
                                    <h4 className="font-semibold text-slate-200 text-sm mb-2 group-hover:text-blue-400 transition-colors">{item.title}</h4>
                                    <div className="flex items-center gap-4 text-xs font-mono">
                                        <span className="bg-slate-800 border border-slate-700 text-slate-400 px-2 py-0.5 rounded flex items-center gap-1">
                                            {item.engine}
                                        </span>
                                        {item.size && (
                                            <span className="flex items-center gap-1 text-slate-500">
                                                <HardDrive size={12} /> {item.size}
                                            </span>
                                        )}
                                        {item.seeds !== undefined && (
                                            <span className="flex items-center gap-1 text-emerald-400">
                                                <ArrowUp size={12} /> {item.seeds}
                                            </span>
                                        )}
                                        {item.leechers !== undefined && (
                                            <span className="flex items-center gap-1 text-red-400">
                                                <ArrowDown size={12} /> {item.leechers}
                                            </span>
                                        )}
                                    </div>
                                </div>
                                <button
                                    onClick={() => handleDownload(item.link, item.title)}
                                    className="opacity-0 group-hover:opacity-100 flex items-center gap-2 px-4 py-2 bg-blue-600/10 hover:bg-blue-600 text-blue-500 hover:text-white rounded-lg transition-all font-medium text-xs border border-blue-500/20 hover:border-transparent"
                                >
                                    <Download size={14} />
                                    Download
                                </button>
                            </motion.div>
                        ))}
                    </div>
                </div>
            </div>
        </div>
    );
};
