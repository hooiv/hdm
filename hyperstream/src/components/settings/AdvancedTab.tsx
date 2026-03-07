import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Activity, ShieldAlert, Key, Copy, Check, Upload, Download, Loader2, RefreshCw } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { useToast } from "../../contexts/ToastContext";
import { error as logError } from "../../utils/logger";

interface ChaosConfig {
  enabled: boolean;
  latency_ms: number;
  error_rate: number;
}

interface AdvancedTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
}

export const AdvancedTab: React.FC<AdvancedTabProps> = ({
  settings,
  setSettings,
}) => {
  const [wfpAppPath, setWfpAppPath] = useState("");
  const [isWfpProcessing, setIsWfpProcessing] = useState(false);
  const [chaos, setChaos] = useState<ChaosConfig>({ enabled: false, latency_ms: 0, error_rate: 0 });
  const [authToken, setAuthToken] = useState<string | null>(null);
  const [tokenCopied, setTokenCopied] = useState(false);
  const [exportPath, setExportPath] = useState("");
  const [importPath, setImportPath] = useState("");
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const toast = useToast();
  const toastRef = React.useRef(toast);
  toastRef.current = toast;

  useEffect(() => {
    invoke<ChaosConfig>("get_chaos_config").then(setChaos).catch((err) => {
      logError("Failed to load chaos config:", err);
      toastRef.current.error("Failed to load chaos config");
    });
    invoke<string>("get_auth_token").then(setAuthToken).catch((err) => {
      logError("Failed to load auth token:", err);
      setAuthToken(""); // Clear loading state — token may not be generated yet
    });
  }, []);

  const updateChaos = async (update: Partial<ChaosConfig>) => {
    const newChaos = { ...chaos, ...update };
    setChaos(newChaos);
    try {
      await invoke("set_chaos_config", {
        latencyMs: newChaos.latency_ms,
        errorRate: newChaos.error_rate,
        enabled: newChaos.enabled,
      });
    } catch (e) {
      toast.error("Failed to update chaos config: " + e);
    }
  };

  const handleWfpChange = async (blocked: boolean) => {
    if (!wfpAppPath) {
      toast.error("Please enter an executable path (e.g., C:\\MyGame\\game.exe)");
      return;
    }
    setIsWfpProcessing(true);
    try {
      const result = await invoke<string>("set_app_firewall_rule", {
        exePath: wfpAppPath,
        blocked,
      });
      toast.success(result);
    } catch (e) {
      toast.error("WFP Error: " + e);
    } finally {
      setIsWfpProcessing(false);
    }
  };

  const copyAuthToken = async () => {
    if (authToken) {
      try {
        await navigator.clipboard.writeText(authToken);
        setTokenCopied(true);
        setTimeout(() => setTokenCopied(false), 2000);
        toast.success("Auth token copied to clipboard");
      } catch {
        toast.error("Failed to copy token to clipboard");
      }
    }
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <SectionHeader icon={Activity} title="Advanced" />

      {/* Retry & recovery — production-grade backoff (segment + queue) */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-lg bg-amber-500/10 flex items-center justify-center border border-amber-500/20">
            <RefreshCw size={20} className="text-amber-400" />
          </div>
          <div>
            <h3 className="text-slate-200 font-semibold">Retry &amp; recovery</h3>
            <p className="text-xs text-slate-500">
              Segment retries (per connection) and queue retries (whole download). Exponential backoff with jitter.
            </p>
          </div>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Segment: max immediate retries</label>
            <input
              type="number"
              min={0}
              max={20}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.segment_retry_max_immediate ?? 3}
              onChange={(e) => setSettings({ ...settings, segment_retry_max_immediate: Math.min(20, Math.max(0, parseInt(e.target.value, 10) || 0)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Segment: max delayed retries</label>
            <input
              type="number"
              min={0}
              max={30}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.segment_retry_max_delayed ?? 5}
              onChange={(e) => setSettings({ ...settings, segment_retry_max_delayed: Math.min(30, Math.max(0, parseInt(e.target.value, 10) || 0)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Segment: initial delay (s)</label>
            <input
              type="number"
              min={0}
              max={60}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.segment_retry_initial_delay_secs ?? 1}
              onChange={(e) => setSettings({ ...settings, segment_retry_initial_delay_secs: Math.min(60, Math.max(0, parseInt(e.target.value, 10) || 0)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Segment: max delay (s)</label>
            <input
              type="number"
              min={1}
              max={600}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.segment_retry_max_delay_secs ?? 60}
              onChange={(e) => setSettings({ ...settings, segment_retry_max_delay_secs: Math.min(600, Math.max(1, parseInt(e.target.value, 10) || 60)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Segment: jitter (0–1)</label>
            <input
              type="number"
              min={0}
              max={1}
              step={0.1}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.segment_retry_jitter ?? 0.3}
              onChange={(e) => setSettings({ ...settings, segment_retry_jitter: Math.min(1, Math.max(0, parseFloat(e.target.value) || 0.3)) })}
            />
          </div>
          <div className="space-y-1" />
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Queue: max retries per download</label>
            <input
              type="number"
              min={0}
              max={50}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.queue_retry_max_retries ?? 5}
              onChange={(e) => setSettings({ ...settings, queue_retry_max_retries: Math.min(50, Math.max(0, parseInt(e.target.value, 10) || 0)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Queue: base delay (s)</label>
            <input
              type="number"
              min={0}
              max={300}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.queue_retry_base_delay_secs ?? 5}
              onChange={(e) => setSettings({ ...settings, queue_retry_base_delay_secs: Math.min(300, Math.max(0, parseInt(e.target.value, 10) || 0)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Queue: max delay (s)</label>
            <input
              type="number"
              min={1}
              max={86400}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.queue_retry_max_delay_secs ?? 300}
              onChange={(e) => setSettings({ ...settings, queue_retry_max_delay_secs: Math.min(86400, Math.max(1, parseInt(e.target.value, 10) || 300)) })}
            />
          </div>
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Stall timeout (s)</label>
            <input
              type="number"
              min={30}
              max={86400}
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm"
              value={settings.stall_timeout_secs ?? 120}
              onChange={(e) => setSettings({ ...settings, stall_timeout_secs: Math.min(86400, Math.max(30, parseInt(e.target.value, 10) || 120)) })}
            />
            <p className="text-xs text-slate-500">No progress for this long → mark failed and allow retry.</p>
          </div>
        </div>
      </div>

      {/* Browser Extension Auth Token */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-lg bg-cyan-500/10 flex items-center justify-center border border-cyan-500/20">
            <Key size={20} className="text-cyan-400" />
          </div>
          <div>
            <h3 className="text-slate-200 font-semibold">Browser Extension Token</h3>
            <p className="text-xs text-slate-500">
              Copy this token and paste it into the HyperStream browser extension popup.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-4 py-2.5 font-mono text-sm text-slate-300 truncate">
            {authToken === null ? "Loading..." : authToken ? `${authToken.slice(0, 8)}${"*".repeat(24)}${authToken.slice(-4)}` : "Token unavailable — restart app"}
          </div>
          <button
            onClick={copyAuthToken}
            disabled={!authToken}
            className="px-4 py-2.5 rounded-lg bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center gap-2"
          >
            {tokenCopied ? <Check size={16} /> : <Copy size={16} />}
            {tokenCopied ? "Copied" : "Copy"}
          </button>
        </div>
      </div>

      {/* Native messaging host installer */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-lg bg-violet-500/10 flex items-center justify-center border border-violet-500/20">
            <ShieldAlert size={20} className="text-violet-400" />
          </div>
          <div>
            <h3 className="text-slate-200 font-semibold">Native Messaging Host</h3>
            <p className="text-xs text-slate-500">
              Install a browser native messaging manifest so the extension can auto-launch HyperStream.
            </p>
          </div>
        </div>
        <button
          onClick={async () => {
            try {
              const result = await invoke<string>('install_native_host');
              toastRef.current?.success(result);
            } catch (e) {
              toastRef.current?.error('Installation failed: ' + e);
            }
          }}
          className="px-4 py-2.5 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/20 hover:bg-violet-500/20 transition-colors text-sm font-medium"
        >
          Install Native Host
        </button>
      </div>

      <div
        className={`p-5 rounded-xl border transition-all ${chaos.enabled ? "bg-red-500/10 border-red-500/30" : "bg-slate-800/20 border-slate-700/30"}`}
      >
        <Toggle
          label="Chaos Mode (Experimental)"
          checked={chaos.enabled}
          onChange={(val) => updateChaos({ enabled: val })}
        />
        <p className="text-xs text-slate-500 mt-2 leading-relaxed">
          Enables experimental parallel fetching algorithms. May use significant
          bandwidth and CPU. Use with caution.
        </p>

        {chaos.enabled && (
          <div className="mt-4 grid gap-4 grid-cols-2">
            <div className="space-y-1">
              <label className="text-xs text-red-400">Latency (ms)</label>
              <input
                type="number"
                className="w-full bg-slate-900 border border-red-900/30 rounded px-2 py-1 text-red-200 text-sm"
                value={chaos.latency_ms}
                onChange={(e) =>
                  updateChaos({ latency_ms: parseInt(e.target.value) || 0 })
                }
              />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-red-400">Error Rate (%)</label>
              <input
                type="number"
                className="w-full bg-slate-900 border border-red-900/30 rounded px-2 py-1 text-red-200 text-sm"
                value={chaos.error_rate}
                onChange={(e) =>
                  updateChaos({ error_rate: parseInt(e.target.value) || 0 })
                }
              />
            </div>
          </div>
        )}
      </div>
      {/* WFP Block */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-lg bg-orange-500/10 flex items-center justify-center border border-orange-500/20">
            <ShieldAlert size={20} className="text-orange-400" />
          </div>
          <div>
            <h3 className="text-slate-200 font-semibold">
              Windows Filtering Platform (WFP)
            </h3>
            <p className="text-xs text-slate-500">
              Block external applications from accessing the internet. Requires Administrator privileges.
            </p>
          </div>
        </div>

        <div className="space-y-3">
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Target Executable Path</label>
            <input
              type="text"
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-orange-500/50 focus:ring-1 focus:ring-orange-500/50"
              placeholder="C:\Path\To\Application.exe"
              value={wfpAppPath}
              onChange={(e) => setWfpAppPath(e.target.value)}
            />
          </div>

          <div className="flex gap-3">
            <button
              onClick={() => handleWfpChange(true)}
              disabled={isWfpProcessing}
              className="flex-1 py-2 px-4 rounded-md bg-orange-500/10 text-orange-400 border border-orange-500/20 hover:bg-orange-500/20 transition-colors text-sm font-medium disabled:opacity-50"
            >
              Block Application
            </button>
            <button
              onClick={() => handleWfpChange(false)}
              disabled={isWfpProcessing}
              className="flex-1 py-2 px-4 rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 transition-colors text-sm font-medium disabled:opacity-50"
            >
              Unblock Application
            </button>
          </div>
        </div>
      </div>

      {/* Torrent Queue */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30 space-y-4">
        <div>
          <h3 className="text-slate-200 font-semibold">Torrent Queue Manager</h3>
          <p className="text-xs text-slate-500 mt-1">
            Automatically keep only a limited number of active torrents and queue the rest.
          </p>
        </div>

        <Toggle
          label="Enable auto queue management"
          checked={settings.torrent_auto_manage_queue}
          onChange={(val) =>
            setSettings({ ...settings, torrent_auto_manage_queue: val })
          }
        />

        <div className="space-y-1">
          <label className="text-xs font-medium text-slate-400">
            Max active torrents (0 = unlimited)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={settings.torrent_max_active_downloads}
            onChange={(e) =>
              setSettings({
                ...settings,
                torrent_max_active_downloads: Math.max(
                  0,
                  Math.min(64, parseInt(e.target.value, 10) || 0),
                ),
              })
            }
            className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50"
          />
        </div>
      </div>

      {/* Seeding Policy */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30 space-y-4">
        <div>
          <h3 className="text-slate-200 font-semibold">Torrent Seeding Policy</h3>
          <p className="text-xs text-slate-500 mt-1">
            Automatically stop seeding completed torrents when ratio or max seeding time is reached.
          </p>
        </div>

        <Toggle
          label="Auto stop seeding"
          checked={settings.torrent_auto_stop_seeding}
          onChange={(val) =>
            setSettings({ ...settings, torrent_auto_stop_seeding: val })
          }
        />

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">
              Seed ratio target (0 = disabled)
            </label>
            <input
              type="number"
              min={0}
              max={20}
              step={0.1}
              value={settings.torrent_seed_ratio_limit}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  torrent_seed_ratio_limit: Math.max(
                    0,
                    Math.min(20, parseFloat(e.target.value) || 0),
                  ),
                })
              }
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50"
            />
          </div>

          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">
              Max seeding minutes (0 = unlimited)
            </label>
            <input
              type="number"
              min={0}
              max={10080}
              value={settings.torrent_seed_time_limit_mins}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  torrent_seed_time_limit_mins: Math.max(
                    0,
                    Math.min(10080, parseInt(e.target.value, 10) || 0),
                  ),
                })
              }
              className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50"
            />
          </div>
        </div>
      </div>

      {/* Data Export / Import */}
      <div className="p-5 rounded-xl border bg-slate-800/20 border-slate-700/30 space-y-4">
        <div className="flex items-center gap-3 mb-2">
          <div className="w-10 h-10 rounded-lg bg-emerald-500/10 flex items-center justify-center border border-emerald-500/20">
            <Download size={20} className="text-emerald-400" />
          </div>
          <div>
            <h3 className="text-slate-200 font-semibold">Data Export / Import</h3>
            <p className="text-xs text-slate-500">
              Export your settings &amp; download history to a JSON file, or import from a previous backup.
              Path must be inside Downloads, Documents, or Desktop.
            </p>
          </div>
        </div>

        <div className="space-y-3">
          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Export Path</label>
            <div className="flex gap-2">
              <input
                type="text"
                className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50"
                placeholder="C:\Users\You\Downloads\hyperstream-backup.json"
                value={exportPath}
                onChange={(e) => setExportPath(e.target.value)}
              />
              <button
                onClick={async () => {
                  if (!exportPath.trim()) { toast.error("Enter an export file path"); return; }
                  setIsExporting(true);
                  try {
                    await invoke("export_data", { path: exportPath.trim() });
                    toast.success("Data exported successfully");
                  } catch (e) {
                    toast.error("Export failed: " + e);
                  } finally {
                    setIsExporting(false);
                  }
                }}
                disabled={isExporting}
                className="px-4 py-2 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center gap-2"
              >
                {isExporting ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}
                Export
              </button>
            </div>
          </div>

          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-400">Import Path</label>
            <div className="flex gap-2">
              <input
                type="text"
                className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50"
                placeholder="C:\Users\You\Downloads\hyperstream-backup.json"
                value={importPath}
                onChange={(e) => setImportPath(e.target.value)}
              />
              <button
                onClick={async () => {
                  if (!importPath.trim()) { toast.error("Enter an import file path"); return; }
                  setIsImporting(true);
                  try {
                    await invoke("import_data", { path: importPath.trim() });
                    toast.success("Data imported successfully — restart for full effect");
                  } catch (e) {
                    toast.error("Import failed: " + e);
                  } finally {
                    setIsImporting(false);
                  }
                }}
                disabled={isImporting}
                className="px-4 py-2 rounded-lg bg-amber-500/10 text-amber-400 border border-amber-500/20 hover:bg-amber-500/20 transition-colors text-sm font-medium disabled:opacity-50 flex items-center gap-2"
              >
                {isImporting ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
                Import
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
