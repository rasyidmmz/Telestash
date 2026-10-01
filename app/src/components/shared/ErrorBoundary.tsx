import { Component, ReactNode } from 'react';
import { AlertTriangle, RefreshCw } from './icons.tsx';
import { formatLogValue, recordErrorLog } from '../../errorLogs';

interface Props {
    children: ReactNode;
}

interface State {
    hasError: boolean;
    error: Error | null;
    retryCount: number;
}

export class ErrorBoundary extends Component<Props, State> {
    constructor(props: Props) {
        super(props);
        this.state = { hasError: false, error: null, retryCount: 0 };
    }

    static getDerivedStateFromError(error: Error): State {
        return { hasError: true, error, retryCount: 0 };
    }

    componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
        recordErrorLog({
            source: 'react.error-boundary',
            message: error.message || 'Unexpected interface error',
            details: [formatLogValue(error), formatLogValue(errorInfo.componentStack)].join('\n'),
        });
        console.error('ErrorBoundary caught an error:', error, errorInfo);
    }

    handleRetry = () => {
        this.setState((state) => ({ hasError: false, error: null, retryCount: state.retryCount + 1 }));
    };

    handleReload = () => {
        window.location.reload();
    };

    render() {
        if (this.state.hasError) {
            return (
                <div className="h-screen w-screen flex items-center justify-center bg-stash-bg p-8">
                    <div className="max-w-md w-full bg-stash-surface border border-stash-border rounded-2xl p-8 text-center shadow-2xl">
                        <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-red-500/10 flex items-center justify-center">
                            <AlertTriangle className="w-8 h-8 text-red-400" />
                        </div>
                        <h1 className="text-xl font-semibold text-stash-text mb-2">TeleStash perlu dimuat ulang</h1>
                        <p className="text-stash-subtext text-sm mb-6">
                            Bagian aplikasi mengalami kesalahan. Muat ulang aplikasi untuk memulihkan sesi.
                        </p>

                        {this.state.error && (
                            <details className="mb-6 text-left">
                                <summary className="text-xs text-stash-subtext cursor-pointer hover:text-stash-text transition-colors">
                                    Detail teknis (untuk laporan)
                                </summary>
                                <pre className="mt-2 p-3 bg-stash-hover rounded-lg text-xs text-red-400 overflow-auto max-h-32">
                                    {this.state.error.message}
                                </pre>
                            </details>
                        )}

                        <div className="flex flex-col sm:flex-row gap-3 justify-center">
                            <button
                                onClick={this.handleRetry}
                                className="inline-flex items-center justify-center gap-2 px-5 py-3 bg-stash-primary text-black font-medium rounded-lg hover:bg-stash-primary/90 transition-colors"
                            >
                                <RefreshCw className="w-4 h-4" />
                                Coba Lagi
                            </button>
                            <button
                                onClick={this.handleReload}
                                className="inline-flex items-center justify-center gap-2 px-5 py-3 border border-stash-border text-stash-text font-medium rounded-lg hover:bg-stash-hover transition-colors"
                            >
                                Muat Ulang Penuh
                            </button>
                        </div>
                    </div>
                </div>
            );
        }

        return <div key={this.state.retryCount}>{this.props.children}</div>;
    }
}
