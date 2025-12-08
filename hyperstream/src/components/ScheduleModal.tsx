import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ScheduleModalProps {
    isOpen: boolean;
    onClose: () => void;
}

interface ScheduledDownload {
    id: string;
    url: string;
    filename: string;
    scheduled_time: string;
    status: string;
}

interface TimeInfo {
    hour: number;
    minute: number;
    second: number;
    is_quiet_hours: boolean;
}

const generateId = () => `sched_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

export const ScheduleModal: React.FC<ScheduleModalProps> = ({ isOpen, onClose }) => {
    const [activeTab, setActiveTab] = useState<'list' | 'add'>('list');
    const [downloads, setDownloads] = useState<ScheduledDownload[]>([]);
    const [timeInfo, setTimeInfo] = useState<TimeInfo | null>(null);

    // Form State
    const [url, setUrl] = useState('');
    const [filename, setFilename] = useState('');
    const [date, setDate] = useState('');
    const [time, setTime] = useState('');
    const [error, setError] = useState('');

    useEffect(() => {
        if (isOpen) {
            refreshData();
            const interval = setInterval(refreshData, 10000);
            return () => clearInterval(interval);
        }
    }, [isOpen]);

    const refreshData = async () => {
        try {
            const list = await invoke<ScheduledDownload[]>('get_scheduled_downloads');
            const info = await invoke<TimeInfo>('get_time_info');
            setDownloads(list);
            setTimeInfo(info);
        } catch (err) {
            console.error('Failed to fetch schedule data:', err);
        }
    };

    const handleUrlChange = (newUrl: string) => {
        setUrl(newUrl);
        if (!filename && newUrl) {
            const extracted = newUrl.split('/').pop()?.split('?')[0] || '';
            setFilename(extracted);
        }
    };

    const handleSchedule = async () => {
        if (!url || !date || !time) {
            setError('Please fill in all fields');
            return;
        }

        try {
            const scheduledTime = new Date(`${date}T${time}`).toISOString();
            const id = generateId();

            await invoke('schedule_download', {
                id,
                url,
                filename: filename || 'download',
                scheduledTime
            });

            // Reset form
            setUrl('');
            setFilename('');
            setError('');
            setActiveTab('list'); // Switch to list view
            refreshData();
        } catch (err) {
            setError('Failed to schedule download');
            console.error(err);
        }
    };

    const handleForceStart = async (id: string) => {
        try {
            await invoke('force_start_scheduled_download', { id });
            await refreshData();
        } catch (err) {
            console.error('Failed to force start:', err);
        }
    };

    const handleRemove = async (id: string) => {
        try {
            await invoke('remove_scheduled_download', { id });
            await refreshData();
        } catch (err) {
            console.error('Failed to remove schedule:', err);
        }
    };

    if (!isOpen) return null;

    const today = new Date().toISOString().split('T')[0];
    const pendingDownloads = downloads.filter(d => d.status === 'pending');

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="modal-content schedule-modal" onClick={e => e.stopPropagation()}>
                <div className="modal-header">
                    <h3>📅 Scheduler</h3>
                    <button className="close-btn" onClick={onClose}>✕</button>
                </div>

                <div className="modal-tabs">
                    <button
                        className={`tab-btn ${activeTab === 'list' ? 'active' : ''}`}
                        onClick={() => setActiveTab('list')}
                    >
                        📋 Upcoming ({pendingDownloads.length})
                    </button>
                    <button
                        className={`tab-btn ${activeTab === 'add' ? 'active' : ''}`}
                        onClick={() => setActiveTab('add')}
                    >
                        ➕ Add New
                    </button>
                </div>

                <div className="modal-body">
                    {timeInfo && (
                        <div className="time-info-banner">
                            <span className="current-time">
                                Current Time: {timeInfo.hour.toString().padStart(2, '0')}:{timeInfo.minute.toString().padStart(2, '0')}
                            </span>
                            {timeInfo.is_quiet_hours ? (
                                <span className="quiet-badge">🌙 Quiet Hours Active</span>
                            ) : (
                                <span className="active-badge">☀️ Active Period</span>
                            )}
                        </div>
                    )}

                    {activeTab === 'list' ? (
                        <div className="schedule-list">
                            {pendingDownloads.length === 0 ? (
                                <div className="empty-state">No pending scheduled downloads.</div>
                            ) : (
                                pendingDownloads.map(item => (
                                    <div key={item.id} className="schedule-item">
                                        <div className="info">
                                            <div className="filename" title={item.filename}>{item.filename}</div>
                                            <div className="time">
                                                ⏰ {new Date(item.scheduled_time).toLocaleString()}
                                            </div>
                                        </div>
                                        <div className="actions">
                                            <button
                                                className="action-btn start-btn"
                                                onClick={() => handleForceStart(item.id)}
                                                title="Start Now"
                                            >
                                                ▶
                                            </button>
                                            <button
                                                className="action-btn delete-btn"
                                                onClick={() => handleRemove(item.id)}
                                                title="Cancel Schedule"
                                            >
                                                ✕
                                            </button>
                                        </div>
                                    </div>
                                ))
                            )}
                        </div>
                    ) : (
                        <div className="schedule-form">
                            {error && <div className="error-msg">{error}</div>}
                            <div className="form-group">
                                <label>URL</label>
                                <input
                                    type="url"
                                    placeholder="https://"
                                    value={url}
                                    onChange={(e) => handleUrlChange(e.target.value)}
                                />
                            </div>
                            <div className="form-group">
                                <label>Filename</label>
                                <input
                                    type="text"
                                    placeholder="file.zip"
                                    value={filename}
                                    onChange={(e) => setFilename(e.target.value)}
                                />
                            </div>
                            <div className="schedule-time-row">
                                <div className="form-group">
                                    <label>Date</label>
                                    <input
                                        type="date"
                                        min={today}
                                        value={date}
                                        onChange={(e) => setDate(e.target.value)}
                                    />
                                </div>
                                <div className="form-group">
                                    <label>Time</label>
                                    <input
                                        type="time"
                                        value={time}
                                        onChange={(e) => setTime(e.target.value)}
                                    />
                                </div>
                            </div>
                            <button className="start-btn full-width" onClick={handleSchedule}>
                                Schedule Download
                            </button>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
};

