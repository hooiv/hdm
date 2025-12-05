import React from 'react';

interface DownloadStats {
    total: number;
    downloading: number;
    completed: number;
    totalBytes: number;
}

interface LayoutProps {
    children: React.ReactNode;
    onAddClick: () => void;
    onScheduleClick?: () => void;
    onSettingsClick?: () => void;
    stats?: DownloadStats;
}

const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const Layout: React.FC<LayoutProps> = ({ children, onAddClick, onScheduleClick, onSettingsClick, stats }) => {
    return (
        <div className="app-container">
            <aside className="sidebar">
                <div className="logo">⚡ HyperStream</div>
                <nav>
                    <div className="nav-item active">
                        📥 All Downloads
                        {stats && <span style={{ float: 'right', opacity: 0.7 }}>{stats.total}</span>}
                    </div>
                    <div className="nav-item">
                        ⬇️ Downloading
                        {stats && stats.downloading > 0 && (
                            <span style={{ float: 'right', color: '#3b82f6' }}>{stats.downloading}</span>
                        )}
                    </div>
                    <div className="nav-item">
                        ✅ Finished
                        {stats && stats.completed > 0 && (
                            <span style={{ float: 'right', color: '#22c55e' }}>{stats.completed}</span>
                        )}
                    </div>
                </nav>

                {stats && (
                    <div className="stats-panel">
                        <div className="stats-row">
                            <span>Total Downloaded</span>
                            <span className="stats-value">{formatBytes(stats.totalBytes)}</span>
                        </div>
                    </div>
                )}

                <div className="sidebar-footer">
                    <div className="nav-item settings" onClick={onSettingsClick}>⚙️ Settings</div>
                </div>
            </aside>
            <main className="main-content">
                <header className="top-bar">
                    <button className="add-btn" onClick={onAddClick}>+ Add Url</button>
                    <button className="schedule-btn" onClick={onScheduleClick}>⏰ Schedule</button>
                    <div className="search-bar">
                        <input type="text" placeholder="Search downloads..." />
                    </div>
                </header>
                <div className="content-area">
                    {children}
                </div>
            </main>
        </div>
    );
};
