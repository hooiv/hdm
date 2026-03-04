import { useState, useImperativeHandle, forwardRef } from 'react';

export interface ToastProps {
    id: number;
    message: string;
    type: 'success' | 'error' | 'info';
}

export interface ToastRef {
    addToast: (message: string, type: 'success' | 'error' | 'info') => void;
}

let toastIdCounter = 0;
const MAX_TOASTS = 5;

export const ToastManager = forwardRef<ToastRef>((_props, ref) => {
    const [toasts, setToasts] = useState<ToastProps[]>([]);

    useImperativeHandle(ref, () => ({
        addToast(message, type) {
            const id = ++toastIdCounter;
            setToasts(prev => {
                const next = [...prev, { id, message, type }];
                // Cap visible toasts to prevent overflow
                return next.length > MAX_TOASTS ? next.slice(-MAX_TOASTS) : next;
            });
            setTimeout(() => {
                setToasts(prev => prev.filter(t => t.id !== id));
            }, 3000);
        }
    }));

    return (
        <div className="toast-container" style={{
            position: 'fixed',
            bottom: '20px',
            right: '20px',
            zIndex: 9999,
            display: 'flex',
            flexDirection: 'column',
            gap: '10px'
        }}>
            {toasts.map(toast => (
                <div key={toast.id} className={`toast toast-${toast.type}`} style={{
                    padding: '12px 20px',
                    borderRadius: '8px',
                    background: toast.type === 'success' ? '#22c55e' : toast.type === 'error' ? '#ef4444' : '#3b82f6',
                    color: 'white',
                    boxShadow: '0 4px 6px rgba(0,0,0,0.1)',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '10px',
                    animation: 'slideIn 0.3s ease-out'
                }}>
                    <span>{toast.type === 'success' ? '✓' : toast.type === 'error' ? '⚠' : 'ℹ️'}</span>
                    {toast.message}
                </div>
            ))}
            <style>{`
                @keyframes slideIn {
                    from { transform: translateX(100%); opacity: 0; }
                    to { transform: translateX(0); opacity: 1; }
                }
            `}</style>
        </div>
    );
});
