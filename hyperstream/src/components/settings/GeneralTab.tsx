import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, Activity, Moon, Clock, Plus, Trash2 } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { motion } from "framer-motion";
import { useToast } from "../../contexts/ToastContext";
import { error as logError } from "../../utils/logger";
import type { SpeedProfile } from "../../types";

interface GeneralTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
}

export const GeneralTab: React.FC<GeneralTabProps> = ({
  settings,
  setSettings,
}) => {
  const toast = useToast();
  const [editingProfile, setEditingProfile] = useState<number | null>(null);

  const handleSelectDir = async () => {
    try {
      const selected = await invoke<string>("select_directory");
      if (selected) {
        setSettings({ ...settings, download_dir: selected });
      }
    } catch (e) {
      logError("Failed to select directory", e);
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
                  segments: Math.min(32, Math.max(1, parseInt(e.target.value, 10) || 1)),
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
                  speed_limit_kbps: Math.max(0, parseInt(e.target.value, 10) || 0),
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

      {/* Quiet Hours */}
      <div className="space-y-4">
        <SectionHeader icon={Moon} title="Quiet Hours" />
        <div className="bg-slate-800/20 rounded-xl p-5 border border-slate-700/30 space-y-4">
          <div className="space-y-1">
            <Toggle
              label="Enable Quiet Hours"
              checked={!!settings.quiet_hours_enabled}
              onChange={(v) => setSettings({ ...settings, quiet_hours_enabled: v })}
            />
            <p className="text-xs text-slate-500 ml-1">
              Restrict download activity during a configurable time window.
            </p>
          </div>

          {settings.quiet_hours_enabled && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              className="space-y-4 pt-4 border-t border-slate-700/30"
            >
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <label className="text-xs font-medium text-slate-400">Start Hour</label>
                  <select
                    value={settings.quiet_hours_start ?? 23}
                    onChange={(e) => setSettings({ ...settings, quiet_hours_start: Number(e.target.value) })}
                    className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-violet-500/50"
                  >
                    {Array.from({ length: 24 }, (_, i) => (
                      <option key={i} value={i}>{String(i).padStart(2, '0')}:00</option>
                    ))}
                  </select>
                </div>
                <div className="space-y-1">
                  <label className="text-xs font-medium text-slate-400">End Hour</label>
                  <select
                    value={settings.quiet_hours_end ?? 7}
                    onChange={(e) => setSettings({ ...settings, quiet_hours_end: Number(e.target.value) })}
                    className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-violet-500/50"
                  >
                    {Array.from({ length: 24 }, (_, i) => (
                      <option key={i} value={i}>{String(i).padStart(2, '0')}:00</option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="space-y-2">
                <label className="text-xs font-medium text-slate-400">Action During Quiet Hours</label>
                <div className="flex gap-3">
                  <button
                    onClick={() => setSettings({ ...settings, quiet_hours_action: 'defer' })}
                    className={`flex-1 px-3 py-2 rounded-lg text-sm font-medium border transition-colors ${
                      (settings.quiet_hours_action || 'defer') === 'defer'
                        ? 'bg-violet-500/20 border-violet-500/40 text-violet-300'
                        : 'bg-slate-800/50 border-slate-700 text-slate-400 hover:text-slate-300'
                    }`}
                  >
                    Defer New Downloads
                  </button>
                  <button
                    onClick={() => setSettings({ ...settings, quiet_hours_action: 'throttle' })}
                    className={`flex-1 px-3 py-2 rounded-lg text-sm font-medium border transition-colors ${
                      settings.quiet_hours_action === 'throttle'
                        ? 'bg-violet-500/20 border-violet-500/40 text-violet-300'
                        : 'bg-slate-800/50 border-slate-700 text-slate-400 hover:text-slate-300'
                    }`}
                  >
                    Throttle Speed
                  </button>
                </div>
              </div>

              {settings.quiet_hours_action === 'throttle' && (
                <div className="space-y-1">
                  <label className="text-xs font-medium text-slate-400">Throttle Speed (KB/s)</label>
                  <input
                    type="number"
                    min={1}
                    max={99999}
                    value={settings.quiet_hours_throttle_kbps ?? 50}
                    onChange={(e) => setSettings({ ...settings, quiet_hours_throttle_kbps: Math.max(1, Number(e.target.value) || 50) })}
                    className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm font-mono focus:outline-none focus:border-violet-500/50"
                  />
                  <p className="text-xs text-slate-500">Maximum download speed during quiet hours.</p>
                </div>
              )}

              <div className="bg-violet-500/5 border border-violet-500/10 rounded-lg px-3 py-2">
                <p className="text-xs text-violet-400/80">
                  {(settings.quiet_hours_action || 'defer') === 'defer'
                    ? `Scheduled downloads due between ${String(settings.quiet_hours_start ?? 23).padStart(2, '0')}:00 and ${String(settings.quiet_hours_end ?? 7).padStart(2, '0')}:00 will be deferred until quiet hours end.`
                    : `Downloads will be throttled to ${settings.quiet_hours_throttle_kbps ?? 50} KB/s between ${String(settings.quiet_hours_start ?? 23).padStart(2, '0')}:00 and ${String(settings.quiet_hours_end ?? 7).padStart(2, '0')}:00.`}
                </p>
              </div>
            </motion.div>
          )}
        </div>
      </div>

      {/* Speed Profiles */}
      <div className="bg-slate-800/30 rounded-xl p-5 border border-white/5">
        <div className="flex items-center justify-between mb-4">
          <SectionHeader icon={Clock} title="Speed Profiles" subtitle="Time-based bandwidth scheduling" />
          <Toggle
            checked={settings.speed_profiles_enabled ?? false}
            onChange={(val) => setSettings({ ...settings, speed_profiles_enabled: val })}
          />
        </div>

        {settings.speed_profiles_enabled && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            className="space-y-3 pt-4 border-t border-slate-700/30"
          >
            {(settings.speed_profiles ?? []).map((profile: SpeedProfile, idx: number) => (
              <div key={idx} className="bg-slate-900/50 border border-slate-700/30 rounded-lg p-3 space-y-2">
                {editingProfile === idx ? (
                  <div className="space-y-3">
                    <input
                      type="text"
                      value={profile.name}
                      onChange={(e) => {
                        const profiles = [...(settings.speed_profiles ?? [])];
                        profiles[idx] = { ...profiles[idx], name: e.target.value };
                        setSettings({ ...settings, speed_profiles: profiles });
                      }}
                      placeholder="Profile Name"
                      className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                    />
                    <div className="grid grid-cols-3 gap-3">
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">Start Time</label>
                        <input
                          type="time"
                          value={profile.start_time}
                          onChange={(e) => {
                            const profiles = [...(settings.speed_profiles ?? [])];
                            profiles[idx] = { ...profiles[idx], start_time: e.target.value };
                            setSettings({ ...settings, speed_profiles: profiles });
                          }}
                          className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">End Time</label>
                        <input
                          type="time"
                          value={profile.end_time}
                          onChange={(e) => {
                            const profiles = [...(settings.speed_profiles ?? [])];
                            profiles[idx] = { ...profiles[idx], end_time: e.target.value };
                            setSettings({ ...settings, speed_profiles: profiles });
                          }}
                          className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:outline-none focus:border-cyan-500/50"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs text-slate-400">Speed (KB/s)</label>
                        <input
                          type="number"
                          min={0}
                          value={profile.speed_limit_kbps}
                          onChange={(e) => {
                            const profiles = [...(settings.speed_profiles ?? [])];
                            profiles[idx] = { ...profiles[idx], speed_limit_kbps: Math.max(0, Number(e.target.value) || 0) };
                            setSettings({ ...settings, speed_profiles: profiles });
                          }}
                          placeholder="0 = unlimited"
                          className="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm font-mono focus:outline-none focus:border-cyan-500/50"
                        />
                      </div>
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">Active Days</label>
                      <div className="flex gap-1.5">
                        {['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'].map((day, dayIdx) => {
                          const isActive = (profile.days ?? []).length === 0 || (profile.days ?? []).includes(dayIdx);
                          return (
                            <button
                              key={day}
                              onClick={() => {
                                const profiles = [...(settings.speed_profiles ?? [])];
                                const currentDays = profiles[idx].days ?? [];
                                if (currentDays.length === 0) {
                                  // Was "every day" — switch to all except this one
                                  profiles[idx] = { ...profiles[idx], days: [0,1,2,3,4,5,6].filter(d => d !== dayIdx) };
                                } else if (currentDays.includes(dayIdx)) {
                                  const newDays = currentDays.filter((d: number) => d !== dayIdx);
                                  profiles[idx] = { ...profiles[idx], days: newDays };
                                } else {
                                  const newDays = [...currentDays, dayIdx].sort();
                                  // If all 7 selected, set to empty (= every day)
                                  profiles[idx] = { ...profiles[idx], days: newDays.length === 7 ? [] : newDays };
                                }
                                setSettings({ ...settings, speed_profiles: profiles });
                              }}
                              className={`px-2 py-1 rounded text-xs font-medium transition-colors ${
                                isActive
                                  ? 'bg-cyan-500/20 border border-cyan-500/40 text-cyan-300'
                                  : 'bg-slate-800/50 border border-slate-700 text-slate-500'
                              }`}
                            >
                              {day}
                            </button>
                          );
                        })}
                      </div>
                      <p className="text-xs text-slate-500">{(profile.days ?? []).length === 0 ? 'Active every day' : `Active ${(profile.days ?? []).length} days/week`}</p>
                    </div>
                    <button
                      onClick={() => setEditingProfile(null)}
                      className="text-xs text-cyan-400 hover:text-cyan-300"
                    >
                      Done Editing
                    </button>
                  </div>
                ) : (
                  <div className="flex items-center justify-between">
                    <div
                      className="flex-1 cursor-pointer"
                      onClick={() => setEditingProfile(idx)}
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium text-slate-200">{profile.name || 'Unnamed Profile'}</span>
                        <span className="text-xs text-slate-500">
                          {profile.start_time} — {profile.end_time}
                        </span>
                      </div>
                      <div className="flex items-center gap-2 mt-0.5">
                        <span className="text-xs text-cyan-400 font-mono">
                          {profile.speed_limit_kbps === 0 ? 'Unlimited' : `${profile.speed_limit_kbps} KB/s`}
                        </span>
                        <span className="text-xs text-slate-600">
                          {(profile.days ?? []).length === 0 ? 'Every day' : (profile.days ?? []).map((d: number) => ['Mo','Tu','We','Th','Fr','Sa','Su'][d]).join(', ')}
                        </span>
                      </div>
                    </div>
                    <button
                      onClick={() => {
                        const profiles = (settings.speed_profiles ?? []).filter((_: SpeedProfile, i: number) => i !== idx);
                        setSettings({ ...settings, speed_profiles: profiles });
                      }}
                      className="p-1.5 text-slate-500 hover:text-red-400 transition-colors"
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                )}
              </div>
            ))}

            {/* Quick preset templates */}
            {(settings.speed_profiles ?? []).length === 0 && (
              <div className="space-y-2 mb-2">
                <p className="text-xs text-slate-500">Quick start with a preset:</p>
                <div className="flex flex-wrap gap-2">
                  {[
                    { name: 'Work Hours', start: '09:00', end: '17:00', speed: 500, days: [0,1,2,3,4], label: 'Mon-Fri 9-5, 500 KB/s' },
                    { name: 'Night Unlimited', start: '23:00', end: '07:00', speed: 0, days: [], label: 'Every night, unlimited' },
                    { name: 'Low Priority', start: '00:00', end: '23:59', speed: 100, days: [], label: 'All day, 100 KB/s' },
                  ].map((preset) => (
                    <button
                      key={preset.name}
                      onClick={() => {
                        const profiles = [...(settings.speed_profiles ?? []), {
                          name: preset.name,
                          start_time: preset.start,
                          end_time: preset.end,
                          speed_limit_kbps: preset.speed,
                          days: preset.days,
                        }];
                        setSettings({ ...settings, speed_profiles: profiles });
                      }}
                      className="flex flex-col items-start px-3 py-2 bg-slate-800/50 border border-slate-700/50 rounded-lg hover:border-cyan-500/40 hover:bg-cyan-500/5 transition-colors group"
                    >
                      <span className="text-xs font-medium text-slate-300 group-hover:text-cyan-300">{preset.name}</span>
                      <span className="text-[10px] text-slate-500">{preset.label}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            <button
              onClick={() => {
                const profiles = [...(settings.speed_profiles ?? []), {
                  name: `Profile ${(settings.speed_profiles ?? []).length + 1}`,
                  start_time: '09:00',
                  end_time: '17:00',
                  speed_limit_kbps: 500,
                  days: [],
                }];
                setSettings({ ...settings, speed_profiles: profiles });
                setEditingProfile(profiles.length - 1);
              }}
              className="w-full flex items-center justify-center gap-2 px-3 py-2.5 border border-dashed border-slate-600 rounded-lg text-sm text-slate-400 hover:text-cyan-400 hover:border-cyan-500/40 transition-colors"
            >
              <Plus size={14} />
              Add Speed Profile
            </button>

            <div className="bg-cyan-500/5 border border-cyan-500/10 rounded-lg px-3 py-2">
              <p className="text-xs text-cyan-400/80">
                Speed profiles override the base speed limit during their active time window. First matching profile wins. Set speed to 0 for unlimited.
              </p>
            </div>
          </motion.div>
        )}
      </div>
    </div>
  );
};
