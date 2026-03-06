import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import {
  X,
  Globe2,
  Download,
  Loader2,
  CheckCircle,
  Link2,
  HardDrive,
} from "lucide-react";

interface IpfsDownloadModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface IpfsResult {
  status: string;
  cid: string;
  gateway_url: string;
  save_path: string;
  file_size: number;
  content_type: string;
}

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
};

export const IpfsDownloadModal: React.FC<IpfsDownloadModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [uri, setUri] = useState("");
  const [savePath, setSavePath] = useState("");
  const [parsedCid, setParsedCid] = useState<string | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);
  const [result, setResult] = useState<IpfsResult | null>(null);
  const [error, setError] = useState("");

  const handleParse = async () => {
    if (!uri.trim()) return;
    setError("");
    setParsedCid(null);
    try {
      const cid = await invoke<string | null>("parse_ipfs_uri_cmd", { input: uri.trim() });
      if (cid) {
        setParsedCid(cid);
      } else {
        setError("Could not parse IPFS URI. Supported formats: ipfs://CID, /ipfs/CID, or bare CID (Qm... or ba...)");
      }
    } catch (e) {
      setError(`Parse error: ${e}`);
    }
  };

  const handleDownload = async () => {
    const cid = parsedCid || uri.trim();
    if (!cid || !savePath.trim()) return;
    setIsDownloading(true);
    setResult(null);
    setError("");
    try {
      const data = await invoke<IpfsResult>("download_ipfs", {
        cid,
        savePath: savePath.trim(),
      });
      setResult(data);
    } catch (e) {
      setError(`Download failed: ${e}`);
    } finally {
      setIsDownloading(false);
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
        >
          <motion.div
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-lg bg-slate-900 border border-slate-700/50 rounded-2xl shadow-2xl overflow-hidden max-h-[80vh] flex flex-col"
          >
            {/* Header */}
            <div className="px-6 py-4 border-b border-slate-700/50 flex items-center justify-between bg-slate-900/80">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-teal-500/10 flex items-center justify-center border border-teal-500/20">
                  <Globe2 size={20} className="text-teal-400" />
                </div>
                <div>
                  <h2 className="text-lg font-bold text-white">IPFS Download</h2>
                  <p className="text-xs text-slate-500">Download files from the IPFS network via public gateways</p>
                </div>
              </div>
              <button
                onClick={onClose}
                className="p-2 rounded-lg hover:bg-white/5 text-slate-400 hover:text-white transition-colors"
              >
                <X size={20} />
              </button>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-6 space-y-5 custom-scrollbar">
              {/* URI Input */}
              <div className="space-y-2">
                <label className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
                  <Link2 size={12} /> IPFS URI or CID
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    className="flex-1 bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-teal-500/50 font-mono"
                    placeholder="ipfs://QmExample... or /ipfs/CID or bare CID"
                    value={uri}
                    onChange={(e) => { setUri(e.target.value); setParsedCid(null); }}
                  />
                  <button
                    onClick={handleParse}
                    className="px-3 py-2 rounded-lg bg-white/5 text-slate-400 hover:text-white hover:bg-white/10 transition-colors text-xs font-medium"
                  >
                    Parse
                  </button>
                </div>
                {parsedCid && (
                  <div className="flex items-center gap-2 text-xs text-teal-400 bg-teal-500/10 border border-teal-500/20 rounded-lg px-3 py-2">
                    <CheckCircle size={12} />
                    <span className="font-mono truncate">{parsedCid}</span>
                  </div>
                )}
              </div>

              {/* Save Path */}
              <div className="space-y-2">
                <label className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
                  <HardDrive size={12} /> Save To
                </label>
                <input
                  type="text"
                  className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2.5 text-slate-200 text-sm focus:outline-none focus:border-teal-500/50"
                  placeholder="C:\Downloads\ipfs-file"
                  value={savePath}
                  onChange={(e) => setSavePath(e.target.value)}
                />
              </div>

              {/* Download Button */}
              <button
                onClick={handleDownload}
                disabled={isDownloading || (!parsedCid && !uri.trim()) || !savePath.trim()}
                className="w-full py-3 rounded-xl bg-teal-500/10 text-teal-400 border border-teal-500/20 hover:bg-teal-500/20 transition-colors font-medium flex items-center justify-center gap-2 disabled:opacity-50"
              >
                {isDownloading ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Downloading from IPFS gateways...
                  </>
                ) : (
                  <>
                    <Download size={16} />
                    Download from IPFS
                  </>
                )}
              </button>

              {/* Info */}
              <p className="text-[10px] text-slate-600 leading-relaxed">
                Races {5} public gateways (ipfs.io, Pinata, Cloudflare, dweb.link, w3s.link) and uses the fastest.
                Max file size: 1 GB.
              </p>

              {/* Error */}
              {error && (
                <div className="text-xs text-red-400 bg-red-500/10 border border-red-500/20 rounded-lg px-3 py-2">
                  {error}
                </div>
              )}

              {/* Result */}
              {result && (
                <div className="p-4 rounded-xl border bg-emerald-500/5 border-emerald-500/20 space-y-2">
                  <div className="flex items-center gap-2 text-emerald-400 font-medium text-sm">
                    <CheckCircle size={16} />
                    Download Complete
                  </div>
                  <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                    <span className="text-slate-500">CID</span>
                    <span className="text-slate-300 font-mono truncate">{result.cid}</span>
                    <span className="text-slate-500">Gateway</span>
                    <span className="text-slate-300 truncate">{result.gateway_url}</span>
                    <span className="text-slate-500">Size</span>
                    <span className="text-slate-300">{formatBytes(result.file_size)}</span>
                    <span className="text-slate-500">Type</span>
                    <span className="text-slate-300">{result.content_type || "unknown"}</span>
                    <span className="text-slate-500">Saved To</span>
                    <span className="text-slate-300 truncate">{result.save_path}</span>
                  </div>
                </div>
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
