import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LanPairing } from './LanPairing';
import { motion } from 'framer-motion';
import { Settings, X, Folder, Layers, Zap, Wifi, Clipboard, Activity } from 'lucide-react';

interface SettingsData {
    download_dir: string;
    segments: number;
    speed_limit_kbps: number;
    clipboard_monitor: boolean;
    auto_start_extension: boolean;
    use_category_folders: boolean;
    ja3_enabled: boolean;
    min_threads: number;
    max_threads: number;
}

interface SettingsPageProps {
    onClose: () => void;
}

export const SettingsPage: React.FC<SettingsPageProps> = ({ onClose }) => {
    const [settings, setSettings] = useState<SettingsData>({
        download_dir: '',
        segments: 8,
        speed_limit_kbps: 0,
        clipboard_monitor: false,
        auto_start_extension: true,
        use_category_folders: true,
        ja3_enabled: true,
        min_threads: 2,
        max_threads: 32,
    });
    const [saving, setSaving] = useState(false);
    const [saved, setSaved] = useState(false);

    // Chaos Mode State
    const [chaosEnabled, setChaosEnabled] = useState(false);
    const [chaosLatency, setChaosLatency] = useState(0);
    const [chaosErrorRate, setChaosErrorRate] = useState(0);

    // Plugin State
    interface PluginMetadata {
        name: string;
        version: string;
        domains: string[];
    }
    const [plugins, setPlugins] = useState<PluginMetadata[]>([]);
    const [loadingPlugins, setLoadingPlugins] = useState(false);

    useEffect(() => {
        loadSettings();
        loadChaosConfig();
        loadPlugins();
    }, []);

    const loadChaosConfig = async () => {
        try {
            const config: any = await invoke('get_chaos_config');
            setChaosEnabled(config.enabled);
            setChaosLatency(config.latency_ms);
            setChaosErrorRate(config.error_rate);
        } catch (e) {
            console.error("Failed to load chaos config", e);
        }
    };

    const updateChaos = async (enabled: boolean, latency: number, errorRate: number) => {
        setChaosEnabled(enabled);
        setChaosLatency(latency);
        setChaosErrorRate(errorRate);
        invoke('set_chaos_config', { latencyMs: latency, errorRate: errorRate, enabled }).catch(console.error);
    };

    const loadPlugins = async () => {
        setLoadingPlugins(true);
        try {
            const list = await invoke<PluginMetadata[]>('get_all_plugins');
            setPlugins(list);
        } catch (e) {
            console.error("Failed to load plugins", e);
        }
        setLoadingPlugins(false);
    };

    const handleReloadPlugins = async () => {
        setLoadingPlugins(true);
        try {
            await invoke('reload_plugins');
            await loadPlugins();
        } catch (e) {
            console.error("Failed to reload plugins", e);
        }
        setLoadingPlugins(false);
    };

    const loadSettings = async () => {
        try {
            const data: SettingsData = await invoke('get_settings');
            setSettings(data);
        } catch (error) {
            console.error('Failed to load settings:', error);
        }
    };

    const saveSettings = async () => {
        setSaving(true);
        try {
            await invoke('save_settings', { newSettings: settings });
            setSaved(true);
            setTimeout(() => setSaved(false), 2000);
        } catch (error) {
            console.error('Failed to save settings:', error);
        }
        setSaving(false);
    };

    const handleSpeedChange = (value: string) => {
        const kbps = parseInt(value) || 0;
        setSettings({ ...settings, speed_limit_kbps: kbps });
    };

    return (
        <motion.div
            className="settings-overlay glass-modal-overlay"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
        >
            <motion.div
                className="settings-page glass-panel"
                initial={{ scale: 0.9, opacity: 0, y: 20 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.9, opacity: 0, y: 20 }}
                transition={{ type: "spring", stiffness: 300, damping: 25 }}
                onClick={(e) => e.stopPropagation()}
            >
                <div className="settings-header">
                    <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                        <Settings size={28} className="text-accent" />
                        <h2>Settings</h2>
                    </div>
                    <button className="close-btn action-icon-btn" onClick={onClose}>
                        <X size={24} />
                    </button>
                </div>

                <div className="settings-content custom-scrollbar">
                    {/* General Settings */}
                    <div className="setting-section">
                        <h3><Folder size={18} /> Storage & Network</h3>
                        <div className="setting-group">
                            <label>Download Directory</label>
                            <input
                                type="text"
                                value={settings.download_dir}
                                onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })}
                                placeholder="C:\Users\You\Downloads"
                                className="glass-input"
                            />
                        </div>

                        <div className="setting-group">
                            <label>Concurrent Segments: {settings.segments}</label>
                            <div className="slider-container">
                                <input
                                    type="range"
                                    min="1"
                                    max="32"
                                    value={settings.segments}
                                    onChange={(e) => setSettings({ ...settings, segments: parseInt(e.target.value) })}
                                    className="accent-slider"
                                />
                            </div>
                        </div>

                        <div className="setting-group">
                            <label>Speed Limit (KB/s)</label>
                            <div className="input-with-unit">
                                <input
                                    type="number"
                                    min="0"
                                    value={settings.speed_limit_kbps}
                                    onChange={(e) => handleSpeedChange(e.target.value)}
                                    placeholder="0 (Unlimited)"
                                    className="glass-input"
                                />
                                <span>KB/s</span>
                            </div>
                        </div>
                    </div>

                    {/* Automation */}
                    <div className="setting-section">
                        <h3><Zap size={18} /> Automation</h3>
                        <div className="setting-group toggles">
                            <div className="toggle-row">
                                <span>Auto-sort by Category</span>
                                <label className="toggle">
                                    <input
                                        type="checkbox"
                                        checked={settings.use_category_folders}
                                        onChange={(e) => setSettings({ ...settings, use_category_folders: e.target.checked })}
                                    />
                                    <span className="slider round"></span>
                                </label>
                            </div>
                        </div>

                        <div className="setting-group toggles">
                            <div className="toggle-row">
                                <span><Clipboard size={14} style={{ display: 'inline', marginRight: 5 }} /> Clipboard Monitor</span>
                                <label className="toggle">
                                    <input
                                        type="checkbox"
                                        checked={settings.clipboard_monitor}
                                        onChange={(e) => setSettings({ ...settings, clipboard_monitor: e.target.checked })}
                                    />
                                    <span className="slider round"></span>
                                </label>
                            </div>
                        </div>

                        <div className="setting-group toggles">
                            <div className="toggle-row">
                                <span>Auto-start Extension Downloads</span>
                                <label className="toggle">
                                    <input
                                        type="checkbox"
                                        checked={settings.auto_start_extension}
                                        onChange={(e) => setSettings({ ...settings, auto_start_extension: e.target.checked })}
                                    />
                                    <span className="slider round"></span>
                                </label>
                            </div>
                        </div>
                    </div>

                    <div className="setting-section">
                        <h3><Wifi size={18} /> Connectivity</h3>
                        <div className="setting-group">
                            <LanPairing />
                        </div>
                    </div>

                    {/* Plugins */}
                    <div className="setting-section">
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '10px' }}>
                            <h3><Layers size={18} /> Plugins</h3>
                            <button className="small-action-btn" onClick={handleReloadPlugins} disabled={loadingPlugins}>
                                {loadingPlugins ? 'Reloading...' : 'Reload Plugins'}
                            </button>
                        </div>
                        {plugins.length === 0 ? (
                            <div className="empty-plugins">No plugins found in <code>plugins/</code></div>
                        ) : (
                            <div className="plugin-list">
                                {plugins.map((p, i) => (
                                    <div key={i} className="plugin-item glass-card">
                                        <div className="plugin-header">
                                            <span className="plugin-name">{p.name}</span>
                                            <span className="plugin-version">v{p.version}</span>
                                        </div>
                                        <div className="plugin-domains">
                                            {p.domains.join(', ')}
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* Chaos Mode (Network Simulator) */}
                    <div className="section-divider"></div>
                    <div className="setting-section chaos-section" style={{ borderColor: chaosEnabled ? '#ef4444' : 'transparent', borderLeftWidth: chaosEnabled ? '4px' : '0' }}>
                        <div className="setting-group">
                            <div className="toggle-row">
                                <span style={{ color: '#ef4444', fontWeight: 'bold', display: 'flex', alignItems: 'center', gap: '5px' }}>
                                    <Activity size={18} /> Chaos Network Simulator
                                </span>
                                <label className="toggle">
                                    <input
                                        type="checkbox"
                                        checked={chaosEnabled}
                                        onChange={(e) => updateChaos(e.target.checked, chaosLatency, chaosErrorRate)}
                                    />
                                    <span className="slider round" style={{ backgroundColor: chaosEnabled ? '#ef4444' : '#334155' }}></span>
                                </label>
                            </div>
                            <small className="warning-text">Intentionally degrades network performance for testing.</small>
                        </div>

                        {chaosEnabled && (
                            <motion.div
                                className="chaos-controls"
                                initial={{ height: 0, opacity: 0 }}
                                animate={{ height: 'auto', opacity: 1 }}
                            >
                                <div className="setting-group">
                                    <label>Latency Injection ({chaosLatency}ms)</label>
                                    <div className="slider-container">
                                        <input
                                            type="range"
                                            min="0"
                                            max="5000"
                                            step="100"
                                            value={chaosLatency}
                                            onChange={(e) => updateChaos(true, parseInt(e.target.value), chaosErrorRate)}
                                            className="danger-slider"
                                        />
                                    </div>
                                </div>
                                <div className="setting-group">
                                    <label>Error Rate ({chaosErrorRate}%)</label>
                                    <div className="slider-container">
                                        <input
                                            type="range"
                                            min="0"
                                            max="100"
                                            value={chaosErrorRate}
                                            onChange={(e) => updateChaos(true, chaosLatency, parseInt(e.target.value))}
                                            className="danger-slider"
                                        />
                                    </div>
                                </div>
                            </motion.div>
                        )}
                    </div>
                </div>

                <div className="settings-footer">
                    <button className="cancel-btn" onClick={onClose}>Cancel</button>
                    <button className="save-btn" onClick={saveSettings} disabled={saving}>
                        {saving ? 'Saving...' : saved ? '✓ Saved!' : 'Save Settings'}
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
};
