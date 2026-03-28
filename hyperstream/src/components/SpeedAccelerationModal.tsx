import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { X, Zap, Activity, Settings2, ShieldCheck, TrendingUp, BarChart2, Radio, ServerCrash } from 'lucide-react';
import { 
    getAccelerationStats, 
    getBandwidthHistory, 
    getOptimalSegmentStrategy, 
    predictNetworkChanges,
    toggleSpeedAcceleration,
    setProtocol,
    AccelerationStats 
} from '../api/speedAccelerationApi';

interface SpeedAccelerationModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export const SpeedAccelerationModal: React.FC<SpeedAccelerationModalProps> = ({ isOpen, onClose }) => {
    const [activeTab, setActiveTab] = useState<'overview' | 'tuning'>('overview');
    const [stats, setStats] = useState<AccelerationStats | null>(null);
    const [history, setHistory] = useState<[number, number][]>([]);
    const [strategy, setStrategy] = useState<string>('');
    const [prediction, setPrediction] = useState<string>('');
    const [isEngineActive, setIsEngineActive] = useState(true);
    const [protocol, setActiveProtocol] = useState<'tcp' | 'quic' | 'bbr' | 'auto'>('auto');

    useEffect(() => {
        if (!isOpen) return;
        
        let mounted = true;
        const fetchData = async () => {
            try {
                const s = await getAccelerationStats();
                const h = await getBandwidthHistory();
                const st = await getOptimalSegmentStrategy();
                const p = await predictNetworkChanges();
                if (mounted) {
                    setStats(s);
                    setHistory(h);
                    setStrategy(st);
                    setPrediction(p);
                }
            } catch (e) {
                console.error('Failed to fetch acceleration stats:', e);
            }
        };

        fetchData();
        const interval = setInterval(fetchData, 2000);
        return () => {
            mounted = false;
            clearInterval(interval);
        };
    }, [isOpen]);

    if (!isOpen) return null;

    const formatSpeed = (bps: number) => {
        if (bps >= 1000000000) return \\ GB/s\;
        if (bps >= 1000000) return \\ MB/s\;
        if (bps >= 1000) return \\ KB/s\;
        return \\ B/s\;
    };

    // Calculate max bandwidth for the chart scale
    const maxBps = history.length > 0 ? Math.max(...history.map(h => h[1]), 1000000) : 1000000;

