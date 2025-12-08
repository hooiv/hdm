import React from 'react';
import { Download, Magnet, Settings, Activity, CheckCircle, Plus, Clock, Globe, Zap, Search, Minimize2 } from 'lucide-react';

interface DownloadStats {
    total: number;
    downloading: number;
    completed: number;
    totalBytes: number;
}

interface LayoutProps {
    children: React.ReactNode;
    onAddClick: () => void;
    onAddTorrentClick?: () => void;
    onScheduleClick?: () => void;
    onSettingsClick?: () => void;
    onOverlayClick?: () => void;
    onSpiderClick?: () => void;
    stats?: DownloadStats;
    onSpeedLimitChange?: (limit: number) => void;
    activeTab?: 'downloads' | 'torrents';
    onTabChange?: (tab: 'downloads' | 'torrents') => void;
}

const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const Layout: React.FC<LayoutProps> = ({ children, onAddClick, onAddTorrentClick, onScheduleClick, onSettingsClick, onOverlayClick, stats, onSpeedLimitChange, onSpiderClick, activeTab = 'downloads', onTabChange }) => {
    return (
        <div className="app-container">
            <aside className="sidebar">
                <div className="logo">⚡ HyperStream</div>
                <nav>
                    <div
                        className={`nav-item ${activeTab === 'downloads' ? 'active' : ''}`}
                        onClick={() => onTabChange && onTabChange('downloads')}
                    >
                        <Download size={20} />
                        <span>All Downloads</span>
                        {stats && <span className="nav-badge">{stats.total}</span>}
                    </div>

                    <div
                        className={`nav-item ${activeTab === 'torrents' ? 'active' : ''}`}
                        onClick={() => onTabChange && onTabChange('torrents')}
                    >
                        <Magnet size={20} />
                        <span>BitTorrent</span>
                    </div>

                    <div className="nav-divider" style={{ height: '1px', background: 'rgba(255,255,255,0.1)', margin: '15px 20px' }}></div>

                    <div className="nav-item">
                        <Activity size={20} style={{ color: '#3b82f6' }} />
                        <span>Downloading</span>
                        {stats && stats.downloading > 0 && (
                            <span className="nav-badge blue">{stats.downloading}</span>
                        )}
                    </div>
                    <div className="nav-item">
                        <CheckCircle size={20} style={{ color: '#22c55e' }} />
                        <span>Finished</span>
                        {stats && stats.completed > 0 && (
                            <span className="nav-badge green">{stats.completed}</span>
                        )}
                    </div>
                </nav>

                {stats && (
                    <div className="stats-panel" style={{ marginTop: 'auto', padding: '20px', fontSize: '0.85rem', color: '#64748b' }}>
                        <div className="stats-row" style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '5px' }}>
                            <span>Total Downloaded</span>
                            <span className="stats-value" style={{ color: '#f8fafc', fontWeight: 600 }}>{formatBytes(stats.totalBytes)}</span>
                        </div>
                    </div>
                )}

                <div className="sidebar-footer">
                    <div className="nav-item settings" onClick={onSettingsClick}>
                        <Settings size={20} />
                        <span>Settings</span>
                    </div>
                </div>
            </aside>
            <main className="main-content">
                <header className="top-bar">
                    <div className="header-actions" style={{ display: 'flex', gap: '10px' }}>
                        {onOverlayClick && (
                            <button className="icon-btn" onClick={onOverlayClick} title="Toggle Mini Overlay">
                                <Minimize2 size={18} />
                            </button>
                        )}
                        <button className="add-btn" onClick={onAddClick}>
                            <Plus size={16} /> Add Url
                        </button>
                        {onAddTorrentClick && (
                            <button className="add-btn" onClick={onAddTorrentClick} style={{ background: '#14b8a6' }}>
                                <Magnet size={16} /> Add Torrent
                            </button>
                        )}
                        <div style={{ width: '1px', background: 'rgba(255,255,255,0.1)', margin: '0 5px' }}></div>

                        <button className="schedule-btn" onClick={onSpiderClick} style={{ background: '#6366f1' }}>
                            <Globe size={16} /> Spider
                        </button>
                        <button className="schedule-btn" onClick={onScheduleClick}>
                            <Clock size={16} /> Schedule
                        </button>

                        <div className="speed-limit-control glass-panel" style={{ display: 'flex', alignItems: 'center', marginLeft: '10px', padding: '4px 8px', borderRadius: '6px', border: '1px solid rgba(255,255,255,0.1)' }}>
                            <Zap size={14} style={{ color: '#f59e0b', marginRight: '5px' }} />
                            <select
                                onChange={(e) => onSpeedLimitChange && onSpeedLimitChange(Number(e.target.value))}
                                style={{ background: 'transparent', border: 'none', color: '#cbd5e1', outline: 'none', fontSize: '0.85rem' }}
                                defaultValue={0}
                            >
                                <option value={0}>Unlimited</option>
                                <option value={512}>512 KB/s</option>
                                <option value={1024}>1 MB/s</option>
                                <option value={2048}>2 MB/s</option>
                                <option value={5120}>5 MB/s</option>
                                <option value={10240}>10 MB/s</option>
                            </select>
                        </div>
                    </div>
                    <div className="search-bar glass-input-container" style={{ marginLeft: 'auto', position: 'relative' }}>
                        <Search size={16} style={{ position: 'absolute', left: '10px', top: '50%', transform: 'translateY(-50%)', color: '#64748b' }} />
                        <input type="text" placeholder="Search..." style={{ paddingLeft: '32px' }} className="glass-input" />
                    </div>
                </header>
                <div className="content-area">
                    {children}
                </div>
            </main>
        </div>
    );
};
