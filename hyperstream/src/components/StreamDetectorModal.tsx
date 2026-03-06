import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import {
  Video,
  Search,
  X,
  Play,
  Download,
  Loader2,
  Radio,
  Music,
  Film,
  Globe,
  Copy,
  Check,
  AlertCircle,
} from "lucide-react";

// ─── Types ───────────────────────────────────────────────────────────────────

interface DetectedStream {
  url: string;
  stream_type: string; // "Hls" | "Dash" | "DirectVideo" | "DirectAudio"
  quality: string | null;
  content_type: string | null;
  estimated_size: number | null;
  title: string | null;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function getStreamIcon(type: string) {
  switch (type) {
    case "Hls": return <Radio size={16} className="text-purple-400" />;
    case "Dash": return <Film size={16} className="text-blue-400" />;
    case "DirectVideo": return <Video size={16} className="text-cyan-400" />;
    case "DirectAudio": return <Music size={16} className="text-pink-400" />;
    default: return <Globe size={16} className="text-slate-400" />;
  }
}

function getStreamColor(type: string) {
  switch (type) {
    case "Hls": return "bg-purple-500/10 text-purple-400 border-purple-500/20";
    case "Dash": return "bg-blue-500/10 text-blue-400 border-blue-500/20";
    case "DirectVideo": return "bg-cyan-500/10 text-cyan-400 border-cyan-500/20";
    case "DirectAudio": return "bg-pink-500/10 text-pink-400 border-pink-500/20";
    default: return "bg-slate-500/10 text-slate-400 border-slate-500/20";
  }
}

// ─── Component ───────────────────────────────────────────────────────────────

interface StreamDetectorModalProps {
  isOpen: boolean;
  onClose: () => void;
  onDownload?: (url: string, filename?: string) => void;
}

export const StreamDetectorModal: React.FC<StreamDetectorModalProps> = ({
  isOpen,
  onClose,
  onDownload,
}) => {
  const [url, setUrl] = useState("");
  const [probeUrl, setProbeUrl] = useState("");
  const [scanning, setScanning] = useState(false);
  const [probing, setProbing] = useState(false);
  const [streams, setStreams] = useState<DetectedStream[]>([]);
  const [probedStream, setProbedStream] = useState<DetectedStream | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

  const handleScan = async () => {
    if (!url.trim()) return;
    setScanning(true);
    setError(null);
    setStreams([]);
    try {
      const results = await invoke<DetectedStream[]>("scan_page_for_streams", {
        url: url.trim(),
      });
      setStreams(results);
      if (results.length === 0) {
        setError("No streams detected on this page");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const handleProbe = async () => {
    if (!probeUrl.trim()) return;
    setProbing(true);
    setError(null);
    setProbedStream(null);
    try {
      const result = await invoke<DetectedStream | null>("probe_video_url", {
        url: probeUrl.trim(),
      });
      if (result) {
        setProbedStream(result);
      } else {
        setError("No video stream detected at this URL");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setProbing(false);
    }
  };

  const handleCopy = (streamUrl: string) => {
    navigator.clipboard.writeText(streamUrl);
    setCopiedUrl(streamUrl);
    setTimeout(() => setCopiedUrl(null), 2000);
  };

  const handleDownload = (stream: DetectedStream) => {
    if (onDownload) {
      const filename = stream.title
        ? `${stream.title}.${stream.stream_type === "DirectAudio" ? "mp3" : "mp4"}`
        : undefined;
      onDownload(stream.url, filename);
    }
  };

  if (!isOpen) return null;

  return (
    <AnimatePresence>
      <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 backdrop-blur-md p-4">
        <motion.div
          className="relative w-full max-w-3xl max-h-[85vh] bg-[#1a1c23] border border-white/5 shadow-2xl rounded-2xl flex flex-col overflow-hidden"
          initial={{ scale: 0.95, opacity: 0, y: 20 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.95, opacity: 0, y: 20 }}
        >
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-white/5">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-cyan-500/10 rounded-lg">
                <Video size={22} className="text-cyan-400" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-slate-200">
                  Stream Detector
                </h2>
                <p className="text-xs text-slate-500">
                  Scan pages for video/audio streams (HLS, DASH, direct)
                </p>
              </div>
            </div>
            <button
              onClick={onClose}
              className="p-2 hover:bg-white/10 rounded-lg transition-colors text-slate-400 hover:text-white"
            >
              <X size={20} />
            </button>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-6">
            {/* Page Scanner */}
            <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
              <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
                <Search size={16} className="text-cyan-400" />
                Scan Page for Streams
              </h3>
              <div className="flex gap-2">
                <input
                  type="url"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://example.com/page-with-video"
                  onKeyDown={(e) => e.key === "Enter" && handleScan()}
                  className="flex-1 px-4 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-cyan-500/50"
                />
                <button
                  onClick={handleScan}
                  disabled={scanning || !url.trim()}
                  className="px-5 py-2.5 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium flex items-center gap-2 transition-colors"
                >
                  {scanning ? (
                    <Loader2 size={16} className="animate-spin" />
                  ) : (
                    <Search size={16} />
                  )}
                  Scan
                </button>
              </div>
            </div>

            {/* URL Probe */}
            <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
              <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
                <Play size={16} className="text-green-400" />
                Probe Direct URL
              </h3>
              <div className="flex gap-2">
                <input
                  type="url"
                  value={probeUrl}
                  onChange={(e) => setProbeUrl(e.target.value)}
                  placeholder="https://example.com/video.m3u8"
                  onKeyDown={(e) => e.key === "Enter" && handleProbe()}
                  className="flex-1 px-4 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-cyan-500/50"
                />
                <button
                  onClick={handleProbe}
                  disabled={probing || !probeUrl.trim()}
                  className="px-5 py-2.5 bg-green-600 hover:bg-green-500 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium flex items-center gap-2 transition-colors"
                >
                  {probing ? (
                    <Loader2 size={16} className="animate-spin" />
                  ) : (
                    <Play size={16} />
                  )}
                  Probe
                </button>
              </div>
            </div>

            {/* Error */}
            {error && (
              <div className="flex items-center gap-2 px-4 py-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
                <AlertCircle size={16} />
                {error}
              </div>
            )}

            {/* Probed Stream Result */}
            {probedStream && (
              <div className="bg-white/[0.02] border border-white/5 rounded-xl p-5">
                <h3 className="text-sm font-semibold text-slate-300 mb-3">
                  Probe Result
                </h3>
                <StreamCard
                  stream={probedStream}
                  onCopy={handleCopy}
                  onDownload={handleDownload}
                  copiedUrl={copiedUrl}
                />
              </div>
            )}

            {/* Detected Streams */}
            {streams.length > 0 && (
              <div>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
                    <Radio size={16} className="text-purple-400" />
                    Detected Streams ({streams.length})
                  </h3>
                  <div className="flex gap-2 text-xs text-slate-500">
                    {["Hls", "Dash", "DirectVideo", "DirectAudio"].map((t) => {
                      const count = streams.filter((s) => s.stream_type === t).length;
                      if (count === 0) return null;
                      return (
                        <span key={t} className={`px-2 py-0.5 rounded ${getStreamColor(t)}`}>
                          {t}: {count}
                        </span>
                      );
                    })}
                  </div>
                </div>
                <div className="space-y-2">
                  {streams.map((stream, i) => (
                    <StreamCard
                      key={i}
                      stream={stream}
                      onCopy={handleCopy}
                      onDownload={handleDownload}
                      copiedUrl={copiedUrl}
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        </motion.div>
      </div>
    </AnimatePresence>
  );
};

// ─── Stream Card ─────────────────────────────────────────────────────────────

const StreamCard: React.FC<{
  stream: DetectedStream;
  onCopy: (url: string) => void;
  onDownload: (stream: DetectedStream) => void;
  copiedUrl: string | null;
}> = ({ stream, onCopy, onDownload, copiedUrl }) => (
  <div className="bg-white/[0.02] border border-white/5 rounded-lg p-4">
    <div className="flex items-start gap-3">
      <div className="mt-0.5">{getStreamIcon(stream.stream_type)}</div>
      <div className="flex-1 min-w-0">
        {stream.title && (
          <div className="text-sm font-medium text-slate-200 mb-1">
            {stream.title}
          </div>
        )}
        <div className="text-xs text-slate-500 font-mono truncate mb-2">
          {stream.url}
        </div>
        <div className="flex items-center gap-3 text-xs">
          <span className={`px-2 py-0.5 rounded border ${getStreamColor(stream.stream_type)}`}>
            {stream.stream_type}
          </span>
          {stream.quality && (
            <span className="text-yellow-400">{stream.quality}</span>
          )}
          {stream.content_type && (
            <span className="text-slate-500">{stream.content_type}</span>
          )}
          {stream.estimated_size && (
            <span className="text-slate-400">
              ~{formatBytes(stream.estimated_size)}
            </span>
          )}
        </div>
      </div>
      <div className="flex items-center gap-1.5">
        <button
          onClick={() => onCopy(stream.url)}
          className="p-2 hover:bg-white/5 rounded-lg transition-colors text-slate-500 hover:text-slate-200"
          title="Copy URL"
        >
          {copiedUrl === stream.url ? (
            <Check size={14} className="text-green-400" />
          ) : (
            <Copy size={14} />
          )}
        </button>
        <button
          onClick={() => onDownload(stream)}
          className="p-2 bg-cyan-600/20 hover:bg-cyan-600/30 text-cyan-400 rounded-lg transition-colors"
          title="Download"
        >
          <Download size={14} />
        </button>
      </div>
    </div>
  </div>
);
