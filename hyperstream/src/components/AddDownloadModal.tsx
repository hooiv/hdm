import React, { useState } from 'react';

interface AddDownloadModalProps {
    isOpen: boolean;
    onClose: () => void;
    onStart: (url: string, filename: string) => void;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({ isOpen, onClose, onStart }) => {
    const [url, setUrl] = useState('');
    const [filename, setFilename] = useState('');

    if (!isOpen) return null;

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (url && filename) {
            onStart(url, filename);
            setUrl('');
            setFilename('');
            onClose();
        }
    };

    return (
        <div className="modal-overlay">
            <div className="modal">
                <h2>Add Download</h2>
                <form onSubmit={handleSubmit}>
                    <div className="form-group">
                        <label>URL</label>
                        <input
                            type="text"
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                            placeholder="https://example.com/file.zip"
                            autoFocus
                        />
                    </div>
                    <div className="form-group">
                        <label>Filename</label>
                        <input
                            type="text"
                            value={filename}
                            onChange={(e) => setFilename(e.target.value)}
                            placeholder="file.zip"
                        />
                    </div>
                    <div className="modal-actions">
                        <button type="button" onClick={onClose} className="cancel-btn">Cancel</button>
                        <button type="submit" className="start-btn">Start Download</button>
                    </div>
                </form>
            </div>
        </div>
    );
};
