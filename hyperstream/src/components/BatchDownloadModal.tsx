import React, { useState } from 'react';

interface LinkItem {
    url: string;
    filename: string;
    size?: string;
    selected: boolean;
}

interface BatchDownloadModalProps {
    isOpen: boolean;
    links: Array<{ url: string; filename: string }>;
    onClose: () => void;
    onDownload: (links: Array<{ url: string; filename: string }>) => void;
}

// Get file extension icon
const getExtIcon = (filename: string): string => {
    const ext = filename.split('.').pop()?.toLowerCase() || '';
    const icons: Record<string, string> = {
        zip: '📦', rar: '📦', '7z': '📦', tar: '📦', gz: '📦',
        mp4: '🎬', mkv: '🎬', avi: '🎬', mov: '🎬', webm: '🎬',
        mp3: '🎵', flac: '🎵', wav: '🎵', aac: '🎵',
        pdf: '📄', doc: '📄', docx: '📄', xls: '📊', xlsx: '📊',
        exe: '⚙️', msi: '⚙️', dmg: '⚙️',
        iso: '💿', img: '💿',
        jpg: '🖼️', jpeg: '🖼️', png: '🖼️', gif: '🖼️',
    };
    return icons[ext] || '📄';
};

export const BatchDownloadModal: React.FC<BatchDownloadModalProps> = ({
    isOpen,
    links,
    onClose,
    onDownload,
}) => {
    const [items, setItems] = useState<LinkItem[]>(
        links.map(l => ({ ...l, selected: true }))
    );
    const [filter, setFilter] = useState('');
    const [typeFilter, setTypeFilter] = useState<string>('all');

    if (!isOpen) return null;

    const filteredItems = items.filter(item => {
        const matchesSearch = item.filename.toLowerCase().includes(filter.toLowerCase()) ||
            item.url.toLowerCase().includes(filter.toLowerCase());

        if (typeFilter === 'all') return matchesSearch;

        const ext = item.filename.split('.').pop()?.toLowerCase() || '';
        const typeMap: Record<string, string[]> = {
            video: ['mp4', 'mkv', 'avi', 'mov', 'webm'],
            audio: ['mp3', 'flac', 'wav', 'aac'],
            archive: ['zip', 'rar', '7z', 'tar', 'gz'],
            document: ['pdf', 'doc', 'docx', 'xls', 'xlsx'],
            program: ['exe', 'msi', 'dmg'],
            image: ['jpg', 'jpeg', 'png', 'gif', 'webp'],
        };

        return matchesSearch && typeMap[typeFilter]?.includes(ext);
    });

    const selectedCount = items.filter(i => i.selected).length;

    const toggleAll = (selected: boolean) => {
        setItems(items.map(item => {
            const isVisible = filteredItems.some(f => f.url === item.url);
            return isVisible ? { ...item, selected } : item;
        }));
    };

    const toggleItem = (url: string) => {
        setItems(items.map(item =>
            item.url === url ? { ...item, selected: !item.selected } : item
        ));
    };

    const handleDownload = () => {
        const selected = items.filter(i => i.selected).map(i => ({
            url: i.url,
            filename: i.filename,
        }));
        onDownload(selected);
        onClose();
    };

    return (
        <div className="batch-overlay">
            <div className="batch-modal">
                <div className="batch-header">
                    <h2>📦 Batch Download</h2>
                    <button className="close-btn" onClick={onClose}>✕</button>
                </div>

                <div className="batch-toolbar">
                    <input
                        type="text"
                        placeholder="Search files..."
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        className="batch-search"
                    />
                    <select
                        value={typeFilter}
                        onChange={(e) => setTypeFilter(e.target.value)}
                        className="batch-filter"
                    >
                        <option value="all">All Types</option>
                        <option value="video">🎬 Videos</option>
                        <option value="audio">🎵 Audio</option>
                        <option value="archive">📦 Archives</option>
                        <option value="document">📄 Documents</option>
                        <option value="program">⚙️ Programs</option>
                        <option value="image">🖼️ Images</option>
                    </select>
                    <div className="batch-select-all">
                        <button onClick={() => toggleAll(true)}>Select All</button>
                        <button onClick={() => toggleAll(false)}>Deselect All</button>
                    </div>
                </div>

                <div className="batch-list">
                    {filteredItems.length === 0 ? (
                        <div className="batch-empty">No files found</div>
                    ) : (
                        filteredItems.map((item, index) => (
                            <div
                                key={index}
                                className={`batch-item ${item.selected ? 'selected' : ''}`}
                                onClick={() => toggleItem(item.url)}
                            >
                                <input
                                    type="checkbox"
                                    checked={item.selected}
                                    onChange={() => toggleItem(item.url)}
                                    onClick={(e) => e.stopPropagation()}
                                />
                                <span className="batch-item-icon">{getExtIcon(item.filename)}</span>
                                <div className="batch-item-info">
                                    <div className="batch-item-name">{item.filename}</div>
                                    <div className="batch-item-url">{item.url}</div>
                                </div>
                            </div>
                        ))
                    )}
                </div>

                <div className="batch-footer">
                    <div className="batch-count">
                        {selectedCount} of {items.length} files selected
                    </div>
                    <div className="batch-actions">
                        <button className="cancel-btn" onClick={onClose}>Cancel</button>
                        <button
                            className="download-btn"
                            onClick={handleDownload}
                            disabled={selectedCount === 0}
                        >
                            Download {selectedCount} Files
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
};
