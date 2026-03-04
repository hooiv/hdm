import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, Activity } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { motion } from "framer-motion";
import { useToast } from "../../contexts/ToastContext";

interface GeneralTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
}

export const GeneralTab: React.FC<GeneralTabProps> = ({
  settings,
  setSettings,
}) => {
  const toast = useToast();

  const handleSelectDir = async () => {
    try {
      const selected = await invoke<string>("select_directory");
      if (selected) {
        setSettings({ ...settings, download_dir: selected });
      }
    } catch (e) {
      console.error("Failed to select directory", e);
      toast.error("Failed to select directory");
    }
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      {/* Storage & Downloads */}
      <div className="space-y-4">
        <SectionHeader icon={Folder} title="Storage & Downloads" />

        <div className="grid gap-6 md:grid-cols-2">
          <div className="space-y-2 md:col-span-2">
            <label className="text-sm font-medium text-slate-400">
              Default Download Path
            </label>
            <div className="flex gap-3">
              <input
                type="text"
                value={settings.download_dir}
                readOnly
                className="flex-1 bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
              />
              <button
                onClick={handleSelectDir}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors font-medium text-sm whitespace-nowrap"
              >
                Change Folder
              </button>
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium text-slate-400">
              Concurrent Segments
            </label>
            <input
              type="number"
              min="1"
              max="32"
              value={settings.segments}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  segments: parseInt(e.target.value) || 1,
                })
              }
              className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-blue-500/50"
            />
          </div>

          <div className="space-y-2">
            <div className="flex justify-between items-center mb-1">
              <label className="text-sm font-medium text-slate-400">
                Speed Limit (KB/s)
              </label>
              <Toggle
                checked={settings.speed_limit_kbps > 0}
                onChange={(val) =>
                  setSettings({ ...settings, speed_limit_kbps: val ? 1024 : 0 })
                }
              />
            </div>
            <input
              type="number"
              disabled={settings.speed_limit_kbps === 0}
              value={settings.speed_limit_kbps}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  speed_limit_kbps: parseInt(e.target.value) || 0,
                })
              }
              className={`w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-blue-500/50 transition-opacity ${settings.speed_limit_kbps === 0 ? "opacity-50" : ""}`}
            />
          </div>
        </div>
      </div>

      {/* Archive Extraction */}
      <div className="space-y-4">
        <SectionHeader icon={Activity} title="Archive Extraction" />
        <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
          <Toggle
            label="Auto-Extract Archives"
            checked={settings.auto_extract_archives || false}
            onChange={(val) =>
              setSettings({ ...settings, auto_extract_archives: val })
            }
          />
          <p className="text-xs text-slate-500 leading-relaxed">
            Automatically extract RAR/ZIP archives when downloads complete.
            Requires unrar or WinRAR installed on your system.
          </p>

          {settings.auto_extract_archives && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="space-y-3 pt-2 border-t border-slate-700/30"
            >
              <Toggle
                label="Cleanup After Extraction"
                checked={settings.cleanup_archives_after_extract || false}
                onChange={(val) =>
                  setSettings({
                    ...settings,
                    cleanup_archives_after_extract: val,
                  })
                }
              />
              <p className="text-xs text-slate-500 leading-relaxed">
                ⚠️ <strong className="text-amber-400">Warning:</strong> Archives
                will be permanently deleted after successful extraction.
              </p>
            </motion.div>
          )}
        </div>
      </div>

      {/* Power Management */}
      <div className="space-y-4">
        <SectionHeader icon={Activity} title="Power Management" />
        <div className="bg-slate-800/20 rounded-xl p-5 border border-slate-700/30 space-y-4">
          <div className="space-y-1">
            <Toggle
              label="Prevent Sleep During Download"
              checked={settings.prevent_sleep_during_download !== false}
              onChange={(v) =>
                setSettings({
                  ...settings,
                  prevent_sleep_during_download: v,
                })
              }
            />
            <p className="text-xs text-slate-500 ml-1 mb-2">
              Keeps the system awake while downloads are active.
            </p>
          </div>

          <div className="space-y-1 pt-4 border-t border-slate-700/30">
            <Toggle
              label="Pause on Low Battery (15%)"
              checked={settings.pause_on_low_battery !== false}
              onChange={(v) =>
                setSettings({ ...settings, pause_on_low_battery: v })
              }
            />
            <p className="text-xs text-slate-500 ml-1">
              Automatically pauses active downloads if battery drops below 15%.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};
