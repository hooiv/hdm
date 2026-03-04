import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { X, Share2, Users, Upload, Download, Copy, Check, Wifi, AlertCircle } from 'lucide-react';
import { useToast } from '../contexts/ToastContext';
import { formatBytes } from '../utils/formatters';
import { error as logError } from '../utils/logger';

interface P2PShareSession {
    id: string;
    download_id: string;
    pairing_code: string;
    peers: string[];
    bytes_sent: number;
    bytes_received: number;
    created_at: number;
    is_host: boolean;
}

interface P2PShareModalProps {
    isOpen: boolean;
    onClose: () => void;
    downloadId: string;
    downloadName: string;
}

export default function P2PShareModal({ isOpen, onClose, downloadId, downloadName }: P2PShareModalProps) {
    const toast = useToast();
    const [session, setSession] = useState<P2PShareSession | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);

    // Join mode states
    const [isJoinMode, setIsJoinMode] = useState(false);
    const [joinCode, setJoinCode] = useState('');
    const [peerAddress, setPeerAddress] = useState('');

    useEffect(() => {
        if (isOpen && !isJoinMode) {
            createShare();
        }
    }, [isOpen, downloadId, isJoinMode]);

    // Cleanup: close session when modal unmounts or closes
    useEffect(() => {
        return () => {
            // Use a local ref capture so we don't close a stale session
            if (session) {
                invoke('close_p2p_session', { sessionId: session.id }).catch(() => {});
            }
        };
    }, [session?.id]);

    const createShare = async () => {
        try {
            setLoading(true);
            setError(null);
            const result = await invoke<P2PShareSession>('create_p2p_share', {
                downloadId: downloadId,
            });
            setSession(result);
        } catch (e) {
            setError(String(e));
        } finally {
            setLoading(false);
        }
    };

    const joinShare = async () => {
        if (!joinCode || !peerAddress) {
            setError('Please enter both pairing code and peer address');
            return;
        }

        try {
            setLoading(true);
            setError(null);
            const result = await invoke<P2PShareSession>('join_p2p_share', {
                code: joinCode,
                peerAddr: peerAddress,
            });
            setSession(result);
            setIsJoinMode(false);
        } catch (e) {
            setError(String(e));
        } finally {
            setLoading(false);
        }
    };

    const closeShare = async () => {
        if (session) {
            try {
                await invoke('close_p2p_session', { sessionId: session.id });
                onClose();
            } catch (e) {
                logError('Failed to close session:', e);
                toast.error("Failed to close P2P session");
                onClose();
            }
        } else {
            onClose();
        }
    };

    const copyPairingCode = () => {
        if (session) {
            navigator.clipboard.writeText(session.pairing_code);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                    onClick={closeShare}
                    role="dialog"
                    aria-modal="true"
                >
                    <motion.div
                        initial={{ scale: 0.95, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        exit={{ scale: 0.95, opacity: 0 }}
                        onClick={(e) => e.stopPropagation()}
                        className="bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 rounded-2xl p-8 max-w-2xl w-full border border-cyan-500/20 shadow-2xl shadow-cyan-500/10"
                    >
                        {/* Header */}
                        <div className="flex items-center justify-between mb-6">
                            <div className="flex items-center gap-3">
                                <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-cyan-500 to-blue-600 flex items-center justify-center">
                                    <Share2 className="w-6 h-6 text-white" />
                                </div>
                                <div>
                                    <h2 className="text-2xl font-bold text-white">P2P Share</h2>
                                    <p className="text-sm text-slate-400">{downloadName}</p>
                                </div>
                            </div>
                            <button
                                onClick={closeShare}
                                className="w-10 h-10 rounded-xl bg-slate-800/50 hover:bg-slate-700/50 flex items-center justify-center transition-colors border border-slate-700/50"
                            >
                                <X className="w-5 h-5 text-slate-400" />
                            </button>
                        </div>

                        {/* Mode Toggle */}
                        {!session && (
                            <div className="flex gap-2 mb-6">
                                <button
                                    onClick={() => setIsJoinMode(false)}
                                    className={`flex-1 py-3 px-4 rounded-xl font-medium transition-all ${!isJoinMode
                                            ? 'bg-cyan-500 text-white shadow-lg shadow-cyan-500/30'
                                            : 'bg-slate-800/50 text-slate-400 hover:bg-slate-700/50'
                                        }`}
                                >
                                    <Upload className="w-4 h-4 inline mr-2" />
                                    Share File
                                </button>
                                <button
                                    onClick={() => setIsJoinMode(true)}
                                    className={`flex-1 py-3 px-4 rounded-xl font-medium transition-all ${isJoinMode
                                            ? 'bg-cyan-500 text-white shadow-lg shadow-cyan-500/30'
                                            : 'bg-slate-800/50 text-slate-400 hover:bg-slate-700/50'
                                        }`}
                                >
                                    <Download className="w-4 h-4 inline mr-2" />
                                    Join Share
                                </button>
                            </div>
                        )}

                        {/* Error Display */}
                        {error && (
                            <div className="mb-6 p-4 bg-red-500/10 border border-red-500/30 rounded-xl flex items-start gap-3">
                                <AlertCircle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
                                <div className="text-sm text-red-200">{error}</div>
                            </div>
                        )}

                        {/* Loading State */}
                        {loading && (
                            <div className="text-center py-12">
                                <div className="inline-block w-12 h-12 border-4 border-cyan-500/30 border-t-cyan-500 rounded-full animate-spin mb-4"></div>
                                <p className="text-slate-400">
                                    {isJoinMode ? 'Connecting to peer...' : 'Creating share session...'}
                                </p>
                            </div>
                        )}

                        {/* Share Mode (Host) */}
                        {!loading && !isJoinMode && session && session.is_host && (
                            <div className="space-y-6">
                                {/* Pairing Code */}
                                <div className="bg-slate-800/30 rounded-xl p-6 border border-slate-700/30">
                                    <label className="text-sm font-medium text-slate-300 mb-3 block">
                                        Pairing Code
                                    </label>
                                    <div className="flex items-center gap-3">
                                        <div className="flex-1 bg-slate-900/50 rounded-lg px-6 py-4 border border-cyan-500/30">
                                            <p className="text-3xl font-mono font-bold text-cyan-400 tracking-wider">
                                                {session.pairing_code}
                                            </p>
                                        </div>
                                        <button
                                            onClick={copyPairingCode}
                                            className="w-12 h-12 rounded-lg bg-cyan-500 hover:bg-cyan-600 flex items-center justify-center transition-colors shadow-lg shadow-cyan-500/30"
                                        >
                                            {copied ? (
                                                <Check className="w-5 h-5 text-white" />
                                            ) : (
                                                <Copy className="w-5 h-5 text-white" />
                                            )}
                                        </button>
                                    </div>
                                    <p className="text-xs text-slate-500 mt-3">
                                        Share this code with peers to let them download this file
                                    </p>
                                </div>

                                {/* Stats */}
                                <div className="grid grid-cols-2 gap-4">
                                    <div className="bg-slate-800/30 rounded-xl p-4 border border-slate-700/30">
                                        <div className="flex items-center gap-2 text-slate-400 text-sm mb-2">
                                            <Users className="w-4 h-4" />
                                            Connected Peers
                                        </div>
                                        <p className="text-2xl font-bold text-white">{session.peers.length}</p>
                                    </div>
                                    <div className="bg-slate-800/30 rounded-xl p-4 border border-slate-700/30">
                                        <div className="flex items-center gap-2 text-slate-400 text-sm mb-2">
                                            <Upload className="w-4 h-4" />
                                            Uploaded
                                        </div>
                                        <p className="text-2xl font-bold text-cyan-400">
                                            {formatBytes(session.bytes_sent)}
                                        </p>
                                    </div>
                                </div>

                                {/* Peers List */}
                                {session.peers.length > 0 && (
                                    <div className="bg-slate-800/30 rounded-xl p-4 border border-slate-700/30">
                                        <label className="text-sm font-medium text-slate-300 mb-3 block">
                                            Active Peers
                                        </label>
                                        <div className="space-y-2">
                                            {session.peers.map((peer, idx) => (
                                                <div
                                                    key={idx}
                                                    className="flex items-center gap-3 p-3 bg-slate-900/50 rounded-lg"
                                                >
                                                    <Wifi className="w-4 h-4 text-green-400" />
                                                    <span className="text-sm text-slate-300 font-mono">{peer}</span>
                                                </div>
                                            ))}
                                        </div>
                                    </div>
                                )}
                            </div>
                        )}

                        {/* Join Mode */}
                        {!loading && isJoinMode && !session && (
                            <div className="space-y-4">
                                <div>
                                    <label className="text-sm font-medium text-slate-300 mb-2 block">
                                        Pairing Code
                                    </label>
                                    <input
                                        type="text"
                                        value={joinCode}
                                        onChange={(e) => setJoinCode(e.target.value)}
                                        placeholder="brave-tiger-mountain"
                                        className="w-full px-4 py-3 bg-slate-800/50 border border-slate-700/50 rounded-xl text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500/50 transition-colors font-mono"
                                    />
                                </div>
                                <div>
                                    <label className="text-sm font-medium text-slate-300 mb-2 block">
                                        Peer Address
                                    </label>
                                    <input
                                        type="text"
                                        value={peerAddress}
                                        onChange={(e) => setPeerAddress(e.target.value)}
                                        placeholder="192.168.1.10:14735"
                                        className="w-full px-4 py-3 bg-slate-800/50 border border-slate-700/50 rounded-xl text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500/50 transition-colors font-mono"
                                    />
                                    <p className="text-xs text-slate-500 mt-2">
                                        Enter the IP address and port of the peer (default port: 14735)
                                    </p>
                                </div>
                                <button
                                    onClick={joinShare}
                                    disabled={!joinCode || !peerAddress}
                                    className="w-full py-3 px-4 bg-gradient-to-r from-cyan-500 to-blue-600 hover:from-cyan-600 hover:to-blue-700 disabled:from-slate-700 disabled:to-slate-700 disabled:cursor-not-allowed text-white font-medium rounded-xl transition-all shadow-lg shadow-cyan-500/30 disabled:shadow-none"
                                >
                                    Connect to Peer
                                </button>
                            </div>
                        )}

                        {/* Joined Session */}
                        {!loading && session && !session.is_host && (
                            <div className="space-y-6">
                                <div className="bg-green-500/10 border border-green-500/30 rounded-xl p-4 flex items-center gap-3">
                                    <Check className="w-5 h-5 text-green-400" />
                                    <div className="text-sm text-green-200">
                                        Successfully connected to peer!
                                    </div>
                                </div>

                                <div className="bg-slate-800/30 rounded-xl p-4 border border-slate-700/30">
                                    <div className="flex items-center gap-2 text-slate-400 text-sm mb-2">
                                        <Download className="w-4 h-4" />
                                        Downloaded
                                    </div>
                                    <p className="text-2xl font-bold text-cyan-400">
                                        {formatBytes(session.bytes_received)}
                                    </p>
                                </div>
                            </div>
                        )}
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}
