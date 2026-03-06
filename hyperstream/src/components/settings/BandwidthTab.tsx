import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Gauge,
  Zap,
  RefreshCw,
  Trash2,
  Plus,
  ArrowUpDown,
  Wifi,
  WifiOff,
  ChevronDown,
  Activity,
  BarChart3,
} from "lucide-react";

// ─── Types ───────────────────────────────────────────────────────────────────

interface AllocationSnapshot {
  download_id: string;
  priority: number;
  allocated_bps: number;
  min_bps: number;
  max_bps: number;
}

interface QosEntry {
  download_id: string;
  priority: string; // "Critical" | "High" | "Normal" | "Low" | "Background"
  max_bytes_per_sec: number;
  current_bytes_per_sec: number;
  total_downloaded: number;
}

interface QosStats {
  total_bandwidth_limit: number;
  total_active: number;
  entries: QosEntry[];
}

type PriorityLevel = "critical" | "high" | "normal" | "low" | "background";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatBps(bps: number): string {
  if (bps === 0) return "Unlimited";
  if (bps < 1024) return `${bps} B/s`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
  if (bps < 1024 * 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(2)} MB/s`;
  return `${(bps / (1024 * 1024 * 1024)).toFixed(2)} GB/s`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

const PRIORITY_LEVELS: { value: PriorityLevel; label: string; color: string; weight: string }[] = [
  { value: "critical", label: "Critical", color: "text-red-400", weight: "100%" },
  { value: "high", label: "High", color: "text-orange-400", weight: "75%" },
  { value: "normal", label: "Normal", color: "text-blue-400", weight: "50%" },
  { value: "low", label: "Low", color: "text-slate-400", weight: "25%" },
  { value: "background", label: "Background", color: "text-slate-600", weight: "10%" },
];

function getPriorityColor(priority: string | number): string {
  if (typeof priority === "number") {
    if (priority >= 9) return "text-red-400";
    if (priority >= 7) return "text-orange-400";
    if (priority >= 5) return "text-blue-400";
    if (priority >= 3) return "text-slate-400";
    return "text-slate-600";
  }
  const level = PRIORITY_LEVELS.find(
    (l) => l.label.toLowerCase() === priority.toString().toLowerCase()
  );
  return level?.color ?? "text-slate-400";
}

function getPriorityBg(priority: string | number): string {
  if (typeof priority === "number") {
    if (priority >= 9) return "bg-red-500/10";
    if (priority >= 7) return "bg-orange-500/10";
    if (priority >= 5) return "bg-blue-500/10";
    if (priority >= 3) return "bg-slate-500/10";
    return "bg-slate-500/5";
  }
  const name = priority.toString().toLowerCase();
  if (name === "critical") return "bg-red-500/10";
  if (name === "high") return "bg-orange-500/10";
  if (name === "normal") return "bg-blue-500/10";
  if (name === "low") return "bg-slate-500/10";
  return "bg-slate-500/5";
}

// ─── Component ───────────────────────────────────────────────────────────────

export const BandwidthTab: React.FC = () => {
  // Global speed limit (kbps from backend)
  const [globalLimitKbps, setGlobalLimitKbps] = useState(0);
  const [globalLimitInput, setGlobalLimitInput] = useState("");
  
  // QoS stats
  const [qosStats, setQosStats] = useState<QosStats | null>(null);
  
  // Bandwidth allocator snapshots
  const [allocations, setAllocations] = useState<AllocationSnapshot[]>([]);
  
  // Polling
  const [autoRefresh, setAutoRefresh] = useState(true);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  
  // QoS global limit input (bytes/sec)
  const [qosLimitInput, setQosLimitInput] = useState("");
  
  // Register new bandwidth allocation
  const [showRegister, setShowRegister] = useState(false);
  const [regId, setRegId] = useState("");
  const [regPriority, setRegPriority] = useState(5);
  const [regMinBps, setRegMinBps] = useState("");
  const [regMaxBps, setRegMaxBps] = useState("");
  
  // Set QoS priority
  const [priorityEditId, setPriorityEditId] = useState<string | null>(null);
  const [priorityEditLevel, setPriorityEditLevel] = useState<PriorityLevel>("normal");

  // ─── Data Loading ────────────────────────────────────────────────────────

  const loadAll = async () => {
    try {
      const [limit, stats, allocs] = await Promise.all([
        invoke<number>("get_speed_limit"),
        invoke<QosStats>("get_qos_stats"),
        invoke<AllocationSnapshot[]>("get_bandwidth_allocations"),
      ]);
      setGlobalLimitKbps(limit);
      setQosStats(stats);
      setAllocations(allocs);
    } catch {
      // Partial failures are acceptable
    }
  };

  useEffect(() => {
    loadAll();
  }, []);

  useEffect(() => {
    if (autoRefresh) {
      pollRef.current = setInterval(loadAll, 2000);
    }
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [autoRefresh]);

  // ─── Handlers ────────────────────────────────────────────────────────────

  const handleSetGlobalLimit = async () => {
    const kbps = parseInt(globalLimitInput, 10);
    if (isNaN(kbps) || kbps < 0) return;
    await invoke("set_speed_limit", { limitKbps: kbps });
    setGlobalLimitKbps(kbps);
    setGlobalLimitInput("");
  };

  const handleSetQosGlobalLimit = async () => {
    const bps = parseInt(qosLimitInput, 10);
    if (isNaN(bps) || bps < 0) return;
    await invoke("set_qos_global_limit", { limit: bps });
    setQosLimitInput("");
    loadAll();
  };

  const handleRebalance = async () => {
    await invoke("rebalance_bandwidth");
    loadAll();
  };

  const handleRegister = async () => {
    if (!regId.trim()) return;
    const config = {
      priority: regPriority,
      min_bps: parseInt(regMinBps, 10) || 0,
      max_bps: parseInt(regMaxBps, 10) || 0,
    };
    await invoke("register_download_bandwidth", { id: regId.trim(), config });
    setRegId("");
    setRegMinBps("");
    setRegMaxBps("");
    setShowRegister(false);
    loadAll();
  };

  const handleDeregister = async (id: string) => {
    await invoke("deregister_download_bandwidth", { id });
    loadAll();
  };

  const handleSetPriority = async (id: string, level: PriorityLevel) => {
    await invoke("set_download_priority", { id, level });
    setPriorityEditId(null);
    loadAll();
  };

  const handleRemoveQos = async (id: string) => {
    await invoke("remove_qos_download", { id });
    loadAll();
  };

  // ─── Computed ────────────────────────────────────────────────────────────

  const totalAllocated = allocations.reduce((s, a) => s + a.allocated_bps, 0);
  const qosTotalCurrent = qosStats?.entries.reduce((s, e) => s + e.current_bytes_per_sec, 0) ?? 0;

  // ─── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-slate-100 flex items-center gap-2">
            <Gauge size={22} className="text-cyan-400" />
            Bandwidth & QoS
          </h2>
          <p className="text-sm text-slate-500 mt-1">
            Control speed limits, bandwidth allocation, and download prioritization
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors ${
              autoRefresh
                ? "bg-green-500/10 text-green-400 hover:bg-green-500/20"
                : "bg-slate-500/10 text-slate-400 hover:bg-slate-500/20"
            }`}
          >
            {autoRefresh ? <Wifi size={14} /> : <WifiOff size={14} />}
            {autoRefresh ? "Live" : "Paused"}
          </button>
          <button
            onClick={loadAll}
            className="p-2 hover:bg-white/5 rounded-lg transition-colors text-slate-400 hover:text-slate-200"
          >
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {/* ─── Global Speed Limit ──────────────────────────────────────────── */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <h3 className="text-sm font-semibold text-slate-300 mb-4 flex items-center gap-2">
          <Zap size={16} className="text-yellow-400" />
          Global Speed Limit
        </h3>
        <div className="flex items-center gap-4">
          <div className="flex-1">
            <div className="text-3xl font-bold text-slate-100">
              {globalLimitKbps === 0 ? (
                <span className="text-green-400">Unlimited</span>
              ) : (
                <span>
                  {globalLimitKbps >= 1024
                    ? `${(globalLimitKbps / 1024).toFixed(1)} MB/s`
                    : `${globalLimitKbps} KB/s`}
                </span>
              )}
            </div>
            <p className="text-xs text-slate-500 mt-1">
              Applies to all downloads. 0 = unlimited.
            </p>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="number"
              placeholder="KB/s (0=unlimited)"
              value={globalLimitInput}
              onChange={(e) => setGlobalLimitInput(e.target.value)}
              className="px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 w-44 focus:outline-none focus:border-cyan-500/50"
            />
            <button
              onClick={handleSetGlobalLimit}
              className="px-4 py-2 bg-cyan-600 hover:bg-cyan-500 text-white rounded-lg text-sm font-medium transition-colors"
            >
              Set
            </button>
          </div>
        </div>
      </div>

      {/* ─── QoS Overview ────────────────────────────────────────────────── */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <ArrowUpDown size={16} className="text-purple-400" />
            QoS Priority Manager
          </h3>
          <div className="flex items-center gap-2 text-xs text-slate-500">
            <span>{qosStats?.total_active ?? 0} active</span>
            <span className="text-slate-700">|</span>
            <span>
              Limit:{" "}
              {qosStats?.total_bandwidth_limit
                ? formatBps(qosStats.total_bandwidth_limit)
                : "Unlimited"}
            </span>
          </div>
        </div>

        {/* QoS Global Bandwidth Limit */}
        <div className="flex items-center gap-2 mb-4">
          <input
            type="number"
            placeholder="QoS global limit (bytes/sec, 0=unlimited)"
            value={qosLimitInput}
            onChange={(e) => setQosLimitInput(e.target.value)}
            className="flex-1 px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 focus:outline-none focus:border-purple-500/50"
          />
          <button
            onClick={handleSetQosGlobalLimit}
            className="px-4 py-2 bg-purple-600 hover:bg-purple-500 text-white rounded-lg text-sm font-medium transition-colors"
          >
            Set QoS Limit
          </button>
        </div>

        {/* Priority Level Reference */}
        <div className="grid grid-cols-5 gap-2 mb-4">
          {PRIORITY_LEVELS.map((pl) => (
            <div
              key={pl.value}
              className={`px-3 py-2 rounded-lg ${getPriorityBg(pl.label)} text-center`}
            >
              <div className={`text-xs font-bold ${pl.color}`}>{pl.label}</div>
              <div className="text-[10px] text-slate-500">{pl.weight}</div>
            </div>
          ))}
        </div>

        {/* QoS Entries */}
        {qosStats && qosStats.entries.length > 0 ? (
          <div className="space-y-2">
            {qosStats.entries.map((entry) => {
              const pctOfLimit =
                qosStats.total_bandwidth_limit > 0
                  ? (entry.max_bytes_per_sec / qosStats.total_bandwidth_limit) * 100
                  : 0;
              const priorityName =
                typeof entry.priority === "object"
                  ? Object.keys(entry.priority)[0]
                  : String(entry.priority);

              return (
                <div
                  key={entry.download_id}
                  className="flex items-center gap-3 bg-white/[0.02] border border-white/5 rounded-lg px-4 py-3"
                >
                  {/* ID */}
                  <div className="w-32 truncate text-xs text-slate-400 font-mono">
                    {entry.download_id.slice(0, 12)}...
                  </div>

                  {/* Priority Badge */}
                  <div className="relative">
                    {priorityEditId === entry.download_id ? (
                      <select
                        value={priorityEditLevel}
                        onChange={(e) => {
                          const level = e.target.value as PriorityLevel;
                          setPriorityEditLevel(level);
                          handleSetPriority(entry.download_id, level);
                        }}
                        onBlur={() => setPriorityEditId(null)}
                        autoFocus
                        className="px-2 py-1 bg-slate-800 border border-white/10 rounded text-xs text-slate-200"
                      >
                        {PRIORITY_LEVELS.map((l) => (
                          <option key={l.value} value={l.value}>
                            {l.label}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <button
                        onClick={() => {
                          setPriorityEditId(entry.download_id);
                          setPriorityEditLevel(
                            priorityName.toLowerCase() as PriorityLevel
                          );
                        }}
                        className={`px-2 py-1 rounded text-xs font-bold flex items-center gap-1 ${getPriorityBg(
                          priorityName
                        )} ${getPriorityColor(priorityName)}`}
                      >
                        {priorityName}
                        <ChevronDown size={10} />
                      </button>
                    )}
                  </div>

                  {/* Speed */}
                  <div className="flex-1 flex items-center gap-3">
                    <div className="flex-1">
                      <div className="flex items-center justify-between text-[10px] text-slate-500 mb-1">
                        <span>{formatBps(entry.current_bytes_per_sec)}</span>
                        <span>Max: {formatBps(entry.max_bytes_per_sec)}</span>
                      </div>
                      <div className="w-full bg-slate-800 rounded-full h-1.5">
                        <div
                          className="h-full rounded-full bg-gradient-to-r from-cyan-500 to-blue-500 transition-all duration-500"
                          style={{
                            width: `${Math.min(pctOfLimit, 100)}%`,
                          }}
                        />
                      </div>
                    </div>
                  </div>

                  {/* Downloaded */}
                  <div className="text-xs text-slate-500 w-20 text-right">
                    {formatBytes(entry.total_downloaded)}
                  </div>

                  {/* Remove */}
                  <button
                    onClick={() => handleRemoveQos(entry.download_id)}
                    className="p-1.5 hover:bg-red-500/10 text-slate-600 hover:text-red-400 rounded-lg transition-colors"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              );
            })}

            {/* Aggregate bar */}
            {qosStats.total_bandwidth_limit > 0 && (
              <div className="mt-3 px-2">
                <div className="flex justify-between text-[10px] text-slate-500 mb-1">
                  <span>Total current: {formatBps(qosTotalCurrent)}</span>
                  <span>Limit: {formatBps(qosStats.total_bandwidth_limit)}</span>
                </div>
                <div className="w-full bg-slate-800 rounded-full h-2">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-purple-500 to-pink-500 transition-all duration-500"
                    style={{
                      width: `${Math.min(
                        (qosTotalCurrent / qosStats.total_bandwidth_limit) * 100,
                        100
                      )}%`,
                    }}
                  />
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="text-center py-6 text-slate-600 text-sm">
            No downloads tracked by QoS manager
          </div>
        )}
      </div>

      {/* ─── Bandwidth Allocator ─────────────────────────────────────────── */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <BarChart3 size={16} className="text-cyan-400" />
            Bandwidth Allocator
          </h3>
          <div className="flex items-center gap-2">
            <button
              onClick={handleRebalance}
              className="px-3 py-1.5 bg-cyan-600/20 hover:bg-cyan-600/30 text-cyan-400 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors"
            >
              <RefreshCw size={12} />
              Rebalance
            </button>
            <button
              onClick={() => setShowRegister(!showRegister)}
              className="px-3 py-1.5 bg-blue-600/20 hover:bg-blue-600/30 text-blue-400 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors"
            >
              <Plus size={12} />
              Register
            </button>
          </div>
        </div>

        {/* Register Form */}
        {showRegister && (
          <div className="mb-4 p-4 bg-white/[0.02] border border-white/5 rounded-lg space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-[10px] text-slate-500 mb-1">Download ID</label>
                <input
                  value={regId}
                  onChange={(e) => setRegId(e.target.value)}
                  placeholder="download-id"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 focus:outline-none focus:border-cyan-500/50"
                />
              </div>
              <div>
                <label className="block text-[10px] text-slate-500 mb-1">
                  Priority (1-10)
                </label>
                <input
                  type="range"
                  min={1}
                  max={10}
                  value={regPriority}
                  onChange={(e) => setRegPriority(parseInt(e.target.value, 10))}
                  className="w-full accent-cyan-500"
                />
                <div className="text-center text-xs text-cyan-400 font-bold">{regPriority}</div>
              </div>
              <div>
                <label className="block text-[10px] text-slate-500 mb-1">
                  Min B/s (0=none)
                </label>
                <input
                  type="number"
                  value={regMinBps}
                  onChange={(e) => setRegMinBps(e.target.value)}
                  placeholder="0"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 focus:outline-none focus:border-cyan-500/50"
                />
              </div>
              <div>
                <label className="block text-[10px] text-slate-500 mb-1">
                  Max B/s (0=unlimited)
                </label>
                <input
                  type="number"
                  value={regMaxBps}
                  onChange={(e) => setRegMaxBps(e.target.value)}
                  placeholder="0"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 focus:outline-none focus:border-cyan-500/50"
                />
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowRegister(false)}
                className="px-3 py-1.5 text-slate-400 hover:text-slate-200 text-xs"
              >
                Cancel
              </button>
              <button
                onClick={handleRegister}
                className="px-4 py-1.5 bg-cyan-600 hover:bg-cyan-500 text-white rounded-lg text-xs font-medium transition-colors"
              >
                Register
              </button>
            </div>
          </div>
        )}

        {/* Allocation Summary Bar */}
        {allocations.length > 0 && (
          <div className="mb-4">
            <div className="flex justify-between text-[10px] text-slate-500 mb-1">
              <span>Total Allocated: {formatBps(totalAllocated)}</span>
              <span>{allocations.length} downloads</span>
            </div>
            <div className="w-full bg-slate-800 rounded-full h-3 flex overflow-hidden">
              {allocations.map((a, i) => {
                const pct =
                  totalAllocated > 0 ? (a.allocated_bps / totalAllocated) * 100 : 0;
                const colors = [
                  "bg-cyan-500",
                  "bg-blue-500",
                  "bg-purple-500",
                  "bg-pink-500",
                  "bg-orange-500",
                  "bg-green-500",
                  "bg-yellow-500",
                  "bg-red-500",
                ];
                return (
                  <div
                    key={a.download_id}
                    className={`h-full ${colors[i % colors.length]} transition-all duration-500`}
                    style={{ width: `${pct}%` }}
                    title={`${a.download_id.slice(0, 12)}... — ${formatBps(a.allocated_bps)} (P${a.priority})`}
                  />
                );
              })}
            </div>
          </div>
        )}

        {/* Allocations Table */}
        {allocations.length > 0 ? (
          <div className="space-y-2">
            {allocations.map((alloc, i) => {
              const colors = [
                "border-l-cyan-500",
                "border-l-blue-500",
                "border-l-purple-500",
                "border-l-pink-500",
                "border-l-orange-500",
                "border-l-green-500",
                "border-l-yellow-500",
                "border-l-red-500",
              ];
              return (
                <div
                  key={alloc.download_id}
                  className={`flex items-center gap-3 bg-white/[0.02] border border-white/5 border-l-2 ${
                    colors[i % colors.length]
                  } rounded-lg px-4 py-3`}
                >
                  <div className="w-32 truncate text-xs text-slate-400 font-mono">
                    {alloc.download_id.slice(0, 12)}...
                  </div>
                  <div
                    className={`px-2 py-0.5 rounded text-xs font-bold ${getPriorityBg(
                      alloc.priority
                    )} ${getPriorityColor(alloc.priority)}`}
                  >
                    P{alloc.priority}
                  </div>
                  <div className="flex-1 grid grid-cols-3 gap-2 text-xs">
                    <div>
                      <span className="text-slate-600">Allocated:</span>{" "}
                      <span className="text-cyan-400 font-medium">
                        {formatBps(alloc.allocated_bps)}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-600">Min:</span>{" "}
                      <span className="text-slate-300">
                        {alloc.min_bps ? formatBps(alloc.min_bps) : "—"}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-600">Max:</span>{" "}
                      <span className="text-slate-300">
                        {alloc.max_bps ? formatBps(alloc.max_bps) : "Unlimited"}
                      </span>
                    </div>
                  </div>
                  <button
                    onClick={() => handleDeregister(alloc.download_id)}
                    className="p-1.5 hover:bg-red-500/10 text-slate-600 hover:text-red-400 rounded-lg transition-colors"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-center py-6 text-slate-600 text-sm flex flex-col items-center gap-2">
            <Activity size={24} className="text-slate-700" />
            No downloads registered with bandwidth allocator
          </div>
        )}
      </div>

      {/* ─── Priority Weight Reference ───────────────────────────────────── */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <h3 className="text-sm font-semibold text-slate-300 mb-3">How Bandwidth Allocation Works</h3>
        <div className="text-xs text-slate-500 space-y-2">
          <p>
            <strong className="text-slate-400">QoS Manager:</strong> Assigns priority levels
            (Critical → Background) to downloads. When a global QoS limit is set, bandwidth
            is distributed proportionally by priority weight.
          </p>
          <p>
            <strong className="text-slate-400">Bandwidth Allocator:</strong> Fine-grained
            control with numeric priorities (1-10), guaranteed minimums, and max caps. Uses a
            5-phase rebalance algorithm: weight calculation → guarantee minimums → fair-share
            distribution → redistribute excess from capped downloads → apply to token buckets.
          </p>
          <p>
            <strong className="text-slate-400">Global Speed Limit:</strong> Hard cap on total
            download speed across the entire app, enforced via a token bucket rate limiter.
          </p>
        </div>
      </div>
    </div>
  );
};
