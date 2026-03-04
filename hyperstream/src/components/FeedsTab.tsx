import React, { useState, useEffect, useRef } from 'react';
import { error as logError } from '../utils/logger';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { RefreshCw, Plus, Trash2, Download, Rss, ExternalLink } from 'lucide-react';
import { useToast } from '../contexts/ToastContext';
import { AppSettings } from '../types';

interface FeedConfig {
    id: string;
    url: string;
    name: string;
    refresh_interval_mins: number;
    auto_download_regex?: string;
    last_checked?: number;
    enabled: boolean;
}

interface FeedItem {
    title: string;
    link: string;
    pub_date?: string;
    description?: string;
    read: boolean;
}

export const FeedsTab: React.FC = () => {
    const toast = useToast();
    const feedNextId = useRef(0);
    const [feeds, setFeeds] = useState<FeedConfig[]>([]);
    const [selectedFeedId, setSelectedFeedId] = useState<string | null>(null);
    const [items, setItems] = useState<FeedItem[]>([]);
    const [loading, setLoading] = useState(false);

    // Modal State
    const [isAddModalOpen, setIsAddModalOpen] = useState(false);
    const [newFeedUrl, setNewFeedUrl] = useState('');
    const [newFeedName, setNewFeedName] = useState('');

    useEffect(() => {
        loadFeeds();
    }, []);

    useEffect(() => {
        let cancelled = false;
        if (selectedFeedId) {
            const feed = feeds.find(f => f.id === selectedFeedId);
            if (feed) {
                setLoading(true);
                invoke<FeedItem[]>('fetch_feed', { url: feed.url })
                    .then(fetched => { if (!cancelled) setItems(fetched); })
                    .catch(e => { if (!cancelled) { logError("Failed to fetch feed items", e); toast.error("Failed to fetch feed items"); } })
                    .finally(() => { if (!cancelled) setLoading(false); });
            }
        } else {
            setItems([]);
        }
        return () => { cancelled = true; };
    }, [selectedFeedId, feeds]);

    const loadFeeds = async () => {
        try {
            const list = await invoke<FeedConfig[]>('get_feeds');
            setFeeds(list);
            if (list.length > 0 && !selectedFeedId) {
                setSelectedFeedId(list[0].id);
            }
        } catch (e) {
            logError("Failed to load feeds", e);
            toast.error("Failed to load feeds");
        }
    };

    const fetchItems = async (url: string) => {
        setLoading(true);
        try {
            const fetchedItems = await invoke<FeedItem[]>('fetch_feed', { url });
            setItems(fetchedItems);
        } catch (e) {
            logError("Failed to fetch feed items", e);
            toast.error("Failed to fetch feed items");
        }
        setLoading(false);
    };

    const handleAddFeed = async () => {
        if (!newFeedUrl || !newFeedName) return;
        const newFeed: FeedConfig = {
            id: Date.now().toString(),
            url: newFeedUrl,
            name: newFeedName,
            refresh_interval_mins: 60,
            auto_download_regex: undefined,
            last_checked: undefined,
            enabled: true
        };

        try {
            await invoke('add_feed', { config: newFeed });
            setNewFeedUrl('');
            setNewFeedName('');
            setIsAddModalOpen(false);
            loadFeeds();
        } catch (e) {
            logError("Failed to add feed", e);
            toast.error("Failed to add feed");
        }
    };

    const handleRemoveFeed = async (id: string, e: React.MouseEvent) => {
        e.stopPropagation();
        if (!confirm("Are you sure you want to remove this feed?")) return;
        try {
            await invoke('remove_feed', { id });
            if (selectedFeedId === id) setSelectedFeedId(null);
            loadFeeds();
        } catch (err) {
            logError("Failed to remove feed", err);
            toast.error("Failed to remove feed");
        }
    };

    const handleDownload = async (url: string, title: string) => {
        try {
            const urlExt = url.split('/').pop()?.split('?')[0]?.match(/\.[a-zA-Z0-9]+$/)?.[0] || '';
            const filename = title.replace(/[^a-zA-Z0-9.-]/g, "_") + urlExt;
            const settings = await invoke<AppSettings>('get_settings');
            const downloadId = `feed_${Date.now()}_${feedNextId.current++}`;
            await invoke('start_download', {
                id: downloadId,
                url,
                path: `${settings.download_dir}/${filename}`,
            });
        } catch (e) {
            logError("Failed to start download", e);
            toast.error("Failed to start download");
        }
    };

    return (
        <div className="flex h-full gap-6">
            {/* Sidebar: Feed List */}
            <div className="w-64 flex flex-col bg-slate-900/50 rounded-xl border border-slate-700/50 overflow-hidden shadow-xl">
                <div className="p-4 border-b border-slate-700/50 flex justify-between items-center bg-slate-900/50">
                    <h3 className="font-semibold text-slate-200">Feeds</h3>
                    <motion.button
                        whileHover={{ scale: 1.1 }}
                        whileTap={{ scale: 0.9 }}
                        onClick={() => setIsAddModalOpen(true)}
                        className="p-1.5 text-slate-400 hover:text-blue-400 hover:bg-blue-500/10 rounded-lg transition-colors"
                    >
                        <Plus size={18} />
                    </motion.button>
                </div>
                <div className="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1">
                    {feeds.map(feed => (
                        <div
                            key={feed.id}
                            onClick={() => setSelectedFeedId(feed.id)}
                            className={`
                                group px-3 py-2.5 rounded-lg cursor-pointer flex items-center justify-between transition-all
                                ${selectedFeedId === feed.id
                                    ? 'bg-blue-600 text-white shadow-lg shadow-blue-900/20'
                                    : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
                                }
                            `}
                        >
                            <div className="flex items-center gap-3 overflow-hidden">
                                <Rss size={14} className={selectedFeedId === feed.id ? 'text-blue-200' : 'text-slate-500 group-hover:text-slate-400'} />
                                <span className="truncate text-sm font-medium">{feed.name}</span>
                            </div>
                            <button
                                onClick={(e) => handleRemoveFeed(feed.id, e)}
                                className={`
                                    p-1 rounded opacity-0 group-hover:opacity-100 transition-opacity
                                    ${selectedFeedId === feed.id
                                        ? 'hover:bg-blue-500 text-blue-100'
                                        : 'hover:bg-red-500/10 hover:text-red-400 text-slate-500'
                                    }
                                `}
                            >
                                <Trash2 size={12} />
                            </button>
                        </div>
                    ))}
                    {feeds.length === 0 && (
                        <div className="p-8 text-center text-slate-500 text-sm italic">
                            No feeds added
                        </div>
                    )}
                </div>
            </div>

            {/* Main: Feed Content */}
            <div className="flex-1 flex flex-col bg-slate-900/50 rounded-xl border border-slate-700/50 overflow-hidden shadow-xl relative">
                {selectedFeedId ? (
                    <>
                        <div className="p-4 border-b border-slate-700/50 flex justify-between items-center bg-slate-900/50 backdrop-blur-sm">
                            <h3 className="font-bold text-lg text-slate-200">
                                {feeds.find(f => f.id === selectedFeedId)?.name}
                            </h3>
                            <motion.button
                                whileTap={{ rotate: 180 }}
                                onClick={() => {
                                    const feed = feeds.find(f => f.id === selectedFeedId);
                                    if (feed) fetchItems(feed.url);
                                }}
                                className="p-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors"
                            >
                                <RefreshCw size={18} className={loading ? 'animate-spin' : ''} />
                            </motion.button>
                        </div>

                        <div className="flex-1 overflow-y-auto custom-scrollbar p-4 space-y-4">
                            {loading ? (
                                <div className="flex flex-col items-center justify-center h-64 text-slate-500 gap-3">
                                    <RefreshCw className="animate-spin" size={24} />
                                    <span>Fetching feed...</span>
                                </div>
                            ) : (
                                <AnimatePresence mode="popLayout">
                                    {items.map((item, idx) => (
                                        <motion.div
                                            key={`${item.link || item.title}-${idx}`}
                                            initial={{ opacity: 0, y: 10 }}
                                            animate={{ opacity: 1, y: 0 }}
                                            transition={{ delay: idx * 0.05 }}
                                            className="bg-slate-800/30 border border-slate-700/50 rounded-xl p-5 hover:bg-slate-800/50 transition-colors group"
                                        >
                                            <div className="flex justify-between items-start mb-2">
                                                <a
                                                    href={item.link}
                                                    target="_blank"
                                                    rel="noreferrer"
                                                    className="text-base font-semibold text-slate-200 hover:text-blue-400 transition-colors flex items-center gap-2"
                                                >
                                                    {item.title}
                                                    <ExternalLink size={12} className="opacity-0 group-hover:opacity-50" />
                                                </a>
                                                <span className="text-xs text-slate-500 font-mono">
                                                    {item.pub_date ? new Date(item.pub_date).toLocaleDateString() : ''}
                                                </span>
                                            </div>

                                            {item.description && (
                                                <p className="text-sm text-slate-400 leading-relaxed mb-4 line-clamp-3">
                                                    {item.description.replace(/<[^>]*>?/gm, '').replace(/&[a-z]+;/gi, ' ').replace(/\s+/g, ' ').trim()}
                                                </p>
                                            )}

                                            <div className="flex gap-3">
                                                <button
                                                    onClick={() => handleDownload(item.link, item.title)}
                                                    className="flex items-center gap-2 px-3 py-1.5 bg-blue-500/10 hover:bg-blue-500/20 text-blue-400 text-xs font-medium rounded-lg border border-blue-500/20 transition-colors"
                                                >
                                                    <Download size={14} />
                                                    Download
                                                </button>
                                            </div>
                                        </motion.div>
                                    ))}
                                </AnimatePresence>
                            )}
                        </div>
                    </>
                ) : (
                    <div className="flex flex-col items-center justify-center h-full text-slate-500 gap-4 opacity-50">
                        <Rss size={48} />
                        <span className="text-sm font-medium">Select a feed to view contents</span>
                    </div>
                )}
            </div>

            {/* Add Feed Modal */}
            <AnimatePresence>
                {isAddModalOpen && (
                    <>
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            onClick={() => setIsAddModalOpen(false)}
                            className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50"
                        />
                        <motion.div
                            className="fixed inset-0 z-50 flex items-center justify-center pointer-events-none"
                        >
                            <motion.div
                                className="w-full max-w-md bg-slate-900 border border-slate-700/50 rounded-xl shadow-2xl p-6 pointer-events-auto"
                                initial={{ scale: 0.95, opacity: 0, y: 10 }}
                                animate={{ scale: 1, opacity: 1, y: 0 }}
                                exit={{ scale: 0.95, opacity: 0, y: 10 }}
                                onClick={e => e.stopPropagation()}
                            >
                                <h3 className="text-lg font-bold text-white mb-4">Add RSS Feed</h3>
                                <div className="space-y-4">
                                    <div className="space-y-2">
                                        <label className="text-sm font-medium text-slate-400">Feed Name</label>
                                        <input
                                            className="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                            value={newFeedName}
                                            onChange={e => setNewFeedName(e.target.value)}
                                            placeholder="e.g. My Podcast"
                                            autoFocus
                                        />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-sm font-medium text-slate-400">Feed URL</label>
                                        <input
                                            className="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                            value={newFeedUrl}
                                            onChange={e => setNewFeedUrl(e.target.value)}
                                            placeholder="https://example.com/feed.xml"
                                        />
                                    </div>
                                    <div className="flex justify-end gap-3 pt-2">
                                        <button
                                            className="px-4 py-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors text-sm font-medium"
                                            onClick={() => setIsAddModalOpen(false)}
                                        >
                                            Cancel
                                        </button>
                                        <button
                                            className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg shadow-lg shadow-blue-900/20 transition-all text-sm font-bold"
                                            onClick={handleAddFeed}
                                        >
                                            Add Feed
                                        </button>
                                    </div>
                                </div>
                            </motion.div>
                        </motion.div>
                    </>
                )}
            </AnimatePresence>
        </div>
    );
};
