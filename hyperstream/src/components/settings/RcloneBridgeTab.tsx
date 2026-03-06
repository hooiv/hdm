import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Cloud,
  RefreshCw,
  FolderOpen,
  ArrowRightLeft,
  Loader2,
  Server,
  HardDrive,
  CheckCircle,
  AlertCircle,
  ChevronRight,
  Info,
} from "lucide-react";

// ─── Types ───────────────────────────────────────────────────────────────────

interface RcloneRemote {
  name: string;
  remote_type: string;
}

// ─── Component ───────────────────────────────────────────────────────────────

export const RcloneBridgeTab: React.FC = () => {
  const [version, setVersion] = useState<string | null>(null);
  const [versionError, setVersionError] = useState<string | null>(null);
  const [remotes, setRemotes] = useState<RcloneRemote[]>([]);
  const [loading, setLoading] = useState(false);

  // Browse state
  const [browsePath, setBrowsePath] = useState("");
  const [browseResult, setBrowseResult] = useState<string | null>(null);
  const [browsing, setBrowsing] = useState(false);

  // Transfer state
  const [source, setSource] = useState("");
  const [destination, setDestination] = useState("");
  const [transferring, setTransferring] = useState(false);
  const [transferResult, setTransferResult] = useState<string | null>(null);
  const [transferError, setTransferError] = useState<string | null>(null);

  // ─── Load ────────────────────────────────────────────────────────────────

  const loadData = async () => {
    setLoading(true);
    try {
      const [ver, rems] = await Promise.all([
        invoke<string>("get_rclone_version").catch(() => null),
        invoke<RcloneRemote[]>("rclone_list_remotes").catch(() => []),
      ]);
      setVersion(ver);
      setVersionError(ver ? null : "rclone not found — install it to use cloud transfers");
      setRemotes(rems);
    } catch {
      setVersionError("Failed to detect rclone");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // ─── Handlers ────────────────────────────────────────────────────────────

  const handleBrowse = async () => {
    if (!browsePath.trim()) return;
    setBrowsing(true);
    setBrowseResult(null);
    try {
      const result = await invoke<string>("get_rclone_ls", {
        remotePath: browsePath.trim(),
      });
      setBrowseResult(result);
    } catch (e) {
      setBrowseResult(`Error: ${e}`);
    } finally {
      setBrowsing(false);
    }
  };

  const handleTransfer = async () => {
    if (!source.trim() || !destination.trim()) return;
    setTransferring(true);
    setTransferResult(null);
    setTransferError(null);
    try {
      const result = await invoke<string>("rclone_transfer", {
        source: source.trim(),
        destination: destination.trim(),
      });
      setTransferResult(result);
    } catch (e) {
      setTransferError(String(e));
    } finally {
      setTransferring(false);
    }
  };

  const remoteTypeIcon = (type: string) => {
    if (type.includes("s3") || type.includes("gcs")) return "bg-orange-500/10 text-orange-400";
    if (type.includes("drive")) return "bg-green-500/10 text-green-400";
    if (type.includes("dropbox")) return "bg-blue-500/10 text-blue-400";
    if (type.includes("onedrive")) return "bg-sky-500/10 text-sky-400";
    if (type.includes("sftp") || type.includes("ftp")) return "bg-yellow-500/10 text-yellow-400";
    return "bg-slate-500/10 text-slate-400";
  };

  // ─── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-slate-100 flex items-center gap-2">
            <Cloud size={22} className="text-sky-400" />
            Rclone Cloud Bridge
          </h2>
          <p className="text-sm text-slate-500 mt-1">
            Transfer files between cloud providers using rclone
          </p>
        </div>
        <button
          onClick={loadData}
          disabled={loading}
          className="p-2 hover:bg-white/5 rounded-lg transition-colors text-slate-400 hover:text-slate-200"
        >
          <RefreshCw size={16} className={loading ? "animate-spin" : ""} />
        </button>
      </div>

      {/* Rclone Status */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <div className="flex items-center gap-3">
          {version ? (
            <>
              <CheckCircle size={18} className="text-green-400" />
              <div>
                <div className="text-sm font-medium text-green-400">rclone detected</div>
                <div className="text-xs text-slate-500">{version}</div>
              </div>
            </>
          ) : versionError ? (
            <>
              <AlertCircle size={18} className="text-red-400" />
              <div>
                <div className="text-sm font-medium text-red-400">rclone not available</div>
                <div className="text-xs text-slate-500">{versionError}</div>
              </div>
            </>
          ) : (
            <>
              <Loader2 size={18} className="text-slate-400 animate-spin" />
              <div className="text-sm text-slate-400">Checking rclone...</div>
            </>
          )}
        </div>
      </div>

      {/* Configured Remotes */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <h3 className="text-sm font-semibold text-slate-300 mb-4 flex items-center gap-2">
          <Server size={16} className="text-sky-400" />
          Configured Remotes ({remotes.length})
        </h3>
        {remotes.length > 0 ? (
          <div className="grid grid-cols-2 gap-2">
            {remotes.map((remote) => (
              <button
                key={remote.name}
                onClick={() => setBrowsePath(`${remote.name}:`)}
                className="flex items-center gap-3 bg-white/[0.02] border border-white/5 rounded-lg p-3 hover:bg-white/[0.04] transition-colors text-left group"
              >
                <div className={`p-2 rounded-lg ${remoteTypeIcon(remote.remote_type)}`}>
                  <HardDrive size={14} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-slate-200 truncate">
                    {remote.name}
                  </div>
                  <div className="text-xs text-slate-500">{remote.remote_type}</div>
                </div>
                <ChevronRight
                  size={14}
                  className="text-slate-600 group-hover:text-slate-400 transition-colors"
                />
              </button>
            ))}
          </div>
        ) : (
          <div className="text-center py-6 text-slate-600 text-sm">
            {version ? (
              <div className="space-y-2">
                <div>No remotes configured</div>
                <div className="flex items-center justify-center gap-1 text-xs text-slate-500">
                  <Info size={12} />
                  Run <code className="px-1 py-0.5 bg-white/5 rounded text-slate-400">rclone config</code> to set up cloud remotes
                </div>
              </div>
            ) : (
              "Install rclone to manage cloud storage"
            )}
          </div>
        )}
      </div>

      {/* Browse Remote */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
          <FolderOpen size={16} className="text-amber-400" />
          Browse Remote
        </h3>
        <div className="flex gap-2 mb-3">
          <input
            value={browsePath}
            onChange={(e) => setBrowsePath(e.target.value)}
            placeholder="remote:path/to/folder"
            className="flex-1 px-4 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500/50"
          />
          <button
            onClick={handleBrowse}
            disabled={browsing || !browsePath.trim()}
            className="px-4 py-2.5 bg-sky-600 hover:bg-sky-500 disabled:opacity-40 text-white rounded-lg text-sm font-medium flex items-center gap-2 transition-colors"
          >
            {browsing ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <FolderOpen size={14} />
            )}
            List
          </button>
        </div>
        {browseResult && (
          <div className="bg-black/30 border border-white/5 rounded-lg p-3 max-h-60 overflow-y-auto custom-scrollbar">
            <pre className="text-xs text-slate-300 whitespace-pre-wrap font-mono">
              {browseResult}
            </pre>
          </div>
        )}
      </div>

      {/* Transfer */}
      <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
        <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
          <ArrowRightLeft size={16} className="text-purple-400" />
          Transfer
        </h3>
        <div className="grid grid-cols-[1fr_auto_1fr] gap-3 items-end mb-3">
          <div>
            <label className="block text-[10px] text-slate-500 mb-1">Source</label>
            <input
              value={source}
              onChange={(e) => setSource(e.target.value)}
              placeholder="remote:path/file.zip"
              className="w-full px-3 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-purple-500/50"
            />
          </div>
          <ArrowRightLeft size={18} className="text-slate-600 mb-2" />
          <div>
            <label className="block text-[10px] text-slate-500 mb-1">Destination</label>
            <input
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
              placeholder="other-remote:path/ or C:\local\path"
              className="w-full px-3 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-purple-500/50"
            />
          </div>
        </div>
        <button
          onClick={handleTransfer}
          disabled={transferring || !source.trim() || !destination.trim()}
          className="w-full px-4 py-2.5 bg-purple-600 hover:bg-purple-500 disabled:opacity-40 text-white rounded-lg text-sm font-medium flex items-center justify-center gap-2 transition-colors"
        >
          {transferring ? (
            <Loader2 size={16} className="animate-spin" />
          ) : (
            <ArrowRightLeft size={16} />
          )}
          Start Transfer
        </button>
        {transferResult && (
          <div className="mt-3 px-3 py-2 bg-green-500/10 border border-green-500/20 rounded-lg text-green-400 text-xs">
            {transferResult}
          </div>
        )}
        {transferError && (
          <div className="mt-3 px-3 py-2 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-xs">
            {transferError}
          </div>
        )}
      </div>
    </div>
  );
};
