
import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ZipEntry {
    name: String;
    is_directory: boolean;
    compressed_size: number;
    uncompressed_size: number;
    compression_method: String;
}

interface ZipPreviewData {
    total_files: number;
    total_directories: number;
    total_compressed_size: number;
    total_uncompressed_size: number;
    entries: ZipEntry[];
}

interface ZipPreviewModalProps {
    filePath: string;
    isOpen: boolean;
    onClose: () => void;
    isPartial?: boolean;
}

export const ZipPreviewModal: React.FC<ZipPreviewModalProps> = ({ filePath, isOpen, onClose, isPartial = false }) => {
    const [data, setData] = useState<ZipPreviewData | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (isOpen && filePath) {
            loadPreview();
        }
    }, [isOpen, filePath]);

    const loadPreview = async () => {
        setLoading(true);
        setError(null);
        try {
            if (isPartial) {
                // Read last 64KB (sufficient for EOCD)
                const bytes = await invoke<number[]>('read_zip_last_bytes', { path: filePath, length: 65536 });
                const result = await invoke<ZipPreviewData>('preview_zip_partial', { data: bytes });
                setData(result);
            } else {
                const result = await invoke<ZipPreviewData>('preview_zip_file', { path: filePath });
                setData(result);
            }
        } catch (err) {
            console.error('Failed to preview ZIP:', err);
            setError(typeof err === 'string' ? err : 'Failed to load preview');
        } finally {
            setLoading(false);
        }
    };

    if (!isOpen) return null;

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="modal-content zip-preview-modal" onClick={e => e.stopPropagation()}>
                <div className="modal-header">
                    <h3>📦 ZIP Preview {isPartial && '(Partial)'}</h3>
                    <button className="close-btn" onClick={onClose}>✕</button>
                </div>

                <div className="modal-body">
                    {loading && <div className="loading-spinner">Loading archive structure...</div>}

                    {error && (
                        <div className="error-message">
                            <p>⚠️ {error}</p>
                            {isPartial && <small>Partial preview requires the end of the file to be downloaded.</small>}
                        </div>
                    )}

                    {data && (
                        <div className="zip-content">
                            <div className="zip-stats">
                                <div className="stat-item">
                                    <span className="label">Files:</span>
                                    <span className="value">{data.total_files}</span>
                                </div>
                                <div className="stat-item">
                                    <span className="label">Size:</span>
                                    <span className="value">{(data.total_uncompressed_size / 1024 / 1024).toFixed(2)} MB</span>
                                </div>
                            </div>

                            <div className="file-list">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>Name</th>
                                            <th>Size</th>
                                            <th>Compressed</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {data.entries.map((entry, idx) => (
                                            <tr key={idx}>
                                                <td className="file-name-cell">
                                                    {entry.is_directory ? '📁 ' : '📄 '}
                                                    {entry.name}
                                                </td>
                                                <td>{(entry.uncompressed_size / 1024).toFixed(1)} KB</td>
                                                <td>{(entry.compressed_size / 1024).toFixed(1)} KB</td>
                                            </tr>
                                        ))}
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
};
