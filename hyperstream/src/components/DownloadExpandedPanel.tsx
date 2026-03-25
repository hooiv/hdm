import React, { useEffect, useRef, useState } from "react";
import type { DiscoveredMirror, DownloadTask, Segment } from "../types";
import { useDownloadActions } from "../hooks/useDownloadActions";
import { useFileActions } from "../hooks/useFileActions";
import { useMediaActions } from "../hooks/useMediaActions";
import { useNetworkActions } from "../hooks/useNetworkActions";
import { useToast } from "../contexts/ToastContext";
import { ThreadVisualizer } from "./ThreadVisualizer";
import { invoke } from "@tauri-apps/api/core";
import {
  Archive,
  HardDrive,
  Cloud,
  Film,
  UserX,
  Share2,
  Bug,
  Shield,
  Search,
  FileText,
  Zap,
  Camera,
  RefreshCw,
  RotateCcw,
  Play,
  ArrowUp,
  Globe,
  Activity,
  ShieldCheck,
} from "lucide-react";

const formatSpeed = (bytes: number): string => {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(1) + ' GB/s';
  if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + ' MB/s';
  if (bytes >= 1024) return (bytes / 1024).toFixed(0) + ' KB/s';
  return bytes.toFixed(0) + ' B/s';
};

const SpeedChart: React.FC<{ samples: { t: number; v: number }[] }> = ({ samples }) => {
  const W = 400, H = 60;
  const maxSpeed = Math.max(...samples.map(s => s.v), 1);
  const avgSpeed = samples.reduce((s, p) => s + p.v, 0) / samples.length;
  const peakSpeed = Math.max(...samples.map(s => s.v));

  const points = samples.map((s, i) => {
    const x = (i / (samples.length - 1)) * W;
    const y = H - (s.v / maxSpeed) * (H - 4);
    return `${x},${y}`;
  });
  const areaPath = `M0,${H} L${points.join(' L')} L${W},${H} Z`;
  const linePath = `M${points.join(' L')}`;

  return (
    <div>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-16" preserveAspectRatio="none">
        <defs>
          <linearGradient id="speedGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgb(6, 182, 212)" stopOpacity="0.3" />
            <stop offset="100%" stopColor="rgb(6, 182, 212)" stopOpacity="0.02" />
          </linearGradient>
        </defs>
        <path d={areaPath} fill="url(#speedGrad)" />
        <path d={linePath} fill="none" stroke="rgb(6, 182, 212)" strokeWidth="1.5" strokeLinejoin="round" />
        {/* avg line */}
        <line
          x1="0" y1={H - (avgSpeed / maxSpeed) * (H - 4)}
          x2={W} y2={H - (avgSpeed / maxSpeed) * (H - 4)}
          stroke="rgb(148, 163, 184)" strokeWidth="0.5" strokeDasharray="4 3" opacity="0.5"
        />
      </svg>
      <div className="flex items-center justify-between text-xs mt-1">
        <span className="text-slate-500">Avg: <span className="text-cyan-400 font-mono">{formatSpeed(avgSpeed)}</span></span>
        <span className="text-slate-500">Peak: <span className="text-emerald-400 font-mono">{formatSpeed(peakSpeed)}</span></span>
        <span className="text-slate-500">Now: <span className="text-slate-300 font-mono">{formatSpeed(samples[samples.length - 1]?.v || 0)}</span></span>
      </div>
    </div>
  );
};

interface DownloadExpandedPanelProps {
  task: DownloadTask;
  filePath: string;
  onResume: (id: string) => void;
  onDiscoveredMirrors?: (id: string, mirrors: DiscoveredMirror[]) => void;
  onShowPreview: () => void;
  onShowP2PShare: () => void;
}

