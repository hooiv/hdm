import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import {
  X,
  Wifi,
  Fingerprint,
  Loader2,
  CheckCircle,
  XCircle,
  AlertTriangle,
} from "lucide-react";

interface NetworkDiagnosticsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface DiagResult {
  status: "idle" | "running" | "pass" | "fail" | "warn";
  message: string;
}

const BROWSER_PROFILES = ["chrome", "firefox", "safari"] as const;

export const NetworkDiagnosticsModal: React.FC<NetworkDiagnosticsModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [connectivity, setConnectivity] = useState<DiagResult>({ status: "idle", message: "" });
  const [captivePortal, setCaptivePortal] = useState<DiagResult>({ status: "idle", message: "" });
  const [fingerprint, setFingerprint] = useState<DiagResult>({ status: "idle", message: "" });
  const [fingerprintData, setFingerprintData] = useState("");
  const [ja3Url, setJa3Url] = useState("https://tls.peet.ws/api/all");
  const [ja3Browser, setJa3Browser] = useState<string>("chrome");
  const [ja3Result, setJa3Result] = useState<DiagResult>({ status: "idle", message: "" });
  const [ja3Data, setJa3Data] = useState("");
  const [isRunningAll, setIsRunningAll] = useState(false);

  const runConnectivity = async () => {
    setConnectivity({ status: "running", message: "Checking internet access..." });
    try {
      const ok = await invoke<boolean>("check_network_status");
      setConnectivity({
        status: ok ? "pass" : "fail",
        message: ok ? "Internet connected — HTTP connectivity confirmed" : "No internet access detected",
      });
      return ok;
    } catch (e) {
      setConnectivity({ status: "fail", message: `Error: ${e}` });
      return false;
    }
  };

  const runCaptivePortal = async () => {
    setCaptivePortal({ status: "running", message: "Checking for captive portal..." });
    try {
      // Send a small probe — if the first bytes look like HTML redirect, it's captive
      const isCaptive = await invoke<boolean>("check_captive_portal", {
        firstBytes: Array.from(new TextEncoder().encode("<html")),
      });
      setCaptivePortal({
        status: isCaptive ? "warn" : "pass",
        message: isCaptive
          ? "Captive portal detected — sign in to your network"
          : "No captive portal — network is clean",
      });
    } catch (e) {
      setCaptivePortal({ status: "fail", message: `Error: ${e}` });
    }
  };

  const runFingerprint = async () => {
    setFingerprint({ status: "running", message: "Testing browser impersonation..." });
    setFingerprintData("");
    try {
      const result = await invoke<string>("test_browser_fingerprint");
      setFingerprintData(result);
      const looksGood = result.toLowerCase().includes("user-agent");
      setFingerprint({
        status: looksGood ? "pass" : "warn",
        message: looksGood
          ? "Browser impersonation successful — headers look authentic"
          : "Fingerprint test returned data (review below)",
      });
    } catch (e) {
      setFingerprint({ status: "fail", message: `Error: ${e}` });
    }
  };

  const runJa3 = async () => {
    if (!ja3Url.trim()) return;
    setJa3Result({ status: "running", message: `Fetching with ${ja3Browser} TLS profile...` });
    setJa3Data("");
    try {
      const result = await invoke<string>("fetch_with_ja3", {
        url: ja3Url.trim(),
        browser: ja3Browser,
      });
      setJa3Data(result);
      setJa3Result({ status: "pass", message: `Fetched successfully with ${ja3Browser} fingerprint` });
    } catch (e) {
      setJa3Result({ status: "fail", message: `Error: ${e}` });
    }
  };

  const runAll = async () => {
    setIsRunningAll(true);
    await runConnectivity();
    await runCaptivePortal();
    await runFingerprint();
    setIsRunningAll(false);
  };

  const StatusIcon: React.FC<{ status: DiagResult["status"] }> = ({ status }) => {
    switch (status) {
      case "running":
        return <Loader2 size={18} className="animate-spin text-cyan-400" />;
      case "pass":
        return <CheckCircle size={18} className="text-emerald-400" />;
      case "fail":
        return <XCircle size={18} className="text-red-400" />;
      case "warn":
        return <AlertTriangle size={18} className="text-amber-400" />;
      default:
        return <div className="w-[18px] h-[18px] rounded-full border-2 border-slate-600" />;
    }
  };

  const statusBorder = (status: DiagResult["status"]) => {
    switch (status) {
      case "pass": return "border-emerald-500/30";
      case "fail": return "border-red-500/30";
      case "warn": return "border-amber-500/30";
      case "running": return "border-cyan-500/30";
      default: return "border-slate-700/30";
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
                <div className="w-10 h-10 rounded-lg bg-cyan-500/10 flex items-center justify-center border border-cyan-500/20">
                  <Wifi size={20} className="text-cyan-400" />
                </div>
                <div>
                  <h2 className="text-lg font-bold text-white">Network Diagnostics</h2>
                  <p className="text-xs text-slate-500">
                    Test connectivity, captive portals, and TLS fingerprinting
                  </p>
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
            <div className="flex-1 overflow-y-auto p-6 space-y-4 custom-scrollbar">
              {/* Run All */}
              <button
                onClick={runAll}
                disabled={isRunningAll}
                className="w-full py-3 rounded-xl bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors font-medium flex items-center justify-center gap-2 disabled:opacity-50"
              >
                {isRunningAll ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Running diagnostics...
                  </>
                ) : (
                  "Run All Diagnostics"
                )}
              </button>

              {/* Connectivity */}
              <div className={`p-4 rounded-xl border bg-slate-800/30 ${statusBorder(connectivity.status)} space-y-2`}>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <StatusIcon status={connectivity.status} />
                    <div>
                      <h3 className="text-sm font-semibold text-slate-200">Internet Connectivity</h3>
                      <p className="text-xs text-slate-500">HEAD request to gstatic.com</p>
                    </div>
                  </div>
                  <button
                    onClick={runConnectivity}
                    disabled={connectivity.status === "running"}
                    className="px-3 py-1.5 text-xs rounded-lg bg-white/5 text-slate-400 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-50"
                  >
                    Test
                  </button>
                </div>
                {connectivity.message && (
                  <p className={`text-xs pl-[30px] ${connectivity.status === "pass" ? "text-emerald-400" : connectivity.status === "fail" ? "text-red-400" : "text-slate-400"}`}>
                    {connectivity.message}
                  </p>
                )}
              </div>

              {/* Captive Portal */}
              <div className={`p-4 rounded-xl border bg-slate-800/30 ${statusBorder(captivePortal.status)} space-y-2`}>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <StatusIcon status={captivePortal.status} />
                    <div>
                      <h3 className="text-sm font-semibold text-slate-200">Captive Portal Detection</h3>
                      <p className="text-xs text-slate-500">Check if network requires sign-in</p>
                    </div>
                  </div>
                  <button
                    onClick={runCaptivePortal}
                    disabled={captivePortal.status === "running"}
                    className="px-3 py-1.5 text-xs rounded-lg bg-white/5 text-slate-400 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-50"
                  >
                    Test
                  </button>
                </div>
                {captivePortal.message && (
                  <p className={`text-xs pl-[30px] ${captivePortal.status === "pass" ? "text-emerald-400" : captivePortal.status === "warn" ? "text-amber-400" : captivePortal.status === "fail" ? "text-red-400" : "text-slate-400"}`}>
                    {captivePortal.message}
                  </p>
                )}
              </div>

              {/* Browser Fingerprint */}
              <div className={`p-4 rounded-xl border bg-slate-800/30 ${statusBorder(fingerprint.status)} space-y-2`}>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <StatusIcon status={fingerprint.status} />
                    <div>
                      <h3 className="text-sm font-semibold text-slate-200">Browser Impersonation</h3>
                      <p className="text-xs text-slate-500">Test Chrome fingerprint via httpbin.org/headers</p>
                    </div>
                  </div>
                  <button
                    onClick={runFingerprint}
                    disabled={fingerprint.status === "running"}
                    className="px-3 py-1.5 text-xs rounded-lg bg-white/5 text-slate-400 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-50"
                  >
                    Test
                  </button>
                </div>
                {fingerprint.message && (
                  <p className={`text-xs pl-[30px] ${fingerprint.status === "pass" ? "text-emerald-400" : fingerprint.status === "warn" ? "text-amber-400" : "text-slate-400"}`}>
                    {fingerprint.message}
                  </p>
                )}
                {fingerprintData && (
                  <pre className="mt-2 text-xs bg-slate-900/80 border border-slate-700/50 rounded-lg p-3 overflow-x-auto text-slate-300 max-h-40 overflow-y-auto custom-scrollbar">
                    {fingerprintData}
                  </pre>
                )}
              </div>

              {/* JA3 TLS Fingerprint */}
              <div className={`p-4 rounded-xl border bg-slate-800/30 ${statusBorder(ja3Result.status)} space-y-3`}>
                <div className="flex items-center gap-3">
                  <StatusIcon status={ja3Result.status} />
                  <div>
                    <h3 className="text-sm font-semibold text-slate-200">JA3 TLS Fingerprint Fetch</h3>
                    <p className="text-xs text-slate-500">Fetch any URL using a browser TLS profile</p>
                  </div>
                </div>

                <div className="space-y-2">
                  <div className="flex gap-2">
                    <input
                      type="text"
                      className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                      placeholder="https://example.com"
                      value={ja3Url}
                      onChange={(e) => setJa3Url(e.target.value)}
                    />
                    <select
                      value={ja3Browser}
                      onChange={(e) => setJa3Browser(e.target.value)}
                      className="bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none"
                    >
                      {BROWSER_PROFILES.map((b) => (
                        <option key={b} value={b}>
                          {b.charAt(0).toUpperCase() + b.slice(1)}
                        </option>
                      ))}
                    </select>
                    <button
                      onClick={runJa3}
                      disabled={ja3Result.status === "running"}
                      className="px-4 py-2 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center gap-2"
                    >
                      {ja3Result.status === "running" ? <Loader2 size={14} className="animate-spin" /> : <Fingerprint size={14} />}
                      Fetch
                    </button>
                  </div>
                </div>

                {ja3Result.message && (
                  <p className={`text-xs ${ja3Result.status === "pass" ? "text-emerald-400" : ja3Result.status === "fail" ? "text-red-400" : "text-slate-400"}`}>
                    {ja3Result.message}
                  </p>
                )}
                {ja3Data && (
                  <pre className="text-xs bg-slate-900/80 border border-slate-700/50 rounded-lg p-3 overflow-x-auto text-slate-300 max-h-48 overflow-y-auto custom-scrollbar">
                    {ja3Data.length > 4000 ? ja3Data.slice(0, 4000) + "\n...(truncated)" : ja3Data}
                  </pre>
                )}
              </div>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
