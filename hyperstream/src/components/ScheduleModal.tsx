import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ScheduleModalProps {
    isOpen: boolean;
    onClose: () => void;
}

const generateId = () => `sched_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

export const ScheduleModal: React.FC<ScheduleModalProps> = ({ isOpen, onClose }) => {
    const [url, setUrl] = useState('');
    const [filename, setFilename] = useState('');
    const [date, setDate] = useState('');
    const [time, setTime] = useState('');
    const [error, setError] = useState('');

    if (!isOpen) return null;

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

            setUrl('');
            setFilename('');
            setDate('');
            setTime('');
            setError('');
            onClose();
        } catch (err) {
            setError('Failed to schedule download');
            console.error(err);
        }
    };

    // Get minimum date (today)
    const today = new Date().toISOString().split('T')[0];

    return (
        <div className="modal-overlay">
            <div className="modal schedule-modal">
                <h2>⏰ Schedule Download</h2>

                {error && <div className="error-msg">{error}</div>}

                <div className="form-group">
                    <label>Download URL</label>
                    <input
                        type="url"
                        placeholder="https://example.com/file.zip"
                        value={url}
                        onChange={(e) => handleUrlChange(e.target.value)}
                    />
                </div>

                <div className="form-group">
                    <label>Save as</label>
                    <input
                        type="text"
                        placeholder="filename.zip"
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

                <div className="modal-actions">
                    <button className="cancel-btn" onClick={onClose}>Cancel</button>
                    <button className="start-btn" onClick={handleSchedule}>
                        📅 Schedule
                    </button>
                </div>
            </div>
        </div>
    );
};
