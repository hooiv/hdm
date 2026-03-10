import React, { useState, useEffect, useRef } from 'react';
import { error as logError } from '../utils/logger';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { RefreshCw, Plus, Trash2, Download, Rss, ExternalLink, Cog } from 'lucide-react';
import { useToast } from '../contexts/ToastContext';
import { AppSettings } from '../types';
import { safeListen } from '../utils/tauri';

async function notifyFeedUpdate(title: string, body: string) {
    if (typeof window === 'undefined' || !('Notification' in window)) {
        return;
    }

    if (Notification.permission !== 'granted') {
        return;
    }

    try {
        new Notification(title, { body });
    } catch (error) {
        logError('Failed to show feed notification', error);
    }
}

interface FeedConfig {
    id: string;
    url: string;
    name: string;
    refresh_interval_mins: number;
    auto_download_regex?: string;
    last_checked?: number;
    enabled: boolean;
    unread_count?: number; // backend will supply this
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

    // Modal State / add & edit
    const [isAddModalOpen, setIsAddModalOpen] = useState(false);
    const [newFeedUrl, setNewFeedUrl] = useState('');
    const [newFeedName, setNewFeedName] = useState('');
    const [newRefresh, setNewRefresh] = useState<number>(60);
    const [newAutoRegex, setNewAutoRegex] = useState('');
    const [newEnabled, setNewEnabled] = useState(true);
    const [editingFeed, setEditingFeed] = useState<FeedConfig | null>(null);

    useEffect(() => {
        loadFeeds();
    }, []);

    // Listen for background poller events
    useEffect(() => {
        let unlisten: () => void = () => {};
        safeListen<{feed_id:string, new_items:FeedItem[]}>('feed_update', async event => {
            const { feed_id, new_items } = event.payload;
            toast.success(`${new_items.length} new item${new_items.length !== 1 ? 's' : ''} received`);
            await notifyFeedUpdate('RSS Feed', `${new_items.length} new item${new_items.length !== 1 ? 's' : ''}`);
            // refresh feed list counts
            loadFeeds();
            if (selectedFeedId === feed_id) {
                setItems(prev => [...new_items, ...prev]);
            }
        }).then(u => { unlisten = u; });
        return () => { unlisten(); };
    }, [selectedFeedId]);

