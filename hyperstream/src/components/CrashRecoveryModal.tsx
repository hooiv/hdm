import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import {
  ShieldAlert,
  RefreshCw,
  Play,
  PlayCircle,
  X,
  FileWarning,
  HardDrive,
  Clock,
  AlertTriangle,
  CheckCircle,
  Loader2,
} from "lucide-react";

// ─── Types ───────────────────────────────────────────────────────────────────

interface RecoveredDownload {
  id: string;
  filename: string;
  url: string;
  path: string;
  downloaded_bytes: number;
  total_size: number;
  has_segments: boolean;
  file_exists: boolean;
  file_size_on_disk: number;
  last_active: string | null;
}

interface CorruptedDownload {
  id: string;
  filename: string;
  reason: string;
}

interface RecoveryReport {
  recovered: RecoveredDownload[];
  corrupted: CorruptedDownload[];
}

interface SavedDownload {
  id: string;
  url: string;
  path: string;
  filename: string;
  total_size: number;
  downloaded_bytes: number;
  status: string;
  last_active: string | null;
  error_message: string | null;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatRelativeTime(isoStr: string | null): string {
  if (!isoStr) return "Unknown";
  try {
    const d = new Date(isoStr);
    const diff = Date.now() - d.getTime();
    if (diff < 60000) return "Just now";
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return `${Math.floor(diff / 86400000)}d ago`;
  } catch {
    return "Unknown";
  }
}

// ─── Component ───────────────────────────────────────────────────────────────

interface CrashRecoveryModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const CrashRecoveryModal: React.FC<CrashRecoveryModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [scanning, setScanning] = useState(false);
  const [report, setReport] = useState<RecoveryReport | null>(null);
  const [interrupted, setInterrupted] = useState<SavedDownload[]>([]);
  const [resumingIds, setResumingIds] = useState<Set<string>>(new Set());
  const [resumingAll, setResumingAll] = useState(false);
  const [resumedCount, setResumedCount] = useState(0);

