import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { Settings, X, Folder, Activity, Globe, Cloud, Save, Shield, Users, Volume2, Webhook, Plus, Trash2, PlayCircle, MessageSquare } from 'lucide-react';

interface SettingsData {
    download_dir: string;
    max_concurrent_downloads: number;
    proxy_enabled: boolean;
    proxy_url: string;
    theme: string;
    vpn_mode: boolean;
    chaos_mode: boolean;
    speed_limit_enabled: boolean;
    speed_limit_rate: number;

    // Cloud
    cloud_enabled: boolean;
    cloud_endpoint: string;
    cloud_bucket: string;
    cloud_region: string;
    cloud_access_key: string;
    cloud_secret_key: string;

    // Advanced / Chaos
    chaos_latency_ms: number;
    chaos_error_rate: number;

    // Privacy
    use_tor: boolean;

    // Team Sync
    last_sync_host?: string;

    // Archive Extraction
    auto_extract_archives?: boolean;
    cleanup_archives_after_extract?: boolean;

    // ChatOps
    telegram_bot_token?: string;
    telegram_chat_id?: string;
    chatops_enabled?: boolean;
}

// Reusable Components
const Toggle: React.FC<{ checked: boolean; onChange: (val: boolean) => void; label?: string }> = ({ checked, onChange, label }) => (
    <div className="flex items-center justify-between py-2 group cursor-pointer" onClick={() => onChange(!checked)}>
        {label && <span className="text-sm font-medium text-slate-300 group-hover:text-white transition-colors">{label}</span>}
        <div className={`w-11 h-6 flex items-center rounded-full p-1 duration-300 ease-in-out ${checked ? 'bg-blue-600' : 'bg-slate-700'}`}>
            <div className={`bg-white w-4 h-4 rounded-full shadow-md transform duration-300 ease-in-out ${checked ? 'translate-x-5' : ''}`} />
        </div>
    </div>
);

const SectionHeader: React.FC<{ icon: any; title: string }> = ({ icon: Icon, title }) => (
    <div className="flex items-center gap-2 mb-4 pb-2 border-b border-slate-700/50 text-blue-400">
        <Icon size={18} />
        <h3 className="font-semibold text-sm uppercase tracking-wider">{title}</h3>
    </div>
);

interface SettingsPageProps {
    isOpen: boolean;
    onClose: () => void;
}