    useEffect(() => {
        let cancelled = false;
        if (selectedFeedId) {
            // load stored items first
            invoke<FeedItem[]>('get_feed_items', { feed_id: selectedFeedId })
                .then(list => { if (!cancelled) setItems(list); })
                .catch(e => { if (!cancelled) { logError("Failed to load stored items", e); } });
        } else {
            setItems([]);
        }
        return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [selectedFeedId]);

    const loadFeeds = async (selectFallback?: boolean) => {
        try {
            const list = await invoke<FeedConfig[]>('get_feeds');
            setFeeds(list);
            if (list.length > 0 && (selectFallback || !selectedFeedId)) {
                setSelectedFeedId(list[0].id);
            }
        } catch (e) {
            logError("Failed to load feeds", e);
            toast.error("Failed to load feeds");
        }
    };


    const clearModalFields = () => {
        setNewFeedUrl('');
        setNewFeedName('');
        setNewRefresh(60);
        setNewAutoRegex('');
        setNewEnabled(true);
        setEditingFeed(null);
    };

    const handleSaveFeed = async () => {
        if (!newFeedUrl || !newFeedName) return;

        // Validate URL format
        try {
            const parsed = new URL(newFeedUrl);
            if (!['http:', 'https:'].includes(parsed.protocol)) {
                toast.error("Feed URL must use http or https protocol");
                return;
            }
        } catch {
            toast.error("Invalid feed URL format");
            return;
        }

        const config: FeedConfig = editingFeed ? {
            ...editingFeed,
            url: newFeedUrl,
            name: newFeedName,
            refresh_interval_mins: newRefresh,
            auto_download_regex: newAutoRegex || undefined,
            enabled: newEnabled,
        } : {
            id: Date.now().toString(),
            url: newFeedUrl,
            name: newFeedName,
            refresh_interval_mins: newRefresh,
            auto_download_regex: newAutoRegex || undefined,
            last_checked: undefined,
            enabled: newEnabled,
        };

        try {
            if (editingFeed) {
                await invoke('update_feed', { config });
            } else {
                await invoke('add_feed', { config });
            }
            clearModalFields();
            setIsAddModalOpen(false);
            loadFeeds();
        } catch (e) {
            logError(editingFeed ? "Failed to update feed" : "Failed to add feed", e);
            toast.error(editingFeed ? "Failed to update feed" : "Failed to add feed");
        }
    };

    const handleRemoveFeed = async (id: string, e: React.MouseEvent) => {
        e.stopPropagation();
        if (!confirm("Are you sure you want to remove this feed?")) return;
        const wasSelected = selectedFeedId === id;
        try {
            await invoke('remove_feed', { id });
            if (wasSelected) setSelectedFeedId(null);
            loadFeeds(wasSelected);
        } catch (err) {
            logError("Failed to remove feed", err);
            toast.error("Failed to remove feed");
        }
    };

    const openEditFeed = (feed: FeedConfig, e: React.MouseEvent) => {
        e.stopPropagation();
        setEditingFeed(feed);
        setNewFeedUrl(feed.url);
        setNewFeedName(feed.name);
        setNewRefresh(feed.refresh_interval_mins);
        setNewAutoRegex(feed.auto_download_regex || '');
        setNewEnabled(feed.enabled);
        setIsAddModalOpen(true);
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

    const markItemRead = async (item: FeedItem) => {
        if (!selectedFeedId) return;
        try {
            await invoke('mark_feed_item_read', { feed_id: selectedFeedId, link: item.link });
            setItems(prev => prev.map(i => i.link === item.link ? { ...i, read: true } : i));
        } catch (e) {
            logError("Failed to mark item read", e);
        }
    };

    const handleItemClick = async (item: FeedItem) => {
        try {
            await invoke('open_file', { path: item.link });
        } catch {
            window.open(item.link, '_blank');
        }
        if (!item.read) {
            markItemRead(item);
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
                                {(feed.unread_count ?? 0) > 0 && (
                                    <span className="ml-auto bg-red-600 text-white text-xs px-2 py-0.5 rounded-full">
                                        {feed.unread_count ?? 0}
                                    </span>
                                )}
                            </div>
                            <div className="flex items-center gap-1">
                                <button
                                    onClick={(e) => openEditFeed(feed, e)}
                                    className="p-1 rounded opacity-0 group-hover:opacity-100 transition-opacity text-slate-500 hover:text-blue-400"
                                >
                                    <Cog size={12} />
                                </button>
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
                                onClick={async () => {
                                    if (!selectedFeedId) return;
                                    const feed = feeds.find(f => f.id === selectedFeedId);
                                    if (!feed) return;
                                    setLoading(true);
                                    try {
                                        await invoke('manual_refresh_feed', { feed_id: feed.id });
                                        await loadFeeds();
                                        const list = await invoke<FeedItem[]>('get_feed_items', { feed_id: feed.id });
                                        setItems(list);
                                    } catch (e) {
                                        logError('Manual refresh failed', e);
                                        toast.error('Refresh failed');
                                    }
                                    setLoading(false);
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
                                                <button
                                                    onClick={() => handleItemClick(item)}
                                                    className={`text-base font-semibold ${item.read ? 'text-slate-500' : 'text-slate-200'} hover:text-blue-400 transition-colors flex items-center gap-2 text-left`}
                                                >
                                                    {item.title}
                                                    <ExternalLink size={12} className="opacity-0 group-hover:opacity-50" />
                                                </button>
                                                <span className="text-xs text-slate-500 font-mono">
                                                    {item.pub_date ? new Date(item.pub_date).toLocaleDateString() : ''}
                                                </span>
                                            </div>

                                            {item.description && (
                                                <p className="text-sm text-slate-400 leading-relaxed mb-4 line-clamp-3">
                                                    {(() => { try { return new DOMParser().parseFromString(item.description, 'text/html').body.textContent?.replace(/\s+/g, ' ').trim() || ''; } catch { return item.description.replace(/<[^>]*>?/gm, '').replace(/&[a-z]+;/gi, ' ').replace(/\s+/g, ' ').trim(); } })()}
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
                                        {!item.read && (
                                            <button
                                                onClick={() => markItemRead(item)}
                                                className="text-xs text-slate-400 hover:text-slate-200 ml-3"
                                            >
                                                Mark read
                                            </button>
                                        )}
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

            {/* Add/Edit Feed Modal */}
            <AnimatePresence>
                {isAddModalOpen && (
                    <>
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            onClick={() => { setIsAddModalOpen(false); clearModalFields(); }}
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
                                onKeyDown={e => { if (e.key === 'Escape') { setIsAddModalOpen(false); clearModalFields(); } else if (e.key === 'Enter') handleSaveFeed(); }}
                                role="dialog"
                                aria-modal="true"
                                aria-label={editingFeed ? "Edit RSS Feed" : "Add RSS Feed"}
                            >
                                <h3 className="text-lg font-bold text-white mb-4">
                                    {editingFeed ? 'Edit RSS Feed' : 'Add RSS Feed'}
                                </h3>
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
                                    <div className="space-y-2">
                                        <label className="text-sm font-medium text-slate-400">Refresh Interval (mins)</label>
                                        <input
                                            type="number"
                                            min={1}
                                            className="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                            value={newRefresh}
                                            onChange={e => setNewRefresh(parseInt(e.target.value) || 1)}
                                        />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-sm font-medium text-slate-400">Auto-download Regex (optional)</label>
                                        <input
                                            className="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                                            value={newAutoRegex}
                                            onChange={e => setNewAutoRegex(e.target.value)}
                                            placeholder="e.g. .*\\.mp3$"
                                        />
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <input
                                            type="checkbox"
                                            checked={newEnabled}
                                            onChange={e => setNewEnabled(e.target.checked)}
                                            id="feed-enabled-checkbox"
                                            className="accent-blue-500"
                                        />
                                        <label htmlFor="feed-enabled-checkbox" className="text-sm text-slate-400">Enabled</label>
                                    </div>
                                    <div className="flex justify-end gap-3 pt-2">
                                        <button
                                            className="px-4 py-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors text-sm font-medium"
                                            onClick={() => { setIsAddModalOpen(false); clearModalFields(); }}
                                        >
                                            Cancel
                                        </button>
                                        <button
                                            className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg shadow-lg shadow-blue-900/20 transition-all text-sm font-bold"
                                            onClick={handleSaveFeed}
                                        >
                                            {editingFeed ? 'Save' : 'Add'}
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
