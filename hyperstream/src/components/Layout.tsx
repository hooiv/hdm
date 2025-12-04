import React from 'react';

interface LayoutProps {
    children: React.ReactNode;
    onAddClick: () => void;
}

export const Layout: React.FC<LayoutProps> = ({ children, onAddClick }) => {
    return (
        <div className="app-container">
            <aside className="sidebar">
                <div className="logo">HyperStream</div>
                <nav>
                    <div className="nav-item active">All Downloads</div>
                    <div className="nav-item">Downloading</div>
                    <div className="nav-item">Finished</div>
                    <div className="nav-item">Trash</div>
                </nav>
            </aside>
            <main className="main-content">
                <header className="top-bar">
                    <button className="add-btn" onClick={onAddClick}>+ Add Url</button>
                    <div className="search-bar">
                        <input type="text" placeholder="Search..." />
                    </div>
                </header>
                <div className="content-area">
                    {children}
                </div>
            </main>
        </div>
    );
};
