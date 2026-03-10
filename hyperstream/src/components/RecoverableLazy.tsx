import { Component, Suspense, lazy, useMemo, useState, type ComponentType, type ErrorInfo, type ReactNode } from 'react';
import { error as logError } from '../utils/logger';

interface RecoverableLazyProps<TModule, TProps extends object> {
  loader: () => Promise<TModule>;
  resolve: (module: TModule) => ComponentType<TProps>;
  componentProps: TProps;
  loadingFallback: ReactNode;
  failureTitle: string;
  failureMessage: string;
  retryLabel?: string;
  renderFailure?: (error: Error, retry: () => void) => ReactNode;
}

interface LazyLoadBoundaryProps {
  children: ReactNode;
  title: string;
  message: string;
  retryLabel: string;
  onRetry: () => void;
  renderFailure?: (error: Error, retry: () => void) => ReactNode;
}

interface LazyLoadBoundaryState {
  error: Error | null;
}

class LazyLoadBoundary extends Component<LazyLoadBoundaryProps, LazyLoadBoundaryState> {
  state: LazyLoadBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): LazyLoadBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    logError('[LazyLoadBoundary] Failed to load lazy view:', error, errorInfo);
  }

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    if (this.props.renderFailure) {
      return this.props.renderFailure(this.state.error, this.props.onRetry);
    }

    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <div className="max-w-lg w-full rounded-2xl border border-amber-500/20 bg-slate-900/80 backdrop-blur-xl shadow-2xl p-6 text-center">
          <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl border border-amber-500/20 bg-amber-500/10 text-2xl">
            ⚠️
          </div>
          <h3 className="text-lg font-semibold text-slate-100">{this.props.title}</h3>
          <p className="mt-2 text-sm leading-6 text-slate-400">{this.props.message}</p>
          <div className="mt-4 rounded-xl border border-amber-500/10 bg-black/20 px-3 py-2 text-left">
            <code className="text-xs text-amber-300 break-all">{this.state.error.message}</code>
          </div>
          <button
            onClick={this.props.onRetry}
            className="mt-5 inline-flex items-center justify-center rounded-xl bg-cyan-500/20 px-4 py-2 text-sm font-medium text-cyan-300 transition-colors hover:bg-cyan-500/30"
          >
            {this.props.retryLabel}
          </button>
        </div>
      </div>
    );
  }
}

export function RecoverableLazy<TModule, TProps extends object>({
  loader,
  resolve,
  componentProps,
  loadingFallback,
  failureTitle,
  failureMessage,
  retryLabel = 'Retry loading view',
  renderFailure,
}: RecoverableLazyProps<TModule, TProps>) {
  const [attempt, setAttempt] = useState(0);

  const LazyComponent = useMemo(
    () => lazy(() => loader().then((module) => ({ default: resolve(module) }))),
    [attempt, loader, resolve],
  );

  return (
    <LazyLoadBoundary
      key={attempt}
      title={failureTitle}
      message={failureMessage}
      retryLabel={retryLabel}
      onRetry={() => setAttempt((prev) => prev + 1)}
      renderFailure={renderFailure}
    >
      <Suspense fallback={loadingFallback}>
        <LazyComponent {...componentProps} />
      </Suspense>
    </LazyLoadBoundary>
  );
}