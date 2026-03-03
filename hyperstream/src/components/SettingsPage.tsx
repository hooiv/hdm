import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { useToast } from "../contexts/ToastContext";
import {
  Settings,
  X,
  Globe,
  Cloud,
  Save,
  Volume2,
  Activity,
} from "lucide-react";

import { SettingsData } from "./settings/types";
import { GeneralTab } from "./settings/GeneralTab";
import { NetworkTab } from "./settings/NetworkTab";
import { CloudTab } from "./settings/CloudTab";
import { NotificationsTab } from "./settings/NotificationsTab";
import { AdvancedTab } from "./settings/AdvancedTab";

interface SettingsPageProps {
  isOpen: boolean;
  onClose: () => void;
}

type TabId = "general" | "network" | "cloud" | "notifications" | "advanced";

export const SettingsPage: React.FC<SettingsPageProps> = ({
  isOpen,
  onClose,
}) => {
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<TabId>("general");

  const [settings, setSettings] = useState<SettingsData>({
    download_dir: "",
    segments: 8,
    proxy_enabled: false,
    proxy_type: "http",
    proxy_host: "127.0.0.1",
    proxy_port: 8080,
    speed_limit_kbps: 0,
    cloud_enabled: false,
    cloud_endpoint: "",
    cloud_bucket: "",
    cloud_region: "us-east-1",
    cloud_access_key: "",
    cloud_secret_key: "",
    use_tor: false,
    dpi_evasion: false,
    ja3_enabled: false,
    min_threads: 2,
    max_threads: 16,
    clipboard_monitor: false,
    auto_start_extension: true,
    use_category_folders: true,
    last_sync_host: "",
    vpn_auto_connect: false,
    vpn_connection_name: "",
    mqtt_enabled: false,
    mqtt_broker_url: "mqtt://localhost:1883",
    mqtt_topic: "hyperstream/downloads",
    prevent_sleep_during_download: true,
    pause_on_low_battery: true,
    p2p_enabled: false,
    auto_scrub_metadata: false,
  });

  const [saved, setSaved] = useState(false);

  // Audio settings state
  const [audioEnabled, setAudioEnabled] = useState(true);
  const [audioVolume, setAudioVolume] = useState(0.5);

  useEffect(() => {
    if (isOpen) {
      loadSettings();
      setSaved(false);
      setActiveTab("general");
    }
  }, [isOpen]);

  const loadSettings = async () => {
    try {
      const data = await invoke<SettingsData>("get_settings");
      setSettings({
        ...data,
        cloud_endpoint: data.cloud_endpoint || "",
        cloud_bucket: data.cloud_bucket || "",
        cloud_region: data.cloud_region || "us-east-1",
        cloud_access_key: data.cloud_access_key || "",
        cloud_secret_key: data.cloud_secret_key || "",
      });

      // Load audio settings
      const enabled = await invoke<boolean>("get_audio_enabled");
      const volume = await invoke<number>("get_audio_volume");
      setAudioEnabled(enabled);
      setAudioVolume(volume);
    } catch (e) {
      console.error("Failed to load settings", e);
      toast.error('Failed to load settings');
    }
  };

  const saveSettings = async () => {
    try {
      await invoke("save_settings", { settings });
      // Save audio settings
      await invoke("set_audio_enabled", { enabled: audioEnabled });
      await invoke("set_audio_volume", { volume: audioVolume });
      setSaved(true);
      toast.success("Settings Saved Successfully");
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save settings", e);
      toast.error("Failed to save settings: " + e);
    }
  };

  if (!isOpen) return null;

  const tabs = [
    { id: "general", label: "General", icon: Settings },
    { id: "network", label: "Network", icon: Globe },
    { id: "cloud", label: "Cloud", icon: Cloud },
    { id: "notifications", label: "Notifications", icon: Volume2 },
    { id: "advanced", label: "Advanced", icon: Activity },
  ] as const;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4">
      <motion.div
        className="relative w-full max-w-5xl h-full max-h-[85vh] bg-[#1a1c23] border border-white/5 shadow-2xl flex rounded-2xl overflow-hidden"
        initial={{ scale: 0.95, opacity: 0, y: 20 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        exit={{ scale: 0.95, opacity: 0, y: 20 }}
      >
        {/* Sidebar */}
        <div className="w-64 bg-[#14151a] border-r border-white/5 flex flex-col">
          <div className="p-6 flex items-center gap-3">
            <div className="p-2 bg-blue-500/10 rounded-lg text-blue-400">
              <Settings size={22} />
            </div>
            <h2 className="text-lg font-bold text-slate-200">Settings</h2>
          </div>

          <nav className="flex-1 px-4 space-y-1">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as TabId)}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-xl text-sm font-medium transition-all duration-200 ${
                    isActive
                      ? "bg-blue-500/10 text-blue-400 shadow-sm"
                      : "text-slate-400 hover:bg-white/5 hover:text-slate-200"
                  }`}
                >
                  <Icon
                    size={18}
                    className={isActive ? "text-blue-400" : "text-slate-500"}
                  />
                  {tab.label}
                </button>
              );
            })}
          </nav>
        </div>

        {/* Main Content Area */}
        <div className="flex-1 flex flex-col overflow-hidden bg-[#1a1c23]">
          {/* Header */}
          <div className="h-16 flex items-center justify-end px-6 border-b border-white/5">
            <button
              onClick={onClose}
              className="p-2 hover:bg-white/10 rounded-lg transition-colors text-slate-400 hover:text-white"
            >
              <X size={20} />
            </button>
          </div>

          {/* Scrollable Tab Content */}
          <div className="flex-1 overflow-y-auto custom-scrollbar p-8">
            <div className="max-w-3xl mx-auto pb-12">
              <AnimatePresence mode="wait">
                <motion.div
                  key={activeTab}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  transition={{ duration: 0.2 }}
                >
                  {activeTab === "general" && (
                    <GeneralTab settings={settings} setSettings={setSettings} />
                  )}
                  {activeTab === "network" && (
                    <NetworkTab settings={settings} setSettings={setSettings} />
                  )}
                  {activeTab === "cloud" && (
                    <CloudTab settings={settings} setSettings={setSettings} />
                  )}
                  {activeTab === "notifications" && (
                    <NotificationsTab
                      settings={settings}
                      setSettings={setSettings}
                      audioEnabled={audioEnabled}
                      setAudioEnabled={setAudioEnabled}
                      audioVolume={audioVolume}
                      setAudioVolume={setAudioVolume}
                    />
                  )}
                  {activeTab === "advanced" && (
                    <AdvancedTab
                      settings={settings}
                      setSettings={setSettings}
                    />
                  )}
                </motion.div>
              </AnimatePresence>
            </div>
          </div>

          {/* Footer Actions */}
          <div className="p-5 border-t border-white/5 bg-[#14151a]/50 flex justify-end gap-3 z-10">
            <button
              onClick={onClose}
              className="px-5 py-2 text-slate-400 hover:text-white hover:bg-white/5 rounded-lg transition-colors text-sm font-medium"
            >
              Cancel
            </button>
            <button
              onClick={saveSettings}
              className="px-6 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg shadow-lg shadow-blue-900/20 text-sm font-bold flex items-center gap-2 transition-all"
            >
              <Save size={16} />
              {saved ? "Saved" : "Save Changes"}
            </button>
          </div>
        </div>
      </motion.div>
    </div>
  );
};

export default SettingsPage;
