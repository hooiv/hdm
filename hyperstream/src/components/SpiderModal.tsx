import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { GrabbedFile } from '../types';
import './SpiderModal.css';

interface SpiderModalProps {
    isOpen: boolean;
    onClose: () => void;
    onDownload: (files: GrabbedFile[]) => void;
}

export const SpiderModal: React.FC<SpiderModalProps> = ({ isOpen, onClose, onDownload }) => {
    const [url, setUrl] = useState('');
    const [maxDepth, setMaxDepth] = useState(1);
    const [extensions, setExtensions] = useState({
        jpg: true,
        png: true,
        mp4: true,
        zip: true,
        pdf: false,
    });
    const [isCrawling, setIsCrawling] = useState(false);
    const [results, setResults] = useState<GrabbedFile[]>([]);
    const [selectedUrls, setSelectedUrls] = useState<Set<string>>(new Set());
    const [error, setError] = useState<string | null>(null);

    const handleCrawl = async () => {
        setIsCrawling(true);
        setError(null);
        setResults([]);
        setSelectedUrls(new Set());

        const activeExtensions = Object.entries(extensions)
            .filter(([_, active]) => active)
            .map(([ext]) => ext);

        try {
            const files = await invoke<GrabbedFile[]>('crawl_website', {
                url,
                maxDepth: Number(maxDepth),
                extensions: activeExtensions
            });
            setResults(files);
            // Auto-select all by default
            const allUrls = new Set(files.map(f => f.url));
            setSelectedUrls(allUrls);
        } catch (err: any) {
            setError(err.toString());
        } finally {
            setIsCrawling(false);
        }
    };

    const toggleSelection = (fileUrl: string) => {
        const newSet = new Set(selectedUrls);
        if (newSet.has(fileUrl)) {
            newSet.delete(fileUrl);
        } else {
            newSet.add(fileUrl);
        }
        setSelectedUrls(newSet);
    };

    const handleDownloadSelected = () => {
        const selectedFiles = results.filter(f => selectedUrls.has(f.url));
        onDownload(selectedFiles);
        onClose();
    };

    if (!isOpen) return null;

    return (
        <div className="modal-overlay">
            <div className="modal-content spider-modal">
                <div className="modal-header">
                    <h2>🕷 Site Grabber</h2>
                    <button className="close-btn" onClick={onClose}>&times;</button>
                </div>
                <div className="spider-controls">
                    <div className="input-group">
                        <label>Start URL:</label>
                        <input
                            type="text"
                            value={url}
                            onChange={e => setUrl(e.target.value)}
                            placeholder="https://example.com/gallery"
                        />
                    </div>
                    <div className="input-group row">
                        <label>Depth:
                            <input
                                type="number"
                                min="0"
                                max="3"
                                value={maxDepth}
                                onChange={e => setMaxDepth(Number(e.target.value))}
                                style={{ width: '60px', marginLeft: '10px' }}
                            />
                        </label>
                    </div>
                    <div className="extensions-group">
                        <label>Extensions:</label>
                        {Object.keys(extensions).map(ext => (
                            <label key={ext} className="checkbox-label">
                                <input
                                    type="checkbox"
                                    checked={(extensions as any)[ext]}
                                    onChange={e => setExtensions(prev => ({ ...prev, [ext]: e.target.checked }))}
                                />
                                {ext.toUpperCase()}
                            </label>
                        ))}
                    </div>
                    <button
                        className="crawl-btn"
                        onClick={handleCrawl}
                        disabled={isCrawling || !url}
                    >
                        {isCrawling ? 'Crawling...' : 'Start Crawling'}
                    </button>
                </div>

                {error && <div className="error-message">{error}</div>}

                <div className="results-area">
                    {results.length > 0 && (
                        <>
                            <div className="results-header">
                                <span>Found {results.length} files</span>
                                <button onClick={() => setSelectedUrls(new Set(results.map(f => f.url)))}>Select All</button>
                                <button onClick={() => setSelectedUrls(new Set())}>Deselect All</button>
                            </div>
                            <div className="results-grid">
                                {results.map((file, idx) => (
                                    <div key={idx} className={`result-item ${selectedUrls.has(file.url) ? 'selected' : ''}`} onClick={() => toggleSelection(file.url)}>
                                        <div className="file-icon">{file.file_type === 'image' ? '🖼️' : '📄'}</div>
                                        <div className="file-info">
                                            <div className="file-name" title={file.url}>{file.filename}</div>
                                            <div className="file-meta">{file.size ? `${(file.size / 1024).toFixed(1)} KB` : 'Unknown size'}</div>
                                        </div>
                                        <input
                                            type="checkbox"
                                            checked={selectedUrls.has(file.url)}
                                            readOnly
                                        />
                                    </div>
                                ))}
                            </div>
                            <button className="download-all-btn" onClick={handleDownloadSelected}>
                                Download {selectedUrls.size} Selected Files
                            </button>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
};
