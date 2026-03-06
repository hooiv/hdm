import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import {
  X,
  Film,
  Music,
  Lock,
  Loader2,
  CheckCircle,
  Merge,
} from "lucide-react";

interface MediaProcessingModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const MediaProcessingModal: React.FC<MediaProcessingModalProps> = ({
  isOpen,
  onClose,
}) => {
  // Mux state
  const [videoPath, setVideoPath] = useState("");
  const [audioPath, setAudioPath] = useState("");
  const [muxOutputPath, setMuxOutputPath] = useState("");
  const [isMuxing, setIsMuxing] = useState(false);
  const [muxResult, setMuxResult] = useState<{ ok: boolean; msg: string } | null>(null);

  // Decrypt state
  const [decryptInput, setDecryptInput] = useState("");
  const [decryptOutput, setDecryptOutput] = useState("");
  const [keyHex, setKeyHex] = useState("");
  const [ivHex, setIvHex] = useState("");
  const [isDecrypting, setIsDecrypting] = useState(false);
  const [decryptResult, setDecryptResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const handleMux = async () => {
    if (!videoPath.trim() || !audioPath.trim() || !muxOutputPath.trim()) return;
    setIsMuxing(true);
    setMuxResult(null);
    try {
      await invoke("mux_video_audio", {
        videoPath: videoPath.trim(),
        audioPath: audioPath.trim(),
        outputPath: muxOutputPath.trim(),
      });
      setMuxResult({ ok: true, msg: "Video and audio muxed successfully" });
    } catch (e) {
      setMuxResult({ ok: false, msg: `${e}` });
    } finally {
      setIsMuxing(false);
    }
  };

  const handleDecrypt = async () => {
    if (!decryptInput.trim() || !decryptOutput.trim() || !keyHex.trim()) return;
    setIsDecrypting(true);
    setDecryptResult(null);
    try {
      await invoke("decrypt_aes_128", {
        inputPath: decryptInput.trim(),
        outputPath: decryptOutput.trim(),
        keyHex: keyHex.trim(),
        ivHex: ivHex.trim() || "00000000000000000000000000000000",
      });
      setDecryptResult({ ok: true, msg: "File decrypted successfully" });
    } catch (e) {
      setDecryptResult({ ok: false, msg: `${e}` });
    } finally {
      setIsDecrypting(false);
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
            className="w-full max-w-2xl bg-slate-900 border border-slate-700/50 rounded-2xl shadow-2xl overflow-hidden max-h-[85vh] flex flex-col"
          >
            {/* Header */}
            <div className="px-6 py-4 border-b border-slate-700/50 flex items-center justify-between bg-slate-900/80">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-violet-500/10 flex items-center justify-center border border-violet-500/20">
                  <Film size={20} className="text-violet-400" />
                </div>
                <div>
                  <h2 className="text-lg font-bold text-white">Media Processing</h2>
                  <p className="text-xs text-slate-500">Mux video+audio streams or decrypt AES-128 segments</p>
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
            <div className="flex-1 overflow-y-auto p-6 space-y-6 custom-scrollbar">
              {/* Mux Section */}
              <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30 space-y-4">
                <div className="flex items-center gap-3">
                  <Merge size={18} className="text-cyan-400" />
                  <div>
                    <h3 className="text-slate-200 font-semibold">Mux Video + Audio</h3>
                    <p className="text-xs text-slate-500">
                      Combine separate video and audio files into a single output (e.g., DASH/HLS segments)
                    </p>
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3">
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
                      <Film size={12} /> Video File Path
                    </label>
                    <input
                      type="text"
                      className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                      placeholder="C:\Downloads\video.mp4"
                      value={videoPath}
                      onChange={(e) => setVideoPath(e.target.value)}
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
                      <Music size={12} /> Audio File Path
                    </label>
                    <input
                      type="text"
                      className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                      placeholder="C:\Downloads\audio.m4a"
                      value={audioPath}
                      onChange={(e) => setAudioPath(e.target.value)}
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-slate-400">Output Path</label>
                    <input
                      type="text"
                      className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                      placeholder="C:\Downloads\merged.mp4"
                      value={muxOutputPath}
                      onChange={(e) => setMuxOutputPath(e.target.value)}
                    />
                  </div>
                </div>

                <button
                  onClick={handleMux}
                  disabled={isMuxing || !videoPath.trim() || !audioPath.trim() || !muxOutputPath.trim()}
                  className="w-full py-2.5 rounded-lg bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center justify-center gap-2"
                >
                  {isMuxing ? <Loader2 size={14} className="animate-spin" /> : <Merge size={14} />}
                  {isMuxing ? "Muxing..." : "Merge Streams"}
                </button>

                {muxResult && (
                  <div className={`flex items-center gap-2 text-xs px-3 py-2 rounded-lg ${muxResult.ok ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400"}`}>
                    {muxResult.ok ? <CheckCircle size={14} /> : <X size={14} />}
                    {muxResult.msg}
                  </div>
                )}
              </div>

              {/* Decrypt Section */}
              <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30 space-y-4">
                <div className="flex items-center gap-3">
                  <Lock size={18} className="text-amber-400" />
                  <div>
                    <h3 className="text-slate-200 font-semibold">AES-128 Decryption</h3>
                    <p className="text-xs text-slate-500">
                      Decrypt AES-128-CBC encrypted media segments (common in HLS streams)
                    </p>
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3">
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-slate-400">Encrypted File Path</label>
                    <input
                      type="text"
                      className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-amber-500/50"
                      placeholder="C:\Downloads\encrypted.ts"
                      value={decryptInput}
                      onChange={(e) => setDecryptInput(e.target.value)}
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-medium text-slate-400">Output File Path</label>
                    <input
                      type="text"
                      className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-amber-500/50"
                      placeholder="C:\Downloads\decrypted.ts"
                      value={decryptOutput}
                      onChange={(e) => setDecryptOutput(e.target.value)}
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <div className="space-y-1">
                      <label className="text-xs font-medium text-slate-400">Key (hex)</label>
                      <input
                        type="text"
                        className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm font-mono focus:outline-none focus:border-amber-500/50"
                        placeholder="0123456789abcdef0123456789abcdef"
                        value={keyHex}
                        onChange={(e) => setKeyHex(e.target.value)}
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs font-medium text-slate-400">IV (hex, optional)</label>
                      <input
                        type="text"
                        className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm font-mono focus:outline-none focus:border-amber-500/50"
                        placeholder="00000000000000000000000000000000"
                        value={ivHex}
                        onChange={(e) => setIvHex(e.target.value)}
                      />
                    </div>
                  </div>
                </div>

                <button
                  onClick={handleDecrypt}
                  disabled={isDecrypting || !decryptInput.trim() || !decryptOutput.trim() || !keyHex.trim()}
                  className="w-full py-2.5 rounded-lg bg-amber-500/10 text-amber-400 border border-amber-500/20 hover:bg-amber-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center justify-center gap-2"
                >
                  {isDecrypting ? <Loader2 size={14} className="animate-spin" /> : <Lock size={14} />}
                  {isDecrypting ? "Decrypting..." : "Decrypt File"}
                </button>

                {decryptResult && (
                  <div className={`flex items-center gap-2 text-xs px-3 py-2 rounded-lg ${decryptResult.ok ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400"}`}>
                    {decryptResult.ok ? <CheckCircle size={14} /> : <X size={14} />}
                    {decryptResult.msg}
                  </div>
                )}
              </div>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
