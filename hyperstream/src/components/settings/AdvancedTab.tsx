import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Activity, ShieldAlert } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { useToast } from "../../contexts/ToastContext";

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
  settings: _settings,
  setSettings: _setSettings,
}) => {
  const [wfpAppPath, setWfpAppPath] = useState("");
  const [isWfpProcessing, setIsWfpProcessing] = useState(false);
  const [chaos, setChaos] = useState<ChaosConfig>({ enabled: false, latency_ms: 0, error_rate: 0 });
  const toast = useToast();

  useEffect(() => {
    invoke<ChaosConfig>("get_chaos_config").then(setChaos).catch((err) => {
      console.error("Failed to load chaos config:", err);
      toast.error("Failed to load chaos config");
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
      const { invoke } = await import("@tauri-apps/api/core");
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

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <SectionHeader icon={Activity} title="Advanced" />
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
    </div>
  );
};