  const loadData = async () => {
    setScanning(true);
    try {
      const [rep, inter] = await Promise.all([
        invoke<RecoveryReport>("scan_crashed_downloads"),
        invoke<SavedDownload[]>("get_interrupted_downloads"),
      ]);
      setReport(rep);
      setInterrupted(inter);
    } catch {
      // partial failures ok
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      loadData();
    }
  }, [isOpen]);

  const handleResumeOne = async (id: string) => {
    setResumingIds((prev) => new Set(prev).add(id));
    try {
      await invoke("resume_interrupted_download", { id });
      setResumedCount((c) => c + 1);
      // Remove from interrupted list
      setInterrupted((prev) => prev.filter((d) => d.id !== id));
      if (report) {
        setReport({
          ...report,
          recovered: report.recovered.filter((d) => d.id !== id),
        });
      }
    } catch {
      // keep in list if failed
    } finally {
      setResumingIds((prev) => {
        const n = new Set(prev);
        n.delete(id);
        return n;
      });
    }
  };

  const handleResumeAll = async () => {
    setResumingAll(true);
    try {
      const count = await invoke<number>("resume_all_interrupted");
      setResumedCount(count);
      setInterrupted([]);
      if (report) {
        setReport({ recovered: [], corrupted: report.corrupted });
      }
    } catch {
      // partial ok
    } finally {
      setResumingAll(false);
    }
  };

  if (!isOpen) return null;

  const totalRecoverable =
    (report?.recovered.length ?? 0) + interrupted.length;

  // Deduplicate: merge recovered + interrupted by id
  const allRecoverable = new Map<string, RecoveredDownload | SavedDownload>();
  report?.recovered.forEach((r) => allRecoverable.set(r.id, r));
  interrupted.forEach((i) => {
    if (!allRecoverable.has(i.id)) allRecoverable.set(i.id, i);
  });

  return (
    <AnimatePresence>
      <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 backdrop-blur-md p-4">
        <motion.div
          className="relative w-full max-w-3xl max-h-[80vh] bg-[#1a1c23] border border-white/5 shadow-2xl rounded-2xl flex flex-col overflow-hidden"
          initial={{ scale: 0.95, opacity: 0, y: 20 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.95, opacity: 0, y: 20 }}
        >
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-white/5">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-orange-500/10 rounded-lg">
                <ShieldAlert size={22} className="text-orange-400" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-slate-200">
                  Crash Recovery
                </h2>
                <p className="text-xs text-slate-500">
                  Scan for interrupted or crashed downloads and resume them
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
            {/* Action Bar */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-4">
                <button
                  onClick={loadData}
                  disabled={scanning}
                  className="px-4 py-2 bg-orange-600/20 hover:bg-orange-600/30 text-orange-400 rounded-lg text-sm font-medium flex items-center gap-2 transition-colors disabled:opacity-40"
                >
                  {scanning ? (
                    <Loader2 size={16} className="animate-spin" />
                  ) : (
                    <RefreshCw size={16} />
                  )}
                  Scan
                </button>
                {totalRecoverable > 0 && (
                  <button
                    onClick={handleResumeAll}
                    disabled={resumingAll}
                    className="px-4 py-2 bg-green-600/20 hover:bg-green-600/30 text-green-400 rounded-lg text-sm font-medium flex items-center gap-2 transition-colors disabled:opacity-40"
                  >
                    {resumingAll ? (
                      <Loader2 size={16} className="animate-spin" />
                    ) : (
                      <PlayCircle size={16} />
                    )}
                    Resume All ({allRecoverable.size})
                  </button>
                )}
              </div>
              {resumedCount > 0 && (
                <div className="flex items-center gap-2 text-green-400 text-sm">
                  <CheckCircle size={16} />
                  {resumedCount} resumed
                </div>
              )}
            </div>

            {/* Summary Cards */}
            {report && (
              <div className="grid grid-cols-3 gap-3">
                <div className="bg-white/[0.02] border border-white/5 rounded-xl p-4 text-center">
                  <div className="text-2xl font-bold text-orange-400">
                    {report.recovered.length}
                  </div>
                  <div className="text-xs text-slate-500">Recoverable</div>
                </div>
                <div className="bg-white/[0.02] border border-white/5 rounded-xl p-4 text-center">
                  <div className="text-2xl font-bold text-red-400">
                    {report.corrupted.length}
                  </div>
                  <div className="text-xs text-slate-500">Corrupted</div>
                </div>
                <div className="bg-white/[0.02] border border-white/5 rounded-xl p-4 text-center">
                  <div className="text-2xl font-bold text-blue-400">
                    {interrupted.length}
                  </div>
                  <div className="text-xs text-slate-500">Interrupted</div>
                </div>
              </div>
            )}

            {/* Recoverable Downloads */}
            {allRecoverable.size > 0 && (
              <div>
                <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
                  <HardDrive size={16} className="text-orange-400" />
                  Recoverable Downloads
                </h3>
                <div className="space-y-2">
                  {Array.from(allRecoverable.values()).map((item) => {
                    const isRecovered = "has_segments" in item;
                    const dl = item as RecoveredDownload & SavedDownload;
                    const pct =
                      dl.total_size > 0
                        ? (dl.downloaded_bytes / dl.total_size) * 100
                        : 0;
                    const isResuming = resumingIds.has(dl.id);

                    return (
                      <div
                        key={dl.id}
                        className="bg-white/[0.02] border border-white/5 rounded-lg p-4"
                      >
                        <div className="flex items-center gap-3">
                          {/* File info */}
                          <div className="flex-1 min-w-0">
                            <div className="text-sm font-medium text-slate-200 truncate">
                              {dl.filename}
                            </div>
                            <div className="flex items-center gap-3 text-xs text-slate-500 mt-1">
                              <span>
                                {formatBytes(dl.downloaded_bytes)} /{" "}
                                {formatBytes(dl.total_size)}
                              </span>
                              {isRecovered && (dl as RecoveredDownload).has_segments && (
                                <span className="text-cyan-500">Segmented</span>
                              )}
                              {isRecovered && (dl as RecoveredDownload).file_exists && (
                                <span className="text-green-500">
                                  File on disk: {formatBytes((dl as RecoveredDownload).file_size_on_disk)}
                                </span>
                              )}
                              {!isRecovered && dl.error_message && (
                                <span className="text-red-400 truncate max-w-[200px]">
                                  {dl.error_message}
                                </span>
                              )}
                              <span className="flex items-center gap-1">
                                <Clock size={10} />
                                {formatRelativeTime(dl.last_active)}
                              </span>
                            </div>
                          </div>

                          {/* Progress + Resume */}
                          <div className="flex items-center gap-3">
                            <div className="w-20 text-right">
                              <div className="text-xs font-bold text-slate-300">
                                {pct.toFixed(1)}%
                              </div>
                              <div className="w-full bg-slate-800 rounded-full h-1.5 mt-1">
                                <div
                                  className="h-full rounded-full bg-gradient-to-r from-orange-500 to-yellow-500"
                                  style={{ width: `${Math.min(pct, 100)}%` }}
                                />
                              </div>
                            </div>
                            <button
                              onClick={() => handleResumeOne(dl.id)}
                              disabled={isResuming}
                              className="p-2 bg-green-600/20 hover:bg-green-600/30 text-green-400 rounded-lg transition-colors disabled:opacity-40"
                            >
                              {isResuming ? (
                                <Loader2 size={16} className="animate-spin" />
                              ) : (
                                <Play size={16} />
                              )}
                            </button>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Corrupted Downloads */}
            {report && report.corrupted.length > 0 && (
              <div>
                <h3 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
                  <AlertTriangle size={16} className="text-red-400" />
                  Corrupted (Cannot Recover)
                </h3>
                <div className="space-y-2">
                  {report.corrupted.map((item) => (
                    <div
                      key={item.id}
                      className="bg-red-500/5 border border-red-500/10 rounded-lg px-4 py-3 flex items-center gap-3"
                    >
                      <FileWarning size={16} className="text-red-400 shrink-0" />
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-slate-300 truncate">
                          {item.filename}
                        </div>
                        <div className="text-xs text-red-400/70">{item.reason}</div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Empty State */}
            {!scanning && report && allRecoverable.size === 0 && report.corrupted.length === 0 && (
              <div className="text-center py-12">
                <CheckCircle size={40} className="mx-auto text-green-500/30 mb-3" />
                <div className="text-slate-400 text-sm font-medium">
                  No crashed or interrupted downloads found
                </div>
                <div className="text-slate-600 text-xs mt-1">
                  Everything looks clean!
                </div>
              </div>
            )}
          </div>
        </motion.div>
      </div>
    </AnimatePresence>
  );
};
