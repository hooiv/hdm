import React, { useEffect, useState } from "react";
import type { DownloadTask, Segment } from "../types";
import { useDownloadActions } from "../hooks/useDownloadActions";
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
  Play,
  ArrowUp,
  Globe,
} from "lucide-react";

interface DownloadExpandedPanelProps {
  task: DownloadTask;
  filePath: string;
  onShowPreview: () => void;
  onShowP2PShare: () => void;
}

export const DownloadExpandedPanel: React.FC<DownloadExpandedPanelProps> = ({
  task,
  filePath,
  onShowPreview,
  onShowP2PShare,
}) => {
  const actions = useDownloadActions(task, filePath);
  const toast = useToast();
  const [shareUrl, setShareUrl] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);

  // Reset ephemeral state when the expanded task changes
  useEffect(() => {
    setShareUrl(null);
    setBusyAction(null);
  }, [task.id]);

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
            onClick={withBusy('scrub', () => actions.handleScrubMetadata())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20 disabled:opacity-50"
          >
            <UserX size={14} /> {busyAction === 'scrub' ? 'Scrubbing...' : 'Scrub Metadata'}
          </button>

          {/* Ephemeral Share */}
          <button
            disabled={isBusy}
            onClick={withBusy('share', async () => {
              const url = await actions.handleEphemeralShare();
              if (url) setShareUrl(url);
            })}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 pointer-events-auto disabled:opacity-50"
          >
            <Share2 size={14} /> {busyAction === 'share' ? 'Sharing...' : 'DropBox Share'}
          </button>

          {/* AI Upscale */}
          <button
            disabled={isBusy}
            onClick={withBusy('upscale', () => actions.handleAiUpscale())}
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
            onClick={withBusy('sandbox', () => actions.handleSandbox())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-amber-500/10 text-amber-400 border border-amber-500/20 hover:bg-amber-500/20 disabled:opacity-50"
          >
            <Bug size={14} /> {busyAction === 'sandbox' ? 'Launching...' : 'Run in Sandbox'}
          </button>

          {/* Notarize */}
          <button
            disabled={isBusy}
            onClick={withBusy('notarize', () => actions.handleNotarize())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-teal-500/10 text-teal-400 border border-teal-500/20 hover:bg-teal-500/20 disabled:opacity-50"
          >
            <Shield size={14} /> {busyAction === 'notarize' ? 'Notarizing...' : 'Notarize (TSA)'}
          </button>

          {/* Find Mirrors */}
          <button
            disabled={isBusy}
            onClick={withBusy('mirrors', () => actions.handleFindMirrors())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50"
          >
            <Search size={14} /> {busyAction === 'mirrors' ? 'Searching...' : 'Find Mirrors'}
          </button>

          {/* Flash to USB */}
          {task.filename.toLowerCase().endsWith(".iso") && (
            <button
              disabled={isBusy}
              onClick={withBusy('flash', () => actions.handleFlashToUsb())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
            >
              <Zap size={14} /> {busyAction === 'flash' ? 'Flashing...' : 'Flash to USB'}
            </button>
          )}

          {/* Validate C2PA */}
          {isImageFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('c2pa', () => actions.handleValidateC2pa())}
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
                onClick={withBusy('stegoHide', () => actions.handleStegoHide())}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20 disabled:opacity-50"
              >
                <Camera size={14} /> {busyAction === 'stegoHide' ? 'Hiding...' : 'Stego Hide'}
              </button>
              <button
                disabled={isBusy}
                onClick={withBusy('stegoExtract', () => actions.handleStegoExtract())}
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
              onClick={withBusy('extract', () => actions.handleAutoExtract())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-lime-500/10 text-lime-400 border border-lime-500/20 hover:bg-lime-500/20 disabled:opacity-50"
            >
              <Archive size={14} /> {busyAction === 'extract' ? 'Extracting...' : 'Extract'}
            </button>
          )}

          {/* SQL Query */}
          {isDataFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('sql', () => actions.handleSqlQuery())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-sky-500/10 text-sky-400 border border-sky-500/20 hover:bg-sky-500/20 disabled:opacity-50"
            >
              <FileText size={14} /> {busyAction === 'sql' ? 'Querying...' : 'SQL Query'}
            </button>
          )}

          {/* Cast to TV */}
          {isMediaFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('dlna', () => actions.handleDlnaCast())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-rose-500/10 text-rose-400 border border-rose-500/20 hover:bg-rose-500/20 disabled:opacity-50"
            >
              <Play size={14} /> {busyAction === 'dlna' ? 'Discovering...' : 'Cast to TV'}
            </button>
          )}

          {/* Subtitles */}
          {isVideoFile && (
            <button
              disabled={isBusy}
              onClick={withBusy('subtitles', () => actions.handleGenerateSubtitles())}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20 disabled:opacity-50"
            >
              <Film size={14} /> {busyAction === 'subtitles' ? 'Generating...' : 'Subtitles'}
            </button>
          )}

          {/* QoS Priority (only for active/paused downloads) */}
          {task.status !== "Done" && (
          <button
            disabled={isBusy}
            onClick={withBusy('priority', () => actions.handleSetPriority())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 hover:bg-indigo-500/20 disabled:opacity-50"
          >
            <ArrowUp size={14} /> Priority
          </button>
          )}

          {/* Mod Optimizer */}
          <button
            disabled={isBusy}
            onClick={withBusy('mods', () => actions.handleOptimizeMods())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-yellow-500/10 text-yellow-500 border border-yellow-500/20 hover:bg-yellow-500/20 disabled:opacity-50"
          >
            <Zap size={14} /> {busyAction === 'mods' ? 'Optimizing...' : 'Mod Optimizer'}
          </button>

          {shareUrl && (
            <div className="w-full mt-2 p-2 bg-cyan-500/5 border border-cyan-500/20 rounded-md text-xs text-cyan-400 font-mono break-all pointer-events-auto">
              🔗 {shareUrl}
            </div>
          )}
        </div>
      )}

      {/* Actions for Error/Paused downloads */}
      {(task.status === "Error" || task.status === "Paused") && (
        <div className="mt-4 pt-3 border-t border-slate-700/30 flex flex-wrap gap-2">
          <button
            disabled={isBusy}
            onClick={withBusy('refresh', () => actions.handleRefreshUrl())}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20 disabled:opacity-50"
          >
            <RefreshCw size={14} /> Refresh Address
          </button>

          {task.status === "Error" && (
            <button
              disabled={isBusy}
              onClick={withBusy('wayback', () => actions.handleWaybackCheck())}
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
          Server: <span className="text-slate-300 ml-1">Multi-Threaded</span>
        </div>
      </div>
    </div>
  );
};
