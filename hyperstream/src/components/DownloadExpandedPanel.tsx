import React, { useState } from "react";
import { DownloadTask } from "./DownloadItem";
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
  const [checkingWayback, setCheckingWayback] = useState(false);

  const isMountable = ["zip", "iso"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );
  const isArchive = ["zip", "jar", "rar", "7z", "tgz"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );
  const isDataFile = ["csv", "json"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );
  const isMediaFile = ["mp4", "mkv", "avi", "mp3", "flac", "wav"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );
  const isVideoFile = ["mp4", "mkv", "avi", "mov", "webm"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );
  const isImageFile = ["jpg", "jpeg", "png", "webp", "gif"].includes(
    task.filename.split(".").pop()?.toLowerCase() || "",
  );

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
              onClick={async (e) => {
                e.stopPropagation();
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
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-green-500/10 text-green-400 border border-green-500/20 hover:bg-green-500/20"
            >
              <HardDrive size={14} /> Mount Drive
            </button>
          )}

          {/* Cloud Upload */}
          <button
            onClick={async (e) => {
              e.stopPropagation();
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
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20"
          >
            <Cloud size={14} /> Upload to Cloud
          </button>

          {/* Media Tools */}
          {isVideoFile && (
            <button
              onClick={async (e) => {
                e.stopPropagation();
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
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20"
            >
              <Film size={14} /> Smart Preview
            </button>
          )}

          {/* Metadata Scrub */}
          <button
            onClick={async (e) => {
              e.stopPropagation();
              await actions.handleScrubMetadata();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20"
          >
            <UserX size={14} /> Scrub Metadata
          </button>

          {/* Ephemeral Share */}
          <button
            onClick={async (e) => {
              e.stopPropagation();
              const url = await actions.handleEphemeralShare();
              if (url) setShareUrl(url);
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 pointer-events-auto"
          >
            <Share2 size={14} /> DropBox Share
          </button>

          {/* AI Upscale */}
          <button
            onClick={async (e) => {
              e.stopPropagation();
              await actions.handleAiUpscale();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20"
          >
            <Film size={14} /> AI Upscale
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
            onClick={(e) => {
              e.stopPropagation();
              actions.handleSandbox();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-amber-500/10 text-amber-400 border border-amber-500/20 hover:bg-amber-500/20"
          >
            <Bug size={14} /> Run in Sandbox
          </button>

          {/* Notarize */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              actions.handleNotarize();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-teal-500/10 text-teal-400 border border-teal-500/20 hover:bg-teal-500/20"
          >
            <Shield size={14} /> Notarize (TSA)
          </button>

          {/* Find Mirrors */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              actions.handleFindMirrors();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20"
          >
            <Search size={14} /> Find Mirrors
          </button>

          {/* Flash to USB */}
          {task.filename.toLowerCase().endsWith(".iso") && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleFlashToUsb();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20"
            >
              <Zap size={14} /> Flash to USB
            </button>
          )}

          {/* Validate C2PA */}
          {isImageFile && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleValidateC2pa();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-blue-500/10 text-blue-400 border border-blue-500/20 hover:bg-blue-500/20"
            >
              <Shield size={14} /> Validate C2PA
            </button>
          )}

          {/* Steganography - hide/extract */}
          {task.filename.toLowerCase().endsWith(".png") && (
            <>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  actions.handleStegoHide();
                }}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20"
              >
                <Camera size={14} /> Stego Hide
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  actions.handleStegoExtract();
                }}
                className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20"
              >
                <Search size={14} /> Stego Extract
              </button>
            </>
          )}

          {/* Extract Archive */}
          {isArchive && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleAutoExtract();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-lime-500/10 text-lime-400 border border-lime-500/20 hover:bg-lime-500/20"
            >
              <Archive size={14} /> Extract
            </button>
          )}

          {/* SQL Query */}
          {isDataFile && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleSqlQuery();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-sky-500/10 text-sky-400 border border-sky-500/20 hover:bg-sky-500/20"
            >
              <FileText size={14} /> SQL Query
            </button>
          )}

          {/* Cast to TV */}
          {isMediaFile && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleDlnaCast();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-rose-500/10 text-rose-400 border border-rose-500/20 hover:bg-rose-500/20"
            >
              <Play size={14} /> Cast to TV
            </button>
          )}

          {/* Subtitles */}
          {isVideoFile && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                actions.handleGenerateSubtitles();
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-pink-500/10 text-pink-400 border border-pink-500/20 hover:bg-pink-500/20"
            >
              <Film size={14} /> Subtitles
            </button>
          )}

          {/* QoS Priority */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              actions.handleSetPriority();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 hover:bg-indigo-500/20"
          >
            <ArrowUp size={14} /> Priority
          </button>

          {/* Mod Optimizer */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              actions.handleOptimizeMods();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-yellow-500/10 text-yellow-500 border border-yellow-500/20 hover:bg-yellow-500/20"
          >
            <Zap size={14} /> Mod Optimizer
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
            onClick={(e) => {
              e.stopPropagation();
              actions.handleRefreshUrl();
            }}
            className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-purple-500/10 text-purple-400 border border-purple-500/20 hover:bg-purple-500/20"
          >
            <RefreshCw size={14} /> Refresh Address
          </button>

          {task.status === "Error" && (
            <button
              disabled={checkingWayback}
              onClick={async (e) => {
                e.stopPropagation();
                setCheckingWayback(true);
                await actions.handleWaybackCheck();
                setCheckingWayback(false);
              }}
              className="px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-2 transition-colors bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 disabled:opacity-50"
            >
              <Globe size={14} />{" "}
              {checkingWayback ? "Searching..." : "🕸 Try Wayback Machine"}
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
              (task.segments || []).filter((s) => s.state === "Downloading")
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
