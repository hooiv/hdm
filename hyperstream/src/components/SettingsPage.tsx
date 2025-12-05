import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface SettingsData {
    download_dir: string;
    segments: number;
    speed_limit_kbps: number;
    clipboard_monitor: boolean;
    auto_start_extension: boolean;
    use_category_folders: boolean;
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
    });
    const [saving, setSaving] = useState(false);
    const [saved, setSaved] = useState(false);

    useEffect(() => {
        loadSettings();
    }, []);

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
        <div className="settings-overlay">
            <div className="settings-page">
                <div className="settings-header">
                    <h2>⚙️ Settings</h2>
                    <button className="close-btn" onClick={onClose}>✕</button>
                </div>

                <div className="settings-content">
                    {/* Download Directory */}
                    <div className="setting-group">
                        <label>Download Directory</label>
                        <input
                            type="text"
                            value={settings.download_dir}
                            onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })}
                            placeholder="C:\Users\You\Downloads"
                        />
                    </div>

                    {/* Number of Segments */}
                    <div className="setting-group">
                        <label>Concurrent Segments per Download</label>
                        <div className="slider-container">
                            <input
                                type="range"
                                min="1"
                                max="32"
                                value={settings.segments}
                                onChange={(e) => setSettings({ ...settings, segments: parseInt(e.target.value) })}
                            />
                            <span className="slider-value">{settings.segments}</span>
                        </div>
                        <small>More segments = faster downloads (if server supports it)</small>
                    </div>

                    {/* Speed Limit */}
                    <div className="setting-group">
                        <label>Speed Limit (KB/s)</label>
                        <div className="speed-limit-container">
                            <input
                                type="number"
                                min="0"
                                value={settings.speed_limit_kbps}
                                onChange={(e) => handleSpeedChange(e.target.value)}
                                placeholder="0"
                            />
                            <span className="speed-unit">KB/s</span>
                        </div>
                        <small>Set to 0 for unlimited speed</small>
                    </div>

                    {/* Category Folders */}
                    <div className="setting-group toggles">
                        <div className="toggle-row">
                            <label>📁 Auto-sort by Category</label>
                            <label className="toggle">
                                <input
                                    type="checkbox"
                                    checked={settings.use_category_folders}
                                    onChange={(e) => setSettings({ ...settings, use_category_folders: e.target.checked })}
                                />
                                <span className="slider"></span>
                            </label>
                        </div>
                        <small>Organize downloads into Video, Audio, Archives, etc. folders</small>
                    </div>

                    {/* Clipboard Monitor */}
                    <div className="setting-group toggles">
                        <div className="toggle-row">
                            <label>📋 Clipboard Monitoring</label>
                            <label className="toggle">
                                <input
                                    type="checkbox"
                                    checked={settings.clipboard_monitor}
                                    onChange={(e) => setSettings({ ...settings, clipboard_monitor: e.target.checked })}
                                />
                                <span className="slider"></span>
                            </label>
                        </div>
                        <small>Auto-detect URLs copied to clipboard</small>
                    </div>

                    <div className="setting-group toggles">
                        <div className="toggle-row">
                            <label>🌐 Auto-start Extension Downloads</label>
                            <label className="toggle">
                                <input
                                    type="checkbox"
                                    checked={settings.auto_start_extension}
                                    onChange={(e) => setSettings({ ...settings, auto_start_extension: e.target.checked })}
                                />
                                <span className="slider"></span>
                            </label>
                        </div>
                        <small>Automatically start downloads from browser extension</small>
                    </div>
                </div>

                <div className="settings-footer">
                    <button className="cancel-btn" onClick={onClose}>Cancel</button>
                    <button className="save-btn" onClick={saveSettings} disabled={saving}>
                        {saving ? 'Saving...' : saved ? '✓ Saved!' : 'Save Settings'}
                    </button>
                </div>
            </div>
        </div>
    );
};
