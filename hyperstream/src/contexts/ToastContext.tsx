import React, { createContext, useContext, useState, useCallback, useRef, useEffect, useMemo, ReactNode } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { CheckCircle, AlertTriangle, XCircle, Info, X } from 'lucide-react';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface ToastMessage {
    id: string;
    type: ToastType;
    title?: string;
    message: string;
    duration?: number;
}

interface ToastContextType {
    toast: (options: Omit<ToastMessage, 'id'>) => void;
    success: (message: string, title?: string) => void;
    error: (message: string, title?: string) => void;
    warning: (message: string, title?: string) => void;
    info: (message: string, title?: string) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const useToast = () => {
    const context = useContext(ToastContext);
    if (!context) {
        throw new Error('useToast must be used within a ToastProvider');
    }
    return context;
};

const getIcon = (type: ToastType) => {
    switch (type) {
        case 'success':
            return <CheckCircle className="w-5 h-5 text-emerald-400" />;
        case 'error':
            return <XCircle className="w-5 h-5 text-rose-400" />;
        case 'warning':
            return <AlertTriangle className="w-5 h-5 text-amber-400" />;
        case 'info':
            return <Info className="w-5 h-5 text-sky-400" />;
    }
};

const getColors = (type: ToastType) => {
    switch (type) {
        case 'success':
            return 'bg-emerald-950/40 border-emerald-500/20';
        case 'error':
            return 'bg-rose-950/40 border-rose-500/20';
        case 'warning':
            return 'bg-amber-950/40 border-amber-500/20';
        case 'info':
            return 'bg-sky-950/40 border-sky-500/20';
    }
};

export const ToastProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
    const [toasts, setToasts] = useState<ToastMessage[]>([]);
    const toastIdCounter = useRef(0);
    const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

    // Cleanup all timers on unmount
    useEffect(() => {
        return () => {
            timersRef.current.forEach((timer) => clearTimeout(timer));
            timersRef.current.clear();
        };
    }, []);

    const removeToast = useCallback((id: string) => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
        const timer = timersRef.current.get(id);
        if (timer) {
            clearTimeout(timer);
            timersRef.current.delete(id);
        }
    }, []);

    const addToast = useCallback((options: Omit<ToastMessage, 'id'>) => {
        const id = `toast-${++toastIdCounter.current}`;
        setToasts((prev) => [...prev, { ...options, id }]);

        if (options.duration !== Infinity) {
            const timer = setTimeout(() => {
                removeToast(id);
            }, options.duration || 5000);
            timersRef.current.set(id, timer);
        }
    }, [removeToast]);

    const contextValue = useMemo<ToastContextType>(() => ({
        toast: addToast,
        success: (m, t) => addToast({ type: 'success', message: m, title: t }),
        error: (m, t) => addToast({ type: 'error', message: m, title: t }),
        warning: (m, t) => addToast({ type: 'warning', message: m, title: t }),
        info: (m, t) => addToast({ type: 'info', message: m, title: t }),
    }), [addToast]);

    return (
        <ToastContext.Provider value={contextValue}>
            {children}

            {/* Toast Container */}
            <div className="fixed bottom-6 right-6 z-[9999] flex flex-col gap-3 pointer-events-none">
                <AnimatePresence>
                    {toasts.map((t) => (
                        <motion.div
                            key={t.id}
                            initial={{ opacity: 0, y: 20, scale: 0.95 }}
                            animate={{ opacity: 1, y: 0, scale: 1 }}
                            exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.15 } }}
                            className={`pointer-events-auto flex items-start gap-3 p-4 rounded-xl border backdrop-blur-xl shadow-2xl max-w-sm w-full ${getColors(t.type)}`}
                        >
                            <div className="flex-shrink-0 mt-0.5">{getIcon(t.type)}</div>
                            <div className="flex-1 flex flex-col gap-1 pr-6">
                                {t.title && <h3 className="text-sm font-semibold text-white/90 leading-none">{t.title}</h3>}
                                <p className="text-sm text-slate-300 leading-snug whitespace-pre-wrap">{t.message}</p>
                            </div>
                            <button
                                onClick={() => removeToast(t.id)}
                                className="absolute top-3 right-3 p-1 rounded-md hover:bg-white/10 text-slate-400 hover:text-white transition-colors"
                            >
                                <X className="w-4 h-4" />
                            </button>
                        </motion.div>
                    ))}
                </AnimatePresence>
            </div>
        </ToastContext.Provider>
    );
};
