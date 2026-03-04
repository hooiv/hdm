import React from 'react';
import { error as logError } from '../utils/logger';

interface ErrorBoundaryProps {
    children: React.ReactNode;
    fallback?: React.ReactNode;
}

interface ErrorBoundaryState {
    hasError: boolean;
    error: Error | null;
    errorInfo: React.ErrorInfo | null;
}

/**
 * Global error boundary to prevent the entire app from crashing.
 * Catches React rendering errors and displays a recovery UI.
 */
const MAX_RESET_RETRIES = 3;

export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState & { resetCount: number }> {
    constructor(props: ErrorBoundaryProps) {
        super(props);
        this.state = { hasError: false, error: null, errorInfo: null, resetCount: 0 };
    }

    static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
        return { hasError: true, error };
    }

    componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
        this.setState({ errorInfo });
        logError('[ErrorBoundary] Uncaught error:', error, errorInfo);
    }

    handleReset = () => {
        if (this.state.resetCount >= MAX_RESET_RETRIES) {
            // Too many retries — force full reload
            window.location.reload();
            return;
        }
        this.setState((prev) => ({ hasError: false, error: null, errorInfo: null, resetCount: prev.resetCount + 1 }));
    };

    render() {
        if (this.state.hasError) {
            if (this.props.fallback) {
                return this.props.fallback;
            }

            return (
                <div style={{
                    position: 'fixed',
                    inset: 0,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    background: 'linear-gradient(135deg, #0f172a 0%, #1e293b 100%)',
                    zIndex: 9999,
                    fontFamily: "'Inter', -apple-system, sans-serif",
                }}>
                    <div style={{
                        maxWidth: 520,
                        width: '90%',
                        padding: 40,
                        borderRadius: 16,
                        background: 'rgba(30, 41, 59, 0.8)',
                        border: '1px solid rgba(148, 163, 184, 0.1)',
                        backdropFilter: 'blur(20px)',
                        boxShadow: '0 25px 50px rgba(0, 0, 0, 0.5)',
                        textAlign: 'center',
                    }}>
                        <div style={{
                            width: 64,
                            height: 64,
                            margin: '0 auto 20px',
                            borderRadius: 16,
                            background: 'rgba(239, 68, 68, 0.1)',
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            fontSize: 28,
                            border: '1px solid rgba(239, 68, 68, 0.2)',
                        }}>
                            ⚠️
                        </div>
                        <h2 style={{
                            color: '#f1f5f9',
                            fontSize: 20,
                            fontWeight: 600,
                            margin: '0 0 8px',
                            letterSpacing: '-0.02em',
                        }}>
                            Something went wrong
                        </h2>
                        <p style={{
                            color: '#94a3b8',
                            fontSize: 14,
                            margin: '0 0 24px',
                            lineHeight: 1.6,
                        }}>
                            An unexpected error occurred. Your downloads are safe — click below to recover.
                        </p>
                        {this.state.error && (
                            <div style={{
                                background: 'rgba(0, 0, 0, 0.3)',
                                borderRadius: 8,
                                padding: '12px 16px',
                                marginBottom: 24,
                                textAlign: 'left',
                                border: '1px solid rgba(239, 68, 68, 0.15)',
                            }}>
                                <code style={{
                                    color: '#f87171',
                                    fontSize: 12,
                                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                                    wordBreak: 'break-word',
                                }}>
                                    {this.state.error.message}
                                </code>
                            </div>
                        )}
                        <button
                            onClick={this.handleReset}
                            style={{
                                padding: '12px 32px',
                                borderRadius: 10,
                                border: 'none',
                                background: 'linear-gradient(135deg, #3b82f6, #2563eb)',
                                color: 'white',
                                fontSize: 14,
                                fontWeight: 600,
                                cursor: 'pointer',
                                letterSpacing: '-0.01em',
                                transition: 'all 0.2s',
                                boxShadow: '0 4px 12px rgba(37, 99, 235, 0.3)',
                            }}
                            onMouseEnter={(e) => {
                                (e.target as HTMLButtonElement).style.transform = 'translateY(-1px)';
                                (e.target as HTMLButtonElement).style.boxShadow = '0 6px 20px rgba(37, 99, 235, 0.4)';
                            }}
                            onMouseLeave={(e) => {
                                (e.target as HTMLButtonElement).style.transform = 'translateY(0)';
                                (e.target as HTMLButtonElement).style.boxShadow = '0 4px 12px rgba(37, 99, 235, 0.3)';
                            }}
                        >
                            Recover App
                        </button>
                    </div>
                </div>
            );
        }

        return this.props.children;
    }
}