export const SettingsPage: React.FC<SettingsPageProps> = ({ isOpen, onClose }) => {
    const [settings, setSettings] = useState<SettingsData>({
        download_dir: '',
        max_concurrent_downloads: 3,
        proxy_enabled: false,
        proxy_url: '',
        theme: 'dark',
        vpn_mode: false,
        chaos_mode: false,
        speed_limit_enabled: false,
        speed_limit_rate: 1024,
        cloud_enabled: false,
        cloud_endpoint: '',
        cloud_bucket: '',
        cloud_region: 'us-east-1',
        cloud_access_key: '',
        cloud_secret_key: '',
        chaos_latency_ms: 0,
        chaos_error_rate: 0,
        use_tor: false
    });

    const [saved, setSaved] = useState(false);

    // Audio settings state
    const [audioEnabled, setAudioEnabled] = useState(true);
    const [audioVolume, setAudioVolume] = useState(0.5);

    // Webhook state
    interface WebhookConfig {
        id: string;
        name: string;
        url: string;
        events: string[];
        template: string;
        enabled: boolean;
        max_retries: number;
    }
    const [webhooks, setWebhooks] = useState<WebhookConfig[]>([]);
    const [showWebhookModal, setShowWebhookModal] = useState(false);
    const [editingWebhook, setEditingWebhook] = useState<WebhookConfig | null>(null);

    useEffect(() => {
        if (isOpen) {
            loadSettings();
            setSaved(false);
        }
    }, [isOpen]);

    const loadSettings = async () => {
        try {
            const data = await invoke<SettingsData>('get_settings');
            // Ensure default values if backend lacks some fields
            setSettings({
                ...data,
                // Add defaults for fields that might be missing in older config
                cloud_endpoint: data.cloud_endpoint || '',
                cloud_bucket: data.cloud_bucket || '',
                cloud_region: data.cloud_region || 'us-east-1',
                cloud_access_key: data.cloud_access_key || '',
                cloud_secret_key: data.cloud_secret_key || '',
                chaos_latency_ms: data.chaos_latency_ms || 0,
                chaos_error_rate: data.chaos_error_rate || 0
            });

            // Load audio settings
            const enabled = await invoke<boolean>('get_audio_enabled');
            const volume = await invoke<number>('get_audio_volume');
            setAudioEnabled(enabled);
            setAudioVolume(volume);
        } catch (e) {
            console.error("Failed to load settings", e);
        }
    };

    const saveSettings = async () => {
        try {
            await invoke('save_settings', { settings });
            // Save audio settings
            await invoke('set_audio_enabled', { enabled: audioEnabled });
            await invoke('set_audio_volume', { volume: audioVolume });
            setSaved(true);
            setTimeout(() => setSaved(false), 2000);
        } catch (e) {
            console.error("Failed to save settings", e);
        }
    };

    const handleSelectDir = async () => {
        try {
            const selected = await invoke<string>('select_directory');
            if (selected) {
                setSettings({ ...settings, download_dir: selected });
            }
        } catch (e) {
            console.error("Failed to select directory", e);
        }
    };

    const handleTestSound = async (soundType: string) => {
        try {
            await invoke('play_test_sound', { soundType });
        } catch (e) {
            console.error("Failed to play test sound", e);
        }
    };

    // Webhook functions
    const loadWebhooks = async () => {
        try {
            const hooks = await invoke<WebhookConfig[]>('get_webhooks');
            setWebhooks(hooks);
        } catch (e) {
            console.error("Failed to load webhooks", e);
        }
    };

    const handleDeleteWebhook = async (id: string) => {
        try {
            await invoke('delete_webhook', { id });
            await loadWebhooks();
        } catch (e) {
            console.error("Failed to delete webhook", e);
        }
    };

    const handleTestWebhook = async (id: string) => {
        try {
            await invoke('test_webhook', { id });
        } catch (e) {
            console.error("Failed to test webhook", e);
        }
    };

    const handleToggleWebhook = async (webhook: WebhookConfig) => {
        try {
            await invoke('update_webhook', {
                id: webhook.id,
                config: { ...webhook, enabled: !webhook.enabled }
            });
            await loadWebhooks();
        } catch (e) {
            console.error("Failed to toggle webhook", e);
        }
    };

    // Load webhooks on open
    useEffect(() => {
        if (isOpen) {
            loadWebhooks();
        }
    }, [isOpen]);

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <motion.div
                className="relative w-full max-w-4xl h-full max-h-[85vh] bg-slate-900 border border-slate-700/50 rounded-2xl shadow-2xl flex flex-col overflow-hidden"
                initial={{ scale: 0.95, opacity: 0, y: 20 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.95, opacity: 0, y: 20 }}
            >
                {/* Header */}
                <div className="flex items-center justify-between px-8 py-5 border-b border-slate-700/50 bg-slate-900/50 backdrop-blur-md z-10">
                    <div className="flex items-center gap-3">
                        <div className="p-2 bg-blue-500/10 rounded-lg text-blue-400">
                            <Settings size={24} />
                        </div>
                        <h2 className="text-xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-white to-slate-400">
                            Settings
                        </h2>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-2 hover:bg-slate-800 rounded-lg transition-colors text-slate-400 hover:text-white"
                    >
                        <X size={24} />
                    </button>
                </div>

                {/* Content */}
                <div className="flex-1 overflow-y-auto custom-scrollbar p-8">
                    <div className="grid gap-10 max-w-3xl mx-auto">

                        {/* General Section */}
                        <div className="space-y-4">
                            <SectionHeader icon={Folder} title="Storage & Downloads" />

                            <div className="grid gap-6 md:grid-cols-2">
                                <div className="space-y-2 md:col-span-2">
                                    <label className="text-sm font-medium text-slate-400">Default Download Path</label>
                                    <div className="flex gap-3">
                                        <input
                                            type="text"
                                            value={settings.download_dir}
                                            readOnly
                                            className="flex-1 bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
                                        />
                                        <button
                                            onClick={handleSelectDir}
                                            className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors font-medium text-sm whitespace-nowrap"
                                        >
                                            Change Folder
                                        </button>
                                    </div>
                                </div>

                                <div className="space-y-2">
                                    <label className="text-sm font-medium text-slate-400">Concurrent Downloads</label>
                                    <input
                                        type="number"
                                        min="1" max="10"
                                        value={settings.max_concurrent_downloads}
                                        onChange={(e) => setSettings({ ...settings, max_concurrent_downloads: parseInt(e.target.value) || 1 })}
                                        className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-blue-500/50"
                                    />
                                </div>

                                <div className="space-y-2">
                                    <div className="flex justify-between items-center mb-1">
                                        <label className="text-sm font-medium text-slate-400">Speed Limit (KB/s)</label>
                                        <Toggle
                                            checked={settings.speed_limit_enabled}
                                            onChange={(val) => setSettings({ ...settings, speed_limit_enabled: val })}
                                        />
                                    </div>
                                    <input
                                        type="number"
                                        disabled={!settings.speed_limit_enabled}
                                        value={settings.speed_limit_rate}
                                        onChange={(e) => setSettings({ ...settings, speed_limit_rate: parseInt(e.target.value) || 0 })}
                                        className={`w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-blue-500/50 transition-opacity ${!settings.speed_limit_enabled ? 'opacity-50' : ''}`}
                                    />
                                </div>
                            </div>
                        </div>

                        {/* Network Section */}
                        <div className="space-y-4">
                            <SectionHeader icon={Globe} title="Network & Privacy" />

                            <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                <Toggle
                                    label="Enable Proxy"
                                    checked={settings.proxy_enabled}
                                    onChange={(val) => setSettings({ ...settings, proxy_enabled: val })}
                                />

                                {settings.proxy_enabled && (
                                    <motion.div
                                        initial={{ opacity: 0, height: 0 }}
                                        animate={{ opacity: 1, height: 'auto' }}
                                        className="pt-2"
                                    >
                                        <label className="text-sm font-medium text-slate-400 mb-2 block">Proxy URL</label>
                                        <input
                                            type="text"
                                            placeholder="http://127.0.0.1:8080"
                                            value={settings.proxy_url}
                                            onChange={(e) => setSettings({ ...settings, proxy_url: e.target.value })}
                                            className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
                                        />
                                    </motion.div>
                                )}

                                <div className="h-px bg-slate-700/50 my-2" />

                                <Toggle
                                    label="VPN Mode"
                                    checked={settings.vpn_mode}
                                    onChange={(val) => setSettings({ ...settings, vpn_mode: val })}
                                />
                            </div>

                            {/* Tor Section */}
                            <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                <div className="flex items-center justify-between">
                                    <div>
                                        <h4 className="text-slate-200 font-medium flex items-center gap-2">
                                            <Shield size={16} className="text-purple-400" />
                                            Tor Network
                                        </h4>
                                        <p className="text-sm text-slate-500">Route all traffic through Onion network</p>
                                    </div>
                                    <Toggle
                                        checked={settings.use_tor}
                                        onChange={async (val) => {
                                            setSettings({ ...settings, use_tor: val });
                                            if (val) {
                                                invoke('init_tor_network').catch(console.error);
                                            }
                                        }}
                                    />
                                </div>
                                {settings.use_tor && (
                                    <motion.div
                                        initial={{ opacity: 0, height: 0 }}
                                        animate={{ opacity: 1, height: 'auto' }}
                                        className="text-xs text-purple-300 bg-purple-900/20 p-3 rounded border border-purple-500/20 flex gap-2"
                                    >
                                        <Activity size={12} className="mt-0.5 shrink-0" />
                                        <span>
                                            <b>Privacy Mode Active:</b> Connection speeds will be significantly slower.
                                            Initial bootstrap may take 30-60 seconds.
                                        </span>
                                    </motion.div>
                                )}
                                <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <h4 className="text-slate-200 font-medium flex items-center gap-2">
                                                <Users size={16} className="text-green-400" />
                                                Team Sync (Shared Workspace)
                                            </h4>
                                            <p className="text-sm text-slate-500">Automatically sync downloads with local peers.</p>
                                        </div>
                                        <div className="flex gap-2">
                                            <input
                                                placeholder="Host IP (e.g. 192.168.1.5)"
                                                className="bg-slate-900 border border-slate-700 rounded px-3 py-1 text-xs text-slate-300 w-40"
                                                value={settings.last_sync_host || ''}
                                                onChange={(e) => setSettings({ ...settings, last_sync_host: e.target.value })}
                                            />
                                            <button
                                                onClick={() => {
                                                    if (settings.last_sync_host) {
                                                        invoke('join_workspace', { hostIp: settings.last_sync_host })
                                                            .then(() => alert("Connected to Workspace!"))
                                                            .catch(e => alert("Connection Failed: " + e));
                                                    }
                                                }}
                                                className="bg-green-600/20 hover:bg-green-600 text-green-400 hover:text-white px-3 py-1 rounded text-xs font-medium border border-green-500/20 transition-all"
                                            >
                                                Join
                                            </button>
                                        </div>
                                    </div>
                                    <div className="text-xs text-slate-500 font-mono bg-slate-900/50 p-2 rounded border border-slate-700/30">
                                        Your Host IP: <span className="text-slate-300 select-all">127.0.0.1 (Check LAN IP)</span> (Port: 8765)
                                    </div>
                                </div>
                            </div>

                            {/* Cloud Bridge */}
                            <div className="space-y-4">
                                <SectionHeader icon={Cloud} title="Cloud Bridge" />
                                <div className="bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                    <div className="flex items-center justify-between mb-4">
                                        <div>
                                            <h4 className="text-slate-200 font-medium">S3 Storage</h4>
                                            <p className="text-sm text-slate-500">Upload finished downloads to Cloud</p>
                                        </div>
                                        <Toggle checked={settings.cloud_enabled} onChange={(v) => setSettings({ ...settings, cloud_enabled: v })} />
                                    </div>

                                    {settings.cloud_enabled && (
                                        <motion.div
                                            initial={{ opacity: 0, height: 0 }}
                                            animate={{ opacity: 1, height: 'auto' }}
                                            className="grid gap-4 md:grid-cols-2 pt-2 border-t border-slate-700/30"
                                        >
                                            <div className="space-y-2 pt-4">
                                                <label className="text-xs font-semibold text-slate-500 uppercase">Endpoint</label>
                                                <input
                                                    className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                                                    value={settings.cloud_endpoint}
                                                    onChange={e => setSettings({ ...settings, cloud_endpoint: e.target.value })}
                                                    placeholder="s3.amazonaws.com"
                                                />
                                            </div>
                                            <div className="space-y-2 pt-4">
                                                <label className="text-xs font-semibold text-slate-500 uppercase">Bucket</label>
                                                <input
                                                    className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                                                    value={settings.cloud_bucket}
                                                    onChange={e => setSettings({ ...settings, cloud_bucket: e.target.value })}
                                                    placeholder="MyBucket"
                                                />
                                            </div>
                                            <div className="space-y-2">
                                                <label className="text-xs font-semibold text-slate-500 uppercase">Access Key</label>
                                                <input
                                                    className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                                                    value={settings.cloud_access_key}
                                                    onChange={e => setSettings({ ...settings, cloud_access_key: e.target.value })}
                                                    type="password"
                                                />
                                            </div>
                                            <div className="space-y-2">
                                                <label className="text-xs font-semibold text-slate-500 uppercase">Secret Key</label>
                                                <input
                                                    className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                                                    value={settings.cloud_secret_key}
                                                    onChange={e => setSettings({ ...settings, cloud_secret_key: e.target.value })}
                                                    type="password"
                                                />
                                            </div>
                                        </motion.div>
                                    )}
                                </div>
                            </div>

                            {/* Sound Settings */}
                            <div className="space-y-4">
                                <SectionHeader icon={Volume2} title="Sound Events" />
                                <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                    <Toggle
                                        label="Enable Sound Effects"
                                        checked={audioEnabled}
                                        onChange={(val) => setAudioEnabled(val)}
                                    />
                                    <p className="text-xs text-slate-500 leading-relaxed">
                                        Play audio notifications when downloads start, complete, or encounter errors.
                                    </p>

                                    {audioEnabled && (
                                        <motion.div
                                            initial={{ opacity: 0, height: 0 }}
                                            animate={{ opacity: 1, height: 'auto' }}
                                            className="space-y-4 pt-2"
                                        >
                                            {/* Volume Slider */}
                                            <div className="space-y-2">
                                                <div className="flex justify-between items-center">
                                                    <label className="text-sm font-medium text-slate-400">Volume</label>
                                                    <span className="text-xs text-slate-500 font-mono">{Math.round(audioVolume * 100)}%</span>
                                                </div>
                                                <input
                                                    type="range"
                                                    min="0"
                                                    max="1"
                                                    step="0.01"
                                                    value={audioVolume}
                                                    onChange={(e) => setAudioVolume(parseFloat(e.target.value))}
                                                    className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-blue-500"
                                                    style={{
                                                        background: `linear-gradient(to right, rgb(59, 130, 246) 0%, rgb(59, 130, 246) ${audioVolume * 100}%, rgb(51, 65, 85) ${audioVolume * 100}%, rgb(51, 65, 85) 100%)`
                                                    }}
                                                />
                                            </div>

                                            {/* Test Buttons */}
                                            <div className="space-y-2">
                                                <label className="text-sm font-medium text-slate-400">Test Sounds</label>
                                                <div className="grid grid-cols-3 gap-3">
                                                    <button
                                                        onClick={() => handleTestSound('success')}
                                                        className="px-4 py-2 bg-green-600/20 border border-green-600/30 hover:bg-green-600/30 text-green-400 rounded-lg transition-colors text-sm font-medium"
                                                    >
                                                        Success
                                                    </button>
                                                    <button
                                                        onClick={() => handleTestSound('error')}
                                                        className="px-4 py-2 bg-red-600/20 border border-red-600/30 hover:bg-red-600/30 text-red-400 rounded-lg transition-colors text-sm font-medium"
                                                    >
                                                        Error
                                                    </button>
                                                    <button
                                                        onClick={() => handleTestSound('start')}
                                                        className="px-4 py-2 bg-blue-600/20 border border-blue-600/30 hover:bg-blue-600/30 text-blue-400 rounded-lg transition-colors text-sm font-medium"
                                                    >
                                                        Start
                                                    </button>
                                                </div>
                                            </div>
                                        </motion.div>
                                    )}
                                </div>
                            </div>

                            {/* Webhooks */}
                            <div className="space-y-4">
                                <SectionHeader icon={Webhook} title="Webhooks" />
                                <div className="space-y-3">
                                    {webhooks.length === 0 && !showWebhookModal && (
                                        <p className="text-sm text-slate-500 text-center py-8">
                                            No webhooks configured. Click below to add one.
                                        </p>
                                    )}

                                    {webhooks.map((webhook) => (
                                        <div
                                            key={webhook.id}
                                            className="bg-slate-800/30 border border-slate-700/30 rounded-lg p-4 hover:border-slate-600/50 transition-all"
                                        >
                                            <div className="flex items-start justify-between gap-4">
                                                <div className="flex-1 min-w-0">
                                                    <div className="flex items-center gap-3 mb-2">
                                                        <h4 className="font-semibold text-white truncate">{webhook.name}</h4>
                                                        <span className={`px-2 py-0.5 text-xs rounded ${webhook.template === 'Discord' ? 'bg-indigo-500/20 text-indigo-400' :
                                                            webhook.template === 'Slack' ? 'bg-purple-500/20 text-purple-400' :
                                                                webhook.template === 'Plex' ? 'bg-orange-500/20 text-orange-400' :
                                                                    'bg-slate-500/20 text-slate-400'
                                                            }`}>
                                                            {webhook.template}
                                                        </span>
                                                        <span className={`px-2 py-0.5 text-xs rounded ${webhook.enabled ? 'bg-green-500/20 text-green-400' : 'bg-slate-500/20 text-slate-400'
                                                            }`}>
                                                            {webhook.enabled ? 'Enabled' : 'Disabled'}
                                                        </span>
                                                    </div>
                                                    <p className="text-xs text-slate-500 truncate mb-2">{webhook.url}</p>
                                                    <div className="flex gap-2 flex-wrap">
                                                        {webhook.events.map(event => (
                                                            <span key={event} className="text-xs px-2 py-1 bg-blue-500/10 text-blue-400 rounded border border-blue-500/20">
                                                                {event}
                                                            </span>
                                                        ))}
                                                    </div>
                                                </div>
                                                <div className="flex gap-2 flex-shrink-0">
                                                    <button
                                                        onClick={() => handleToggleWebhook(webhook)}
                                                        className={`p-2 rounded transition-colors ${webhook.enabled
                                                            ? 'bg-green-500/20 text-green-400 hover:bg-green-500/30'
                                                            : 'bg-slate-700 text-slate-400 hover:bg-slate-600'
                                                            }`}
                                                        title={webhook.enabled ? "Disable" : "Enable"}
                                                    >
                                                        {webhook.enabled ? '✓' : '○'}
                                                    </button>
                                                    <button
                                                        onClick={() => handleTestWebhook(webhook.id)}
                                                        className="p-2 bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 rounded transition-colors"
                                                        title="Test Webhook"
                                                    >
                                                        <PlayCircle className="w-4 h-4" />
                                                    </button>
                                                    <button
                                                        onClick={() => handleDeleteWebhook(webhook.id)}
                                                        className="p-2 bg-red-500/20 text-red-400 hover:bg-red-500/30 rounded transition-colors"
                                                        title="Delete"
                                                    >
                                                        <Trash2 className="w-4 h-4" />
                                                    </button>
                                                </div>
                                            </div>
                                        </div>
                                    ))}

                                    {!showWebhookModal && (
                                        <button
                                            onClick={() => {
                                                setEditingWebhook(null);
                                                setShowWebhookModal(true);
                                            }}
                                            className="w-full py-3 px-4 bg-blue-500/20 border border-blue-500/30 hover:bg-blue-500/30 text-blue-400 rounded-lg transition-colors flex items-center justify-center gap-2 font-medium"
                                        >
                                            <Plus className="w-5 h-5" />
                                            Add Webhook
                                        </button>
                                    )}

                                    {showWebhookModal && (
                                        <motion.div
                                            initial={{ opacity: 0, height: 0 }}
                                            animate={{ opacity: 1, height: 'auto' }}
                                            className="bg-slate-800/50 border border-slate-700/50 rounded-lg p-5 space-y-4"
                                        >
                                            <div className="flex justify-between items-center">
                                                <h3 className="font-semibold text-white">
                                                    {editingWebhook ? 'Edit Webhook' : 'Add New Webhook'}
                                                </h3>
                                                <button
                                                    onClick={() => setShowWebhookModal(false)}
                                                    className="text-slate-400 hover:text-white"
                                                >
                                                    <X className="w-5 h-5" />
                                                </button>
                                            </div>

                                            <div className="space-y-3">
                                                <div>
                                                    <label className="block text-sm font-medium text-slate-400 mb-1.5">Name</label>
                                                    <input
                                                        type="text"
                                                        placeholder="My Discord Webhook"
                                                        className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white focus:border-blue-500 focus:outline-none"
                                                        id="webhook-name"
                                                    />
                                                </div>

                                                <div>
                                                    <label className="block text-sm font-medium text-slate-400 mb-1.5">Webhook URL</label>
                                                    <input
                                                        type="url"
                                                        placeholder="https://discord.com/api/webhooks/..."
                                                        className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white font-mono text-sm focus:border-blue-500 focus:outline-none"
                                                        id="webhook-url"
                                                    />
                                                </div>

                                                <div>
                                                    <label className="block text-sm font-medium text-slate-400 mb-1.5">Template</label>
                                                    <select
                                                        className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white focus:border-blue-500 focus:outline-none"
                                                        id="webhook-template"
                                                    >
                                                        <option value="Discord">Discord</option>
                                                        <option value="Slack">Slack</option>
                                                        <option value="Plex">Plex</option>
                                                        <option value="Gotify">Gotify</option>
                                                        <option value="Custom">Custom (Raw JSON)</option>
                                                    </select>
                                                </div>

                                                <div>
                                                    <label className="block text-sm font-medium text-slate-400 mb-2">Events</label>
                                                    <div className="flex gap-3">
                                                        <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                                                            <input type="checkbox" className="rounded" id="event-start" defaultChecked />
                                                            Download Start
                                                        </label>
                                                        <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                                                            <input type="checkbox" className="rounded" id="event-complete" defaultChecked />
                                                            Download Complete
                                                        </label>
                                                        <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                                                            <input type="checkbox" className="rounded" id="event-error" defaultChecked />
                                                            Download Error
                                                        </label>
                                                    </div>
                                                </div>

                                                <div className="flex gap-3 pt-2">
                                                    <button
                                                        onClick={async () => {
                                                            const name = (document.getElementById('webhook-name') as HTMLInputElement).value;
                                                            const url = (document.getElementById('webhook-url') as HTMLInputElement).value;
                                                            const template = (document.getElementById('webhook-template') as HTMLSelectElement).value;
                                                            const events = [];
                                                            if ((document.getElementById('event-start') as HTMLInputElement).checked) events.push('DownloadStart');
                                                            if ((document.getElementById('event-complete') as HTMLInputElement).checked) events.push('DownloadComplete');
                                                            if ((document.getElementById('event-error') as HTMLInputElement).checked) events.push('DownloadError');

                                                            if (!name || !url || events.length === 0) {
                                                                alert('Please fill all fields and select at least one event');
                                                                return;
                                                            }

                                                            try {
                                                                await invoke('add_webhook', {
                                                                    config: {
                                                                        id: `webhook_${Date.now()}`,
                                                                        name,
                                                                        url,
                                                                        events,
                                                                        template,
                                                                        enabled: true,
                                                                        max_retries: 3
                                                                    }
                                                                });
                                                                setShowWebhookModal(false);
                                                                await loadWebhooks();
                                                            } catch (e) {
                                                                console.error('Failed to add webhook', e);
                                                            }
                                                        }}
                                                        className="flex-1 px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition-colors"
                                                    >
                                                        Save Webhook
                                                    </button>
                                                    <button
                                                        onClick={() => setShowWebhookModal(false)}
                                                        className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-300 rounded-lg font-medium transition-colors"
                                                    >
                                                        Cancel
                                                    </button>
                                                </div>
                                            </div>
                                        </motion.div>
                                    )}
                                </div>
                            </div>

                            {/* ChatOps Settings */}
                            <div className="bg-slate-800/50 rounded-xl p-6 border border-slate-700/50 backdrop-blur-sm">
                                <SectionHeader icon={MessageSquare} title="ChatOps (Telegram)" />

                                <div className="space-y-4">
                                    <Toggle
                                        label="Enable Telegram Bot"
                                        checked={settings.chatops_enabled || false}
                                        onChange={(val) => setSettings({ ...settings, chatops_enabled: val })}
                                    />

                                    {settings.chatops_enabled && (
                                        <motion.div
                                            initial={{ height: 0, opacity: 0 }}
                                            animate={{ height: 'auto', opacity: 1 }}
                                            className="space-y-3 pt-2"
                                        >
                                            <div>
                                                <label className="block text-xs font-medium text-slate-400 mb-1">Bot Token (from @BotFather)</label>
                                                <input
                                                    type="password"
                                                    value={settings.telegram_bot_token || ''}
                                                    onChange={(e) => setSettings({ ...settings, telegram_bot_token: e.target.value })}
                                                    className="w-full bg-slate-900/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-purple-500 transition-colors"
                                                    placeholder="123456789:ABCdefGHIjklMNOpqrs..."
                                                />
                                            </div>
                                            <div>
                                                <label className="block text-xs font-medium text-slate-400 mb-1">Chat ID (Optional - Auto-detected)</label>
                                                <input
                                                    type="text"
                                                    value={settings.telegram_chat_id || ''}
                                                    onChange={(e) => setSettings({ ...settings, telegram_chat_id: e.target.value })}
                                                    className="w-full bg-slate-900/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-purple-500 transition-colors"
                                                    placeholder="Automatically filled after first message"
                                                />
                                            </div>
                                            <div className="text-xs text-slate-500 bg-slate-900/30 p-3 rounded-lg border border-slate-700/30">
                                                <p className="font-semibold mb-1">How to use:</p>
                                                <ol className="list-decimal list-inside space-y-1">
                                                    <li>Create a bot with <a href="https://t.me/BotFather" target="_blank" className="text-purple-400 hover:underline">@BotFather</a></li>
                                                    <li>Paste the token above and enable it</li>
                                                    <li>Send <code className="bg-slate-800 px-1 rounded">/start</code> to your bot</li>
                                                </ol>
                                            </div>
                                        </motion.div>
                                    )}
                                </div>
                            </div>

                            {/* Archive Extraction */}
                            <div className="space-y-4">
                                <SectionHeader icon={Activity} title="Archive Extraction" />
                                <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
                                    <Toggle
                                        label="Auto-Extract Archives"
                                        checked={settings.auto_extract_archives || false}
                                        onChange={(val) => setSettings({ ...settings, auto_extract_archives: val })}
                                    />
                                    <p className="text-xs text-slate-500 leading-relaxed">
                                        Automatically extract RAR/ZIP archives when downloads complete. Requires unrar or WinRAR installed on your system.
                                    </p>

                                    {settings.auto_extract_archives && (
                                        <motion.div
                                            initial={{ opacity: 0, height: 0 }}
                                            animate={{ opacity: 1, height: 'auto' }}
                                            className="space-y-3 pt-2 border-t border-slate-700/30"
                                        >
                                            <Toggle
                                                label="Cleanup After Extraction"
                                                checked={settings.cleanup_archives_after_extract || false}
                                                onChange={(val) => setSettings({ ...settings, cleanup_archives_after_extract: val })}
                                            />
                                            <p className="text-xs text-slate-500 leading-relaxed">
                                                ⚠️  <strong className="text-amber-400">Warning:</strong> Archives will be permanently deleted after successful extraction.
                                            </p>
                                        </motion.div>
                                    )}
                                </div>
                            </div>

                            {/* Chaos Mode */}
                            <div className="space-y-4">
                                <SectionHeader icon={Activity} title="Advanced" />
                                <div className={`p-5 rounded-xl border transition-all ${settings.chaos_mode ? 'bg-red-500/10 border-red-500/30' : 'bg-slate-800/20 border-slate-700/30'}`}>
                                    <Toggle
                                        label="Chaos Mode (Experimental)"
                                        checked={settings.chaos_mode}
                                        onChange={(val) => setSettings({ ...settings, chaos_mode: val })}
                                    />
                                    <p className="text-xs text-slate-500 mt-2 leading-relaxed">
                                        Enables experimental parallel fetching algorithms. May use significant bandwidth and CPU. Use with caution.
                                    </p>

                                    {settings.chaos_mode && (
                                        <div className="mt-4 grid gap-4 grid-cols-2">
                                            <div className="space-y-1">
                                                <label className="text-xs text-red-400">Latency (ms)</label>
                                                <input
                                                    type="number"
                                                    className="w-full bg-slate-900 border border-red-900/30 rounded px-2 py-1 text-red-200 text-sm"
                                                    value={settings.chaos_latency_ms}
                                                    onChange={e => setSettings({ ...settings, chaos_latency_ms: parseInt(e.target.value) || 0 })}
                                                />
                                            </div>
                                            <div className="space-y-1">
                                                <label className="text-xs text-red-400">Error Rate (%)</label>
                                                <input
                                                    type="number"
                                                    className="w-full bg-slate-900 border border-red-900/30 rounded px-2 py-1 text-red-200 text-sm"
                                                    value={settings.chaos_error_rate}
                                                    onChange={e => setSettings({ ...settings, chaos_error_rate: parseInt(e.target.value) || 0 })}
                                                />
                                            </div>
                                        </div>
                                    )}
                                </div>
                            </div>

                        </div>
                    </div>
                </div>

                {/* Footer */}
                <div className="px-8 py-5 border-t border-slate-700/50 bg-slate-900/50 backdrop-blur-md flex justify-end gap-4 z-10">
                    <button
                        onClick={onClose}
                        className="px-6 py-2.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors font-medium"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={saveSettings}
                        className="px-6 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg shadow-lg shadow-blue-900/20 font-bold flex items-center gap-2 transition-all"
                    >
                        <Save size={18} />
                        {saved ? 'Saved!' : 'Save Changes'}
                    </button>
                </div>
            </motion.div>
        </div>
    );
};
