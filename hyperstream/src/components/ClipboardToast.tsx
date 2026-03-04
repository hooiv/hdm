import React from 'react';

interface ToastProps {
    message: string;
    filename: string;
    onDownload: () => void;
    onDismiss: () => void;
}

export const ClipboardToast: React.FC<ToastProps> = ({ message, filename, onDownload, onDismiss }) => {
    return (
        <div className="clipboard-toast" role="alert" aria-live="polite">
            <div className="toast-icon">📋</div>
            <div className="toast-content">
                <div className="toast-message">{message}</div>
                <div className="toast-filename">{filename}</div>
            </div>
            <div className="toast-actions">
                <button className="toast-download" onClick={onDownload}>Download</button>
                <button className="toast-dismiss" onClick={onDismiss}>✕</button>
            </div>
        </div>
    );
};
