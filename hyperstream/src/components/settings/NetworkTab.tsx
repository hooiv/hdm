import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Globe, Shield, Users, Activity } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { motion } from "framer-motion";
import { useToast } from "../../contexts/ToastContext";

interface NetworkTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
}

export const NetworkTab: React.FC<NetworkTabProps> = ({
  settings,
  setSettings,
}) => {
  const toast = useToast();

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <SectionHeader icon={Globe} title="Network & Privacy" />

      {/* Proxies and VPN */}
      <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
        <Toggle
          label="Enable Proxy"
          checked={settings.proxy_enabled}
          onChange={(val) => setSettings({ ...settings, proxy_enabled: val })}
        />

        {settings.proxy_enabled && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            className="pt-2 space-y-3"
          >
            <div>
              <label className="text-sm font-medium text-slate-400 mb-2 block">
                Proxy Host
              </label>
              <input
                type="text"
                placeholder="127.0.0.1"
                value={settings.proxy_host}
                onChange={(e) =>
                  setSettings({ ...settings, proxy_host: e.target.value })
                }
                className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
              />
            </div>
            <div>
              <label className="text-sm font-medium text-slate-400 mb-2 block">
                Proxy Port
              </label>
              <input
                type="number"
                placeholder="8080"
                value={settings.proxy_port}
                onChange={(e) =>
                  setSettings({ ...settings, proxy_port: parseInt(e.target.value) || 8080 })
                }
                className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
              />
            </div>
          </motion.div>
        )}

        <div className="h-px bg-slate-700/50 my-2" />

        <div className="mt-4 pt-4 border-t border-slate-700/30">
          <h4 className="text-slate-200 font-medium mb-2">VPN Auto-Connect</h4>
          <p className="text-sm text-slate-500 mb-4">
            Automatically dial a VPN connection before starting downloads.
          </p>
          <Toggle
            label="Enable VPN Auto-Connect"
            checked={settings.vpn_auto_connect}
            onChange={(val) =>
              setSettings({ ...settings, vpn_auto_connect: val })
            }
          />
          {settings.vpn_auto_connect && (
            <div className="pt-3">
              <label className="text-sm font-medium text-slate-400 mb-2 block">
                VPN Connection Name
              </label>
              <input
                type="text"
                placeholder="e.g. MyVPN"
                value={settings.vpn_connection_name}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    vpn_connection_name: e.target.value,
                  })
                }
                className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 font-mono text-sm focus:outline-none focus:border-blue-500/50"
              />
            </div>
          )}
        </div>
      </div>

      {/* Tor Section */}
      <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-slate-200 font-medium flex items-center gap-2">
              <Shield size={16} className="text-purple-400" />
              Tor Network
            </h4>
            <p className="text-sm text-slate-500">
              Route all traffic through Onion network
            </p>
          </div>
          <Toggle
            checked={settings.use_tor}
            onChange={async (val) => {
              setSettings({ ...settings, use_tor: val });
              if (val) {
                try {
                  await invoke("init_tor_network");
                  toast.success("Tor network initialized");
                } catch (err) {
                  console.error("Tor init failed:", err);
                  toast.error(`Failed to initialize Tor: ${err}`);
                  setSettings({ ...settings, use_tor: false });
                }
              }
            }}
          />
        </div>
        {settings.use_tor && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            className="text-xs text-purple-300 bg-purple-900/20 p-3 rounded border border-purple-500/20 flex gap-2"
          >
            <Activity size={12} className="mt-0.5 shrink-0" />
            <span>
              <b>Privacy Mode Active:</b> Connection speeds will be
              significantly slower. Initial bootstrap may take 30-60 seconds.
            </span>
          </motion.div>
        )}
      </div>

      {/* Team Sync */}
      <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-slate-200 font-medium flex items-center gap-2">
              <Users size={16} className="text-green-400" />
              Team Sync (Shared Workspace)
            </h4>
            <p className="text-sm text-slate-500">
              Automatically sync downloads with local peers.
            </p>
          </div>
          <div className="flex gap-2">
            <input
              placeholder="Host IP (e.g. 192.168.1.5)"
              className="bg-slate-900 border border-slate-700 rounded px-3 py-1 text-xs text-slate-300 w-40"
              value={settings.last_sync_host || ""}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  last_sync_host: e.target.value,
                })
              }
            />
            <button
              onClick={() => {
                if (settings.last_sync_host) {
                  invoke("join_workspace", {
                    hostIp: settings.last_sync_host,
                  })
                    .then(() => toast.success("Connected to Workspace!"))
                    .catch((e) => toast.error("Connection Failed: " + e));
                }
              }}
              className="bg-green-600/20 hover:bg-green-600 text-green-400 hover:text-white px-3 py-1 rounded text-xs font-medium border border-green-500/20 transition-all"
            >
              Join
            </button>
          </div>
        </div>
        <div className="text-xs text-slate-500 font-mono bg-slate-900/50 p-2 rounded border border-slate-700/30">
          Your Host IP:{" "}
          <span className="text-slate-300 select-all">
            127.0.0.1 (Check LAN IP)
          </span>{" "}
          (Port: 8765)
        </div>
      </div>
    </div>
  );
};