export const DownloadExpandedPanel: React.FC<DownloadExpandedPanelProps> = ({
  task,
  filePath,
  onResume,
  onDiscoveredMirrors,
  onShowPreview,
  onShowP2PShare,
}) => {
  const downloadActions = useDownloadActions(task);
  const fileActions = useFileActions(filePath);
  const mediaActions = useMediaActions(filePath);
  const networkActions = useNetworkActions(task, { onDiscoveredMirrors });
  const toast = useToast();
  const [shareUrl, setShareUrl] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const discoveredMirrorCount = task.discoveredMirrors?.length ?? 0;

  // Speed history tracking
  const MAX_SAMPLES = 120; // ~2 min at 1 sample/sec
  const speedHistory = useRef<{ t: number; v: number }[]>([]);
  const [, forceRender] = useState(0);

  // Reset ephemeral state when the expanded task changes
  useEffect(() => {
    setShareUrl(null);
    setBusyAction(null);
    speedHistory.current = [];
  }, [task.id]);

  // Sample speed every second while downloading
  useEffect(() => {
    if (task.status !== 'Downloading') return;
    const interval = setInterval(() => {
      const arr = speedHistory.current;
      arr.push({ t: Date.now(), v: task.speed || 0 });
      if (arr.length > MAX_SAMPLES) arr.shift();
      forceRender(c => c + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [task.id, task.status, task.speed]);

  /** Wraps an async action with busy-state tracking to prevent double-clicks. */
  const withBusy = (name: string, fn: () => Promise<unknown>) => async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (busyAction) return;
    setBusyAction(name);
    try { await fn(); } finally { setBusyAction(null); }
  };

  const isBusy = busyAction !== null;

  /** Extract the effective file extension, handling compound extensions like .tar.gz */
  const getExtension = (name: string): string => {
    const lower = name.toLowerCase();
    // Check compound extensions first
    const compoundExts = ['.tar.gz', '.tar.bz2', '.tar.xz', '.tar.zst'];
    for (const ext of compoundExts) {
      if (lower.endsWith(ext)) return ext.slice(1); // e.g. "tar.gz"
    }
    return lower.split('.').pop() || '';
  };

  const ext = getExtension(task.filename);
  const isMountable = ['zip', 'iso'].includes(ext);
  const isArchive = ['zip', 'jar', 'rar', '7z', 'tgz', 'tar.gz', 'tar.bz2', 'tar.xz', 'tar.zst', 'gz', 'bz2', 'xz'].includes(ext);
  const isDataFile = ['csv', 'json'].includes(ext);
  const isMediaFile = ['mp4', 'mkv', 'avi', 'mp3', 'flac', 'wav'].includes(ext);
  const isVideoFile = ['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(ext);
  const isImageFile = ['jpg', 'jpeg', 'png', 'webp', 'gif'].includes(ext);

  return (
    <div className="p-4">
      {/* Thread Visualization */}
      <ThreadVisualizer segments={task.segments || []} totalSize={task.total} />

      {/* Speed History Graph */}
      {speedHistory.current.length > 1 && (
        <div className="mt-3 p-3 bg-slate-900/50 rounded-lg border border-slate-700/30">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-1.5 text-xs text-slate-400">
              <Activity size={12} className="text-cyan-400" />
              <span>Speed History</span>
            </div>
            <span className="text-xs text-slate-500 font-mono">
              {speedHistory.current.length}s
            </span>
          </div>
          <SpeedChart samples={speedHistory.current} />
        </div>
      )}

      {/* Advanced Actions Toolbar */}
      {task.status === "Done" && (
        <div className="mt-4 pt-3 border-t border-slate-700/30 flex flex-wrap gap-2">
          {/* Archive Preview */}
          {(task.filename.endsWith(".zip") ||
            task.filename.endsWith(".jar")) && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onShowPreview();
                }}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-blue-500/10 text-blue-400 border border-blue-500/20 hover:bg-blue-500/20"
              >
                <Archive size={14} /> Browse Content
              </button>
            )}

          {/* Mount Drive */}
          {isMountable && (
            <button
              disabled={isBusy}
              onClick={withBusy('mount', async () => {
                try {
                  const port = await invoke("mount_drive", {
                    path: filePath,
                    letter: "Z",
                  });
                  toast.success(
                    `Mounted on WebDAV Port: ${port}.\n\nUse 'Map Network Drive' to http://127.0.0.1:${port}`,
                  );
                } catch (err) {
                  toast.error("Mount failed: " + err);
                }
              })}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-green-500/10 text-green-400 border border-green-500/20 hover:bg-green-500/20 disabled:opacity-50"
            >
              <HardDrive size={14} /> {busyAction === 'mount' ? 'Mounting...' : 'Mount Drive'}
            </button>
          )}

          {/* Cloud Upload */}
          <button
            disabled={isBusy}
            onClick={withBusy('upload', async () => {
              if (!confirm("Upload to configured Cloud Storage?")) return;
              try {
                toast.info("Upload started... please wait.");
                const result = await invoke("upload_to_cloud", {
                  path: filePath,
                  targetName: null,
                });
                toast.success("Success: " + result);
              } catch (err) {
                toast.error("Upload failed: " + err);
              }
            })}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20 disabled:opacity-50"
          >
            <Cloud size={14} /> {busyAction === 'upload' ? 'Uploading...' : 'Upload to Cloud'}
          </button>

          {/* Media Tools */}
          {isVideoFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('preview', async () => {
                try {
                  toast.info("Generating Preview (WebP)...");
                  await invoke("process_media", {
                    path: filePath,
                    action: "preview",
                  });
                  toast.success("Preview Generated!");
                } catch (err) {
                  toast.error("Media Process Failed: " + err);
                }
              })}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20 disabled:opacity-50"
            >
              <Film size={14} /> {busyAction === 'preview' ? 'Generating...' : 'Smart Preview'}
            </button>
          )}

          {/* Metadata Scrub */}
          <button
            disabled={isBusy}
            onClick={withBusy('scrub', () => fileActions.handleScrubMetadata())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20 disabled:opacity-50"
          >
            <UserX size={14} /> {busyAction === 'scrub' ? 'Scrubbing...' : 'Scrub Metadata'}
          </button>

          {/* Ephemeral Share */}
          <button
            disabled={isBusy}
            onClick={withBusy('share', async () => {
              const url = await fileActions.handleEphemeralShare();
              if (url) setShareUrl(url);
            })}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 pointer-events-auto disabled:opacity-50"
          >
            <Share2 size={14} /> {busyAction === 'share' ? 'Sharing...' : 'DropBox Share'}
          </button>

          {/* AI Upscale */}
          <button
            disabled={isBusy}
            onClick={withBusy('upscale', () => mediaActions.handleAiUpscale())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50"
          >
            <Film size={14} /> {busyAction === 'upscale' ? 'Upscaling...' : 'AI Upscale'}
          </button>

          {/* P2P Share */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              onShowP2PShare();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-fuchsia-500/10 text-fuchsia-400 border border-fuchsia-500/20 hover:bg-fuchsia-500/20"
          >
            <Globe size={14} /> P2P Torrent
          </button>

          {/* Sandbox */}
          <button
            disabled={isBusy}
            onClick={withBusy('sandbox', () => fileActions.handleSandbox())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-amber-500/10 text-amber-400 border border-amber-500/20 hover:bg-amber-500/20 disabled:opacity-50"
          >
            <Bug size={14} /> {busyAction === 'sandbox' ? 'Launching...' : 'Run in Sandbox'}
          </button>

          {/* Notarize */}
          <button
            disabled={isBusy}
            onClick={withBusy('notarize', () => fileActions.handleNotarize())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-teal-500/10 text-teal-400 border border-teal-500/20 hover:bg-teal-500/20 disabled:opacity-50"
          >
            <Shield size={14} /> {busyAction === 'notarize' ? 'Notarizing...' : 'Notarize (TSA)'}
          </button>

          {/* Find Mirrors */}
          <button
            disabled={isBusy}
            onClick={withBusy('mirrors', () => networkActions.handleFindMirrors())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50"
          >
            <Search size={14} /> {busyAction === 'mirrors' ? 'Searching...' : 'Find Mirrors'}
          </button>

          {/* Flash to USB */}
          {task.filename.toLowerCase().endsWith(".iso") && (
            <button
              disabled={isBusy}
              onClick={withBusy('flash', () => fileActions.handleFlashToUsb())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
            >
              <Zap size={14} /> {busyAction === 'flash' ? 'Flashing...' : 'Flash to USB'}
            </button>
          )}

          {/* Validate C2PA */}
          {isImageFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('c2pa', () => fileActions.handleValidateC2pa())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-blue-500/10 text-blue-400 border border-blue-500/20 hover:bg-blue-500/20 disabled:opacity-50"
            >
              <Shield size={14} /> {busyAction === 'c2pa' ? 'Validating...' : 'Validate C2PA'}
            </button>
          )}

          {/* Steganography - hide/extract */}
          {task.filename.toLowerCase().endsWith(".png") && (
            <>
              <button
                disabled={isBusy}
                onClick={withBusy('stegoHide', () => fileActions.handleStegoHide())}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20 disabled:opacity-50"
              >
                <Camera size={14} /> {busyAction === 'stegoHide' ? 'Hiding...' : 'Stego Hide'}
              </button>
              <button
                disabled={isBusy}
                onClick={withBusy('stegoExtract', () => fileActions.handleStegoExtract())}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20 disabled:opacity-50"
              >
                <Search size={14} /> {busyAction === 'stegoExtract' ? 'Extracting...' : 'Stego Extract'}
              </button>
            </>
          )}

          {/* Extract Archive */}
          {isArchive && (
            <button
              disabled={isBusy}
              onClick={withBusy('extract', () => fileActions.handleAutoExtract())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-lime-500/10 text-lime-400 border border-lime-500/20 hover:bg-lime-500/20 disabled:opacity-50"
            >
              <Archive size={14} /> {busyAction === 'extract' ? 'Extracting...' : 'Extract'}
            </button>
          )}

          {/* Checksum Verification */}
          {task.status === 'Done' && (
            <button
              disabled={isBusy}
              onClick={withBusy('checksum', () => fileActions.handleVerifyChecksum())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 hover:bg-indigo-500/20 disabled:opacity-50"
            >
              <ShieldCheck size={14} /> {busyAction === 'checksum' ? 'Verifying...' : 'Verify Checksum'}
            </button>
          )}

          {/* SQL Query */}
          {isDataFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('sql', () => fileActions.handleSqlQuery())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-sky-500/10 text-sky-400 border border-sky-500/20 hover:bg-sky-500/20 disabled:opacity-50"
            >
              <FileText size={14} /> {busyAction === 'sql' ? 'Querying...' : 'SQL Query'}
            </button>
          )}

          {/* Cast to TV */}
          {isMediaFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('dlna', () => mediaActions.handleDlnaCast())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-rose-500/10 text-rose-400 border border-rose-500/20 hover:bg-rose-500/20 disabled:opacity-50"
            >
              <Play size={14} /> {busyAction === 'dlna' ? 'Discovering...' : 'Cast to TV'}
            </button>
          )}

          {/* Subtitles */}
          {isVideoFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('subtitles', () => mediaActions.handleGenerateSubtitles())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20 disabled:opacity-50"
            >
              <Film size={14} /> {busyAction === 'subtitles' ? 'Generating...' : 'Subtitles'}
            </button>
          )}

          {/* QoS Priority (only for active/paused downloads) */}
          {task.status !== "Done" && (
          <button
            disabled={isBusy}
            onClick={withBusy('priority', () => downloadActions.handleSetPriority())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 hover:bg-indigo-500/20 disabled:opacity-50"
          >
            <ArrowUp size={14} /> Priority
          </button>
          )}

          {/* Mod Optimizer */}
          <button
            disabled={isBusy}
            onClick={withBusy('mods', () => mediaActions.handleOptimizeMods())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-yellow-500/10 text-yellow-500 border border-yellow-500/20 hover:bg-yellow-500/20 disabled:opacity-50"
          >
            <Zap size={14} /> {busyAction === 'mods' ? 'Optimizing...' : 'Mod Optimizer'}
          </button>

          {/* API Fuzz — discover alternate download endpoints */}
          {task.url && (
            <button
              disabled={isBusy}
              onClick={withBusy('fuzz', () => networkActions.handleApiFuzz())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
            >
              <Search size={14} /> {busyAction === 'fuzz' ? 'Fuzzing...' : 'Fuzz URL'}
            </button>
          )}

          {/* HTTP Replay — replay the original request and inspect the response */}
          {task.url && (
            <button
              disabled={isBusy}
              onClick={withBusy('replay', () => networkActions.handleApiReplay())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-teal-500/10 text-teal-400 border border-teal-500/20 hover:bg-teal-500/20 disabled:opacity-50"
            >
              <RotateCcw size={14} /> {busyAction === 'replay' ? 'Replaying...' : 'HTTP Replay'}
            </button>
          )}

          {shareUrl && (
            <div className="w-full mt-2 p-2 bg-cyan-500/5 border border-cyan-500/20 rounded-md text-xs text-cyan-400 font-mono break-all pointer-events-auto flex items-center gap-2">
              <span className="flex-1">🔗 {shareUrl}</span>
              <button
                onClick={async () => {
                  try {
                    const shares = await invoke<{ id: string; url: string }[]>('list_ephemeral_shares');
                    const match = shares.find(s => shareUrl.includes(`:${s.url.split(':').pop()}`));
                    if (match) {
                      await invoke('stop_ephemeral_share', { id: match.id });
                      setShareUrl(null);
                    }
                  } catch { /* ignore */ }
                }}
                className="flex-shrink-0 px-2 py-1 rounded bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors border border-red-500/30 text-[10px] font-bold"
              >
                Stop
              </button>
            </div>
          )}
        </div>
      )}

      {/* Actions for Error/Paused downloads */}
      {(task.status === "Error" || task.status === "Paused") && (
        <div className="mt-4 pt-3 border-t border-slate-700/30 flex flex-wrap gap-2">
          {discoveredMirrorCount > 0 && (
            <>
              <button
                disabled={isBusy}
                onClick={() => onResume(task.id)}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-300 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50"
              >
                <Play size={14} />
                {`Resume with ${discoveredMirrorCount} Mirror${discoveredMirrorCount === 1 ? '' : 's'}`}
              </button>
              <div className="basis-full text-[10px] text-emerald-300/80">
                {discoveredMirrorCount} recovery-ready mirror candidate{discoveredMirrorCount === 1 ? '' : 's'} saved for this download.
              </div>
            </>
          )}

          <button
            disabled={isBusy}
            onClick={withBusy('refresh', () => downloadActions.handleRefreshUrl())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20 disabled:opacity-50"
          >
            <RefreshCw size={14} /> Refresh Address
          </button>

          {task.status === "Error" && (
            <button
              disabled={isBusy}
              onClick={withBusy('wayback', () => downloadActions.handleWaybackCheck())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
            >
              <Globe size={14} />{" "}
              {busyAction === 'wayback' ? "Searching..." : "Try Wayback Machine"}
            </button>
          )}
        </div>
      )}

      {/* More Details Grid */}
      <div className="grid grid-cols-3 gap-3 text-xs text-slate-500 mt-3 p-3 bg-slate-900/50 rounded-lg border border-slate-700/30">
        <div>
          ID:{" "}
          <span className="text-slate-300 font-mono ml-1">
            {task.id.split("_").pop()}
          </span>
        </div>
        <div>
          Threads:{" "}
          <span className="text-slate-300 ml-1">
            {
              (task.segments || []).filter((s: Segment) => s.state === "Downloading")
                .length
            }
          </span>
        </div>
        <div>
          Server: <span className="text-slate-300 ml-1">{task.mirrorStats && task.mirrorStats.length > 1 ? 'Multi-Source' : 'Multi-Threaded'}</span>
        </div>
        <div className="col-span-3">
          URL:{" "}
          <span className="text-slate-300 font-mono ml-1 break-all">
            {task.url}
          </span>
        </div>
        <div className="col-span-3">
          Save Path:{" "}
          <span className="text-slate-300 font-mono ml-1 break-all">
            {filePath}
          </span>
        </div>
        <div>
          Date Added:{" "}
          <span className="text-slate-300 font-mono ml-1">
            {new Date(task.dateAdded).toLocaleString()}
          </span>
        </div>
      </div>

      {/* Live Mirror Stats */}
      {task.mirrorStats && task.mirrorStats.length > 1 && (
        <div className="mt-3 p-3 bg-slate-900/50 rounded-lg border border-emerald-500/20">
          <div className="flex items-center gap-2 mb-2">
            <Globe size={12} className="text-emerald-400" />
            <span className="text-xs font-semibold text-emerald-400">Mirror Sources</span>
            <button
              disabled={isBusy}
              onClick={withBusy('arbitrage', async () => {
                const urls = task.mirrorStats!
                  .filter(m => !m.disabled && !m.quarantined)
                  .map(m => m.url);
                if (urls.length < 2) { toast.error('Need at least 2 active mirrors to probe'); return; }
                toast.info('Probing mirror bandwidth...');
                const results = await invoke<{ url: string; speed_bytes_per_sec: number; latency_ms: number; supports_range: boolean; status: number }[]>('arbitrage_download', { urls });
                const sorted = results.sort((a, b) => b.speed_bytes_per_sec - a.speed_bytes_per_sec);
                const lines = sorted.slice(0, 5).map((r, i) =>
                  `${i === 0 ? '🏆' : '  '} ${(r.speed_bytes_per_sec / 1024).toFixed(0)} KB/s — ${r.latency_ms}ms — ${new URL(r.url).hostname}`
                ).join('\n');
                toast.success(`Bandwidth Probe:\n${lines}`);
              })}
              className="ml-2 px-2 py-0.5 rounded text-[10px] font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
            >
              {busyAction === 'arbitrage' ? 'Probing...' : 'Probe Speeds'}
            </button>
            <span className="ml-auto text-[10px] text-slate-500">
              {task.mirrorStats.filter(m => !m.disabled && !m.quarantined).length} active
              {task.mirrorStats.some(m => m.quarantined) ? ` · ${task.mirrorStats.filter(m => m.quarantined).length} quarantined` : ''}
            </span>
          </div>
          <div className="space-y-1">
            {task.mirrorStats.map((m, i) => {
              const speedKB = m.avg_speed_bps > 0 ? (m.avg_speed_bps / 1024).toFixed(0) : '0';
              const maxSpeed = Math.max(
                ...task.mirrorStats!
                  .filter(ms => !ms.disabled && !ms.quarantined)
                  .map(ms => ms.avg_speed_bps),
                1,
              );
              const barWidth = (m.avg_speed_bps / maxSpeed) * 100;
              const isUnavailable = m.disabled || m.quarantined;
              const dotClass = m.disabled
                ? 'bg-red-500'
                : m.quarantined
                  ? 'bg-amber-400'
                  : m.canonical
                    ? 'bg-emerald-400'
                    : 'bg-blue-400';
              const barClass = m.canonical ? 'bg-emerald-500' : m.quarantined ? 'bg-amber-500' : 'bg-blue-500';
              return (
                <div key={i} className={`rounded border px-2 py-1.5 ${isUnavailable ? 'border-slate-700/50 opacity-60' : 'border-slate-700/20'}`}>
                  <div className="flex items-center gap-2 text-[10px]">
                    <span className={`w-1.5 h-1.5 rounded-full ${dotClass}`} />
                    <span className="text-slate-300 truncate w-20" title={m.url}>{m.source}</span>
                    {m.canonical && <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[9px] text-emerald-300">canonical</span>}
                    {m.quarantined && <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[9px] text-amber-300">quarantined</span>}
                    {!m.quarantined && !m.disabled && m.identity_status === 'verified' && (
                      <span className="rounded bg-cyan-500/10 px-1.5 py-0.5 text-[9px] text-cyan-300">verified</span>
                    )}
                    <div className="flex-1 h-1 bg-slate-800 rounded-full overflow-hidden">
                      <div className={`h-full rounded-full ${barClass}`} style={{ width: `${barWidth}%` }} />
                    </div>
                    <span className="text-slate-400 font-mono w-16 text-right">{speedKB} KB/s</span>
                    <span className="text-slate-500 font-mono w-12 text-right">{m.latency_ms < 999999 ? `${m.latency_ms}ms` : '—'}</span>
                  </div>
                  <div className="mt-1 flex items-center justify-between text-[9px] text-slate-500">
                    <span>{m.supports_range ? 'Range OK' : 'No range support'}</span>
                    <span>{m.success_count} ok · {m.error_count} err</span>
                  </div>
                  {m.quarantine_reason && (
                    <div className="mt-1 text-[9px] text-amber-300" title={m.quarantine_reason}>
                      Rejected: {m.quarantine_reason}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
};
