import React from 'react';
import { motion } from 'framer-motion';
import { Zap, Activity, HardDrive } from 'lucide-react';

interface DashboardStatsProps {
    globalSpeed: number; // bytes per second
    activeCount: number;
    totalDownloaded: number; // bytes
}

const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const DashboardStats: React.FC<DashboardStatsProps> = ({ globalSpeed, activeCount, totalDownloaded }) => {
    return (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6 px-4">
            <StatCard
                label="Global Speed"
                value={`${formatBytes(globalSpeed)}/s`}
                icon={Zap}
                color="cyan"
                trend={globalSpeed > 0 ? "Active" : "Idle"}
            />
            <StatCard
                label="Active Downloads"
                value={activeCount.toString()}
                icon={Activity}
                color="violet"
                trend={`${activeCount} TASKS`}
            />
            <StatCard
                label="Total Data"
                value={formatBytes(totalDownloaded)}
                icon={HardDrive}
                color="emerald"
                trend="Session"
            />
        </div>
    );
};

const StatCard: React.FC<{ label: string; value: string; icon: any; color: 'cyan' | 'violet' | 'emerald'; trend: string }> = ({ label, value, icon: Icon, color, trend }) => {
    const colorClasses = {
        cyan: 'from-cyan-500/20 to-blue-500/5 border-cyan-500/30 text-cyan-400',
        violet: 'from-violet-500/20 to-fuchsia-500/5 border-violet-500/30 text-violet-400',
        emerald: 'from-emerald-500/20 to-teal-500/5 border-emerald-500/30 text-emerald-400'
    };

    return (
        <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            whileHover={{ scale: 1.02 }}
            className={`
                relative overflow-hidden rounded-2xl border p-4 backdrop-blur-md
                bg-gradient-to-br ${colorClasses[color]}
                shadow-[0_0_15px_rgba(0,0,0,0.2)]
            `}
        >
            <div className="absolute top-0 right-0 p-3 opacity-20">
                <Icon size={48} />
            </div>

            <div className="relative z-10 flex flex-col gap-1">
                <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wider opacity-70">
                    <Icon size={14} />
                    {label}
                </div>
                <div className="text-2xl font-bold font-mono tracking-tight text-white drop-shadow-sm">
                    {value}
                </div>
                <div className="text-[10px] font-semibold opacity-60 flex items-center gap-1 mt-1">
                    {trend === 'Idle' ? <div className="w-1.5 h-1.5 rounded-full bg-slate-500" /> : <div className="w-1.5 h-1.5 rounded-full bg-white animate-pulse" />}
                    {trend}
                </div>
            </div>

            {/* Background Glow */}
            <div className="absolute -bottom-8 -right-8 w-24 h-24 bg-white/5 rounded-full blur-2xl pointer-events-none" />
        </motion.div>
    );
};
