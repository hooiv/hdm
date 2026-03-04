import React, { useState, useEffect } from 'react';

interface AddTorrentModalProps {
    isOpen: boolean;
    onClose: () => void;
    onAdd: (magnet: string) => void;
}

export const AddTorrentModal: React.FC<AddTorrentModalProps> = ({ isOpen, onClose, onAdd }) => {
    const [magnetLink, setMagnetLink] = useState('');

    // Reset state when modal opens
    useEffect(() => {
        if (isOpen) setMagnetLink('');
    }, [isOpen]);

    if (!isOpen) return null;

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        const trimmed = magnetLink.trim();
        if (trimmed && (trimmed.startsWith('magnet:?') || trimmed.startsWith('http://') || trimmed.startsWith('https://'))) {
            onAdd(trimmed);
            setMagnetLink('');
            onClose();
        }
    };

    return (
        <div className="modal-overlay" onClick={onClose}
            role="dialog" aria-modal="true" aria-labelledby="torrent-modal-title"
            onKeyDown={e => e.key === 'Escape' && onClose()}
            style={{
            position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
            background: 'rgba(0,0,0,0.7)', display: 'flex', alignItems: 'center', justifyContent: 'center',
            zIndex: 1000, backdropFilter: 'blur(5px)'
        }}>
            <div className="modal-content" onClick={e => e.stopPropagation()} style={{
                background: '#1e293b', padding: '24px', borderRadius: '12px',
                width: '500px', border: '1px solid #334155', color: '#f1f5f9'
            }}>
                <h2 id="torrent-modal-title" style={{ marginBottom: '16px', fontSize: '1.25rem' }}>Add Torrent</h2>
                <form onSubmit={handleSubmit}>
                    <div style={{ marginBottom: '16px' }}>
                        <label style={{ display: 'block', marginBottom: '8px', fontSize: '0.9rem', color: '#94a3b8' }}>
                            Magnet Link / URL
                        </label>
                        <input
                            type="text"
                            value={magnetLink}
                            onChange={e => setMagnetLink(e.target.value)}
                            placeholder="magnet:?xt=urn:btih:..."
                            style={{
                                width: '100%', padding: '10px', borderRadius: '6px',
                                background: '#0f172a', border: '1px solid #334155',
                                color: 'white', fontFamily: 'monospace'
                            }}
                            autoFocus
                        />
                    </div>

                    <div className="button-group" style={{ display: 'flex', justifyContent: 'flex-end', gap: '10px' }}>
                        <button type="button" onClick={onClose} style={{
                            padding: '8px 16px', borderRadius: '6px', cursor: 'pointer',
                            background: 'transparent', border: '1px solid #475569', color: '#cbd5e1'
                        }}>
                            Cancel
                        </button>
                        <button type="submit" disabled={!magnetLink.trim()} style={{
                            padding: '8px 16px', borderRadius: '6px', cursor: magnetLink.trim() ? 'pointer' : 'not-allowed',
                            background: magnetLink.trim() ? '#3b82f6' : '#475569', border: 'none', color: 'white', fontWeight: 600,
                            opacity: magnetLink.trim() ? 1 : 0.6
                        }}>
                            Add Download
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
};