    return (
        <AnimatePresence>
            <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
                onClick={onClose}
            >
                <motion.div
                    initial={{ scale: 0.95, opacity: 0, y: 20 }}
                    animate={{ scale: 1, opacity: 1, y: 0 }}
                    exit={{ scale: 0.95, opacity: 0, y: 20 }}
                    onClick={(e) => e.stopPropagation()}
                    className="w-full max-w-4xl bg-slate-900 border border-white/10 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]"
                >
                    <div className="flex items-center justify-between p-4 border-b border-white/10 bg-slate-800/50">
                        <div className="flex items-center gap-3">
                            <div className="p-2 bg-amber-500/20 rounded-lg">
                                <Zap className="text-amber-400" size={20} />
                            </div>
                            <div>
                                <h2 className="text-lg font-semibold text-white">Speed Acceleration Engine</h2>
                                <p className="text-xs text-slate-400">AI-driven bandwidth optimization & segment tuning</p>
                            </div>
                        </div>
                        <button
                            onClick={onClose}
                            className="p-2 text-slate-400 hover:text-white hover:bg-white/10 rounded-lg transition-colors"
                        >
                            <X size={20} />
                        </button>
                    </div>

                    <div className="flex border-b border-white/10 px-4 pt-2 gap-4 bg-slate-800/30">
                        <button
                            onClick={() => setActiveTab('overview')}
                            className={\pb-3 px-2 text-sm font-medium transition-colors relative \\}
                        >
                            Live Telemetry
                            {activeTab === 'overview' && (
                                <motion.div layoutId="speed-tab" className="absolute bottom-0 left-0 right-0 h-0.5 bg-amber-400 rounded-t-full" />
                            )}
                        </button>
                        <button
                            onClick={() => setActiveTab('tuning')}
                            className={\pb-3 px-2 text-sm font-medium transition-colors relative \\}
                        >
                            Engine Tuning
                            {activeTab === 'tuning' && (
                                <motion.div layoutId="speed-tab" className="absolute bottom-0 left-0 right-0 h-0.5 bg-amber-400 rounded-t-full" />
                            )}
                        </button>
                    </div>

                    <div className="p-6 overflow-y-auto custom-scrollbar flex-1">
                        {activeTab === 'overview' ? (
                            <div className="space-y-6">
                                {/* Stats Row */}
                                <div className="grid grid-cols-4 gap-4">
                                    <div className="bg-white/5 rounded-xl p-4 border border-white/5 flex flex-col justify-between">
                                        <div className="flex justify-between items-start mb-2">
                                            <span className="text-xs text-slate-400 font-medium">Avg Speed</span>
                                            <Activity size={14} className="text-cyan-400" />
                                        </div>
                                        <div className="text-2xl font-semibold text-white">
                                            {stats ? formatSpeed(stats.avg_speed_bps) : '---'}
                                        </div>
                                    </div>
                                    <div className="bg-white/5 rounded-xl p-4 border border-white/5 flex flex-col justify-between">
                                        <div className="flex justify-between items-start mb-2">
                                            <span className="text-xs text-slate-400 font-medium">Max Speed</span>
                                            <TrendingUp size={14} className="text-emerald-400" />
                                        </div>
                                        <div className="text-2xl font-semibold text-white">
                                            {stats ? formatSpeed(stats.max_speed_bps) : '---'}
                                        </div>
                                    </div>
                                    <div className="bg-white/5 rounded-xl p-4 border border-white/5 flex flex-col justify-between">
                                        <div className="flex justify-between items-start mb-2">
                                            <span className="text-xs text-slate-400 font-medium">Network Condition</span>
                                            <Radio size={14} className={stats?.network_condition === 'Excellent' ? 'text-emerald-400' : 'text-amber-400'} />
                                        </div>
                                        <div className="text-lg font-semibold text-white capitalize">
                                            {stats ? stats.network_condition.toLowerCase() : 'Analyzing...'}
                                        </div>
                                    </div>
                                    <div className="bg-white/5 rounded-xl p-4 border border-white/5 flex flex-col justify-between">
                                        <div className="flex justify-between items-start mb-2">
                                            <span className="text-xs text-slate-400 font-medium">Health Score</span>
                                            <ShieldCheck size={14} className="text-purple-400" />
                                        </div>
                                        <div className="flex items-end gap-2">
                                            <div className="text-2xl font-semibold text-white">
                                                {stats ? stats.health_score : '--'}
                                            </div>
                                            <span className="text-sm text-slate-400 mb-1">/ 100</span>
                                        </div>
                                    </div>
                                </div>

                                {/* Bandwidth Graph */}
                                <div className="bg-black/20 rounded-xl p-5 border border-white/5">
                                    <div className="flex items-center justify-between mb-6">
                                        <h3 className="text-sm font-medium text-slate-300 flex items-center gap-2">
                                            <BarChart2 size={16} className="text-blue-400" />
                                            Real-Time Throughput
                                        </h3>
                                        <div className="text-xs text-slate-500">
                                            Last {history.length} samples
                                        </div>
                                    </div>
                                    <div className="h-40 flex items-end gap-1 px-2">
                                        {history.length === 0 ? (
                                            <div className="w-full h-full flex items-center justify-center text-slate-500 text-sm">
                                                Awaiting telemetry...
                                            </div>
                                        ) : (
                                            history.map((point, i) => {
                                                const heightPct = Math.max((point[1] / maxBps) * 100, 2);
                                                return (
                                                    <motion.div
                                                        key={i}
                                                        initial={{ height: 0 }}
                                                        animate={{ height: \\%\ }}
                                                        transition={{ type: 'spring', bounce: 0, duration: 0.5 }}
                                                        className="flex-1 bg-gradient-to-t from-cyan-600/50 to-cyan-400 rounded-t-sm opacity-80 hover:opacity-100 transition-opacity relative group"
                                                    >
                                                        <div className="absolute -top-8 left-1/2 -translate-x-1/2 bg-slate-800 text-white text-xs py-1 px-2 rounded opacity-0 group-hover:opacity-100 pointer-events-none whitespace-nowrap z-10">
                                                            {formatSpeed(point[1])}
                                                        </div>
                                                    </motion.div>
                                                )
                                            })
                                        )}
                                    </div>
                                </div>

                                {/* AI Prediction & Strategy */}
                                <div className="grid grid-cols-2 gap-4">
                                    <div className="bg-white/5 rounded-xl p-5 border border-white/5">
                                        <h3 className="text-sm font-medium text-slate-300 mb-3 flex items-center gap-2">
                                            <ServerCrash size={16} className="text-indigo-400" />
                                            Predictive Analysis
                                        </h3>
                                        <div className="p-3 bg-black/20 rounded-lg text-sm text-slate-300 whitespace-pre-wrap font-mono">
                                            {prediction || 'Analyzing traffic patterns...'}
                                        </div>
                                    </div>
                                    <div className="bg-white/5 rounded-xl p-5 border border-white/5">
                                        <h3 className="text-sm font-medium text-slate-300 mb-3 flex items-center gap-2">
                                            <Settings2 size={16} className="text-rose-400" />
                                            Optimal Applied Strategy
                                        </h3>
                                        <div className="p-3 bg-black/20 rounded-lg text-sm text-slate-300 whitespace-pre-wrap font-mono relative overflow-hidden">
                                            {strategy || 'Converging optimal parameters...'}
                                            <div className="absolute inset-0 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNDAiIGhlaWdodD0iNDAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CjxwYXRoIGQ9Ik0wIDBoNDB2NDBIMHoiIGZpbGw9Im5vbmUiLz4KPHBhdGggZD0iTTAgMGg0MHY0MEgweiIgZmlsbD0idXJsKCNwKSIvPgo8ZGVmcz4KPHBhdHRlcm4gaWQ9InAiIHdpZHRoPSI0IiBoZWlnaHQ9IjQiIHBhdHRlcm5Vbml0cz0idXNlclNwYWNlT25Vc2UiPgo8cGF0aCBkPSJNMCA0VjBoNHY0IiBmaWxsPSJub25lIiBzdHJva2U9InJnYmEoMjU1LDI1NSwyNTUsMC4wNSkiLz4KPC9wYXR0ZXJuPgo8L2RlZnM+Cjwvc3ZnPg==')] opacity-50" pointer-events="none" />
                                        </div>
                                    </div>
                                </div>
                            </div>
                        ) : (
                            <div className="space-y-6">
                                {/* Engine Controls */}
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between p-4 bg-white/5 rounded-xl border border-white/5">
                                        <div>
                                            <h3 className="text-white font-medium">Master Engine Switch</h3>
                                            <p className="text-sm text-slate-400">Toggle all AI acceleration features</p>
                                        </div>
                                        <button 
                                            onClick={async () => {
                                                const next = !isEngineActive;
                                                await toggleSpeedAcceleration(next);
                                                setIsEngineActive(next);
                                            }}
                                            className={\w-12 h-6 rounded-full transition-colors relative \\}
                                        >
                                            <div className={\w-4 h-4 rounded-full bg-white absolute top-1 transition-transform \\} />
                                        </button>
                                    </div>

                                    <div className="grid grid-cols-2 gap-4">
                                        <div className="p-4 bg-white/5 rounded-xl border border-white/5 opacity-80 hover:opacity-100 transition-opacity">
                                            <h3 className="text-white font-medium mb-1">Transport Protocol</h3>
                                            <p className="text-xs text-slate-400 mb-3">Defines layer 4 connection semantics</p>
                                            <div className="flex bg-black/40 p-1 rounded-lg">
                                                {['auto', 'tcp', 'quic', 'bbr'].map(p => (
                                                    <button 
                                                        key={p}
                                                        onClick={() => {
                                                            setActiveProtocol(p as any);
                                                            setProtocol(p as any);
                                                        }}
                                                        className={\lex-1 text-xs py-1.5 rounded-md capitalize font-medium transition-colors \\}
                                                    >
                                                        {p}
                                                    </button>
                                                ))}
                                            </div>
                                        </div>
                                        
                                        <div className="p-4 bg-white/5 rounded-xl border border-white/5">
                                            <div className="flex justify-between items-center mb-1">
                                                <h3 className="text-white font-medium">TCP BBR Tuning</h3>
                                                <span className="px-2 py-0.5 rounded text-[10px] uppercase font-bold bg-emerald-500/20 text-emerald-400">Active</span>
                                            </div>
                                            <p className="text-xs text-slate-400">Bottleneck Bandwidth and Round-trip propagation time congestion control.</p>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        )}
                    </div>
                </motion.div>
            </motion.div>
        </AnimatePresence>
    );
};
