import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error as logError } from '../../utils/logger';
import {
  Volume2,
  Home,
  Webhook,
  MessageSquare,
  Trash2,
  PlayCircle,
  Plus,
  X,
} from "lucide-react";
import { SettingsData, WebhookConfig } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { motion } from "framer-motion";
import { useToast } from "../../contexts/ToastContext";

interface NotificationsTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
  audioEnabled: boolean;
  setAudioEnabled: (val: boolean) => void;
  audioVolume: number;
  setAudioVolume: (val: number) => void;
}

export const NotificationsTab: React.FC<NotificationsTabProps> = ({
  settings,
  setSettings,
  audioEnabled,
  setAudioEnabled,
  audioVolume,
  setAudioVolume,
}) => {
  const toast = useToast();

  // Webhook State locally managed
  const [webhooks, setWebhooks] = useState<WebhookConfig[]>([]);
  const [showWebhookModal, setShowWebhookModal] = useState(false);
  const [editingWebhook, setEditingWebhook] = useState<WebhookConfig | null>(
    null,
  );

  // Webhook form state (replaces document.getElementById usage)
  const [webhookName, setWebhookName] = useState('');
  const [webhookUrl, setWebhookUrl] = useState('');
  const [webhookTemplate, setWebhookTemplate] = useState('Discord');
  const [eventStart, setEventStart] = useState(true);
  const [eventComplete, setEventComplete] = useState(true);
  const [eventError, setEventError] = useState(true);

  // Custom sound file paths (replaces DOM manipulation)
  const [customSoundPaths, setCustomSoundPaths] = useState<Record<string, string>>({
    start: '',
    complete: '',
    error: '',
  });

  useEffect(() => {
    loadWebhooks();
  }, []);

  // Initialize custom sound paths from settings
  useEffect(() => {
    setCustomSoundPaths({
      start: settings.custom_sound_start ?? '',
      complete: settings.custom_sound_complete ?? '',
      error: settings.custom_sound_error ?? '',
    });
  }, [settings.custom_sound_start, settings.custom_sound_complete, settings.custom_sound_error]);

  // Reset webhook form when modal opens
  useEffect(() => {
    if (showWebhookModal && !editingWebhook) {
      setWebhookName('');
      setWebhookUrl('');
      setWebhookTemplate('Discord');
      setEventStart(true);
      setEventComplete(true);
      setEventError(true);
    }
  }, [showWebhookModal, editingWebhook]);

  // Escape key handler for webhook modal
  useEffect(() => {
    if (!showWebhookModal) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowWebhookModal(false);
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showWebhookModal]);

  const loadWebhooks = async () => {
    try {
      const hooks = await invoke<WebhookConfig[]>("get_webhooks");
      setWebhooks(hooks);
    } catch (e) {
      logError("Failed to load webhooks", e);
      toast.error("Failed to load webhooks");
    }
  };

  const handleDeleteWebhook = async (id: string) => {
    try {
      await invoke("delete_webhook", { id });
      await loadWebhooks();
    } catch (e) {
      toast.error("Failed to delete webhook");
    }
  };

  const handleTestWebhook = async (id: string) => {
    try {
      await invoke("test_webhook", { id });
      toast.success("Test payload sent!");
    } catch (e) {
      toast.error("Failed to test webhook");
    }
  };

  const handleToggleWebhook = async (webhook: WebhookConfig) => {
    try {
      await invoke("update_webhook", {
        id: webhook.id,
        config: { ...webhook, enabled: !webhook.enabled },
      });
      await loadWebhooks();
    } catch (e) {
      toast.error("Failed to toggle webhook");
    }
  };

  const handleTestSound = async (soundType: string) => {
    try {
      await invoke("play_test_sound", { soundType });
    } catch (e) {
      logError("Failed to play test sound", e);
      toast.error("Failed to play test sound");
    }
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      {/* Sound Settings */}
      <div className="space-y-4">
        <SectionHeader icon={Volume2} title="Sound Events" />
        <div className="space-y-4 bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
          <Toggle
            label="Enable Sound Effects"
            checked={audioEnabled}
            onChange={(val) => setAudioEnabled(val)}
          />
          <p className="text-xs text-slate-500 leading-relaxed">
            Play audio notifications when downloads start, complete, or
            encounter errors.
          </p>

          {audioEnabled && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="space-y-4 pt-2"
            >
              <div className="space-y-2">
                <div className="flex justify-between items-center">
                  <label className="text-sm font-medium text-slate-400">
                    Volume
                  </label>
                  <span className="text-xs text-slate-500 font-mono">
                    {Math.round(audioVolume * 100)}%
                  </span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  value={audioVolume}
                  onChange={(e) => setAudioVolume(parseFloat(e.target.value))}
                  className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-blue-500"
                  style={{
                    background: `linear-gradient(to right, rgb(59, 130, 246) 0%, rgb(59, 130, 246) ${audioVolume * 100}%, rgb(51, 65, 85) ${audioVolume * 100}%, rgb(51, 65, 85) 100%)`,
                  }}
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-slate-400">
                  Test Sounds
                </label>
                <div className="grid grid-cols-3 gap-3">
                  <button
                    onClick={() => handleTestSound("success")}
                    className="px-4 py-2 bg-green-600/20 border border-green-600/30 hover:bg-green-600/30 text-green-400 rounded-lg transition-colors text-sm font-medium"
                  >
                    Success
                  </button>
                  <button
                    onClick={() => handleTestSound("error")}
                    className="px-4 py-2 bg-red-600/20 border border-red-600/30 hover:bg-red-600/30 text-red-400 rounded-lg transition-colors text-sm font-medium"
                  >
                    Error
                  </button>
                  <button
                    onClick={() => handleTestSound("start")}
                    className="px-4 py-2 bg-blue-600/20 border border-blue-600/30 hover:bg-blue-600/30 text-blue-400 rounded-lg transition-colors text-sm font-medium"
                  >
                    Start
                  </button>
                </div>
              </div>

              {/* Custom Sound Files */}
              <div className="space-y-3 pt-3 border-t border-slate-700/30">
                <label className="text-sm font-medium text-slate-400">
                  Custom Sound Files
                </label>
                <p className="text-xs text-slate-500 mb-2">
                  Override default sounds with your own WAV files.
                </p>
                {(["start", "complete", "error"] as const).map((eventType) => (
                  <div key={eventType} className="flex items-center gap-3">
                    <span className="text-xs text-slate-400 w-20 capitalize font-medium">
                      {eventType}
                    </span>
                    <input
                      type="text"
                      readOnly
                      placeholder="Default (embedded)"
                      className="flex-1 bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-1.5 text-slate-300 text-xs font-mono truncate"
                      value={customSoundPaths[eventType] || ''}
                    />
                    <button
                      onClick={async () => {
                        try {
                          const path = await invoke<string>("select_file", {
                            filter: "wav",
                          });
                          if (path) {
                            await invoke("set_custom_sound_path", {
                              eventType,
                              path,
                            });
                            setCustomSoundPaths(prev => ({ ...prev, [eventType]: path }));
                          }
                        } catch (e) {
                          logError(e);
                          toast.error("Failed to set custom sound");
                        }
                      }}
                      className="px-3 py-1.5 bg-blue-500/20 text-blue-400 border border-blue-500/20 hover:bg-blue-500/30 rounded-lg text-xs font-medium transition-colors whitespace-nowrap"
                    >
                      Browse
                    </button>
                    <button
                      onClick={async () => {
                        try {
                          await invoke("clear_custom_sound_path", {
                            eventType,
                          });
                          setCustomSoundPaths(prev => ({ ...prev, [eventType]: '' }));
                        } catch (e) {
                          logError(e);
                          toast.error("Failed to clear custom sound");
                        }
                      }}
                      className="px-2 py-1.5 bg-slate-700/50 text-slate-400 hover:bg-slate-700 hover:text-slate-200 rounded-lg text-xs transition-colors"
                    >
                      Reset
                    </button>
                  </div>
                ))}
              </div>
            </motion.div>
          )}
        </div>
      </div>

      {/* Webhooks Section */}
      <div className="space-y-4">
        <SectionHeader icon={Webhook} title="Webhooks" />
        <div className="space-y-3">
          {webhooks.length === 0 && !showWebhookModal && (
            <p className="text-sm text-slate-500 text-center py-8">
              No webhooks configured. Click below to add one.
            </p>
          )}

          {webhooks.map((webhook) => (
            <div
              key={webhook.id}
              className="bg-slate-800/30 border border-slate-700/30 rounded-lg p-4 hover:border-slate-600/50 transition-all"
            >
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3 mb-2">
                    <h4 className="font-semibold text-white truncate">
                      {webhook.name}
                    </h4>
                    <span
                      className={`px-2 py-0.5 text-xs rounded ${
                        webhook.template === "Discord"
                          ? "bg-indigo-500/20 text-indigo-400"
                          : webhook.template === "Slack"
                            ? "bg-purple-500/20 text-purple-400"
                            : webhook.template === "Plex"
                              ? "bg-orange-500/20 text-orange-400"
                              : "bg-slate-500/20 text-slate-400"
                      }`}
                    >
                      {webhook.template}
                    </span>
                    <span
                      className={`px-2 py-0.5 text-xs rounded ${webhook.enabled ? "bg-green-500/20 text-green-400" : "bg-slate-500/20 text-slate-400"}`}
                    >
                      {webhook.enabled ? "Enabled" : "Disabled"}
                    </span>
                  </div>
                  <p className="text-xs text-slate-500 truncate mb-2">
                    {webhook.url}
                  </p>
                  <div className="flex gap-2 flex-wrap">
                    {webhook.events.map((event) => (
                      <span
                        key={event}
                        className="text-xs px-2 py-1 bg-blue-500/10 text-blue-400 rounded border border-blue-500/20"
                      >
                        {event}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="flex gap-2 flex-shrink-0">
                  <button
                    onClick={() => handleToggleWebhook(webhook)}
                    className={`p-2 rounded transition-colors ${webhook.enabled ? "bg-green-500/20 text-green-400 hover:bg-green-500/30" : "bg-slate-700 text-slate-400 hover:bg-slate-600"}`}
                    title={webhook.enabled ? "Disable" : "Enable"}
                  >
                    {webhook.enabled ? "✓" : "○"}
                  </button>
                  <button
                    onClick={() => handleTestWebhook(webhook.id)}
                    className="p-2 bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 rounded transition-colors"
                    title="Test Webhook"
                  >
                    <PlayCircle className="w-4 h-4" />
                  </button>
                  <button
                    onClick={() => handleDeleteWebhook(webhook.id)}
                    className="p-2 bg-red-500/20 text-red-400 hover:bg-red-500/30 rounded transition-colors"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}

          {!showWebhookModal && (
            <button
              onClick={() => {
                setEditingWebhook(null);
                setShowWebhookModal(true);
              }}
              className="w-full py-3 px-4 bg-blue-500/20 border border-blue-500/30 hover:bg-blue-500/30 text-blue-400 rounded-lg transition-colors flex items-center justify-center gap-2 font-medium"
            >
              <Plus className="w-5 h-5" /> Add Webhook
            </button>
          )}

          {showWebhookModal && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="bg-slate-800/50 border border-slate-700/50 rounded-lg p-5 space-y-4"
            >
              <div className="flex justify-between items-center">
                <h3 className="font-semibold text-white">
                  {editingWebhook ? "Edit Webhook" : "Add New Webhook"}
                </h3>
                <button
                  onClick={() => setShowWebhookModal(false)}
                  className="text-slate-400 hover:text-white"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>

              <div className="space-y-3">
                <div>
                  <label className="block text-sm font-medium text-slate-400 mb-1.5">
                    Name
                  </label>
                  <input
                    type="text"
                    placeholder="My Discord Webhook"
                    className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white focus:border-blue-500 focus:outline-none"
                    value={webhookName}
                    onChange={(e) => setWebhookName(e.target.value)}
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-400 mb-1.5">
                    Webhook URL
                  </label>
                  <input
                    type="url"
                    placeholder="https://discord.com/api/webhooks/..."
                    className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white font-mono text-sm focus:border-blue-500 focus:outline-none"
                    value={webhookUrl}
                    onChange={(e) => setWebhookUrl(e.target.value)}
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-400 mb-1.5">
                    Template
                  </label>
                  <select
                    className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white focus:border-blue-500 focus:outline-none"
                    value={webhookTemplate}
                    onChange={(e) => setWebhookTemplate(e.target.value)}
                  >
                    <option value="Discord">Discord</option>
                    <option value="Slack">Slack</option>
                    <option value="Plex">Plex</option>
                    <option value="Gotify">Gotify</option>
                    <option value="Custom">Custom (Raw JSON)</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-400 mb-2">
                    Events
                  </label>
                  <div className="flex gap-3">
                    <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                      <input
                        type="checkbox"
                        className="rounded"
                        checked={eventStart}
                        onChange={(e) => setEventStart(e.target.checked)}
                      />{" "}
                      Download Start
                    </label>
                    <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                      <input
                        type="checkbox"
                        className="rounded"
                        checked={eventComplete}
                        onChange={(e) => setEventComplete(e.target.checked)}
                      />{" "}
                      Download Complete
                    </label>
                    <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                      <input
                        type="checkbox"
                        className="rounded"
                        checked={eventError}
                        onChange={(e) => setEventError(e.target.checked)}
                      />{" "}
                      Download Error
                    </label>
                  </div>
                </div>

                <div className="flex gap-3 pt-2">
                  <button
                    onClick={async () => {
                      const events = [];
                      if (eventStart) events.push("DownloadStart");
                      if (eventComplete) events.push("DownloadComplete");
                      if (eventError) events.push("DownloadError");

                      if (!webhookName || !webhookUrl || events.length === 0) {
                        toast.warning(
                          "Please fill all fields and select at least one event",
                        );
                        return;
                      }

                      // Validate URL format
                      try {
                        const parsed = new URL(webhookUrl);
                        if (!['http:', 'https:'].includes(parsed.protocol)) {
                          toast.error("Webhook URL must use http or https");
                          return;
                        }
                      } catch {
                        toast.error("Invalid webhook URL format");
                        return;
                      }

                      try {
                        await invoke("add_webhook", {
                          config: {
                            id: `webhook_${Date.now()}`,
                            name: webhookName,
                            url: webhookUrl,
                            events,
                            template: webhookTemplate,
                            enabled: true,
                            max_retries: 3,
                          },
                        });
                        setShowWebhookModal(false);
                        await loadWebhooks();
                      } catch (e) {
                        logError("Failed to add webhook", e);
                        toast.error("Failed to add webhook");
                      }
                    }}
                    disabled={!webhookName.trim() || !webhookUrl.trim()}
                    className="flex-1 px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    Save Webhook
                  </button>
                  <button
                    onClick={() => setShowWebhookModal(false)}
                    className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-300 rounded-lg font-medium transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </motion.div>
          )}
        </div>
      </div>

      {/* MQTT Smart Home Integration */}
      <div className="space-y-4">
        <SectionHeader icon={Home} title="Smart Home (MQTT)" />
        <div className="bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h4 className="text-slate-200 font-medium">MQTT Notifications</h4>
              <p className="text-sm text-slate-500">
                Publish download events to an MQTT broker.
              </p>
            </div>
            <Toggle
              checked={settings.mqtt_enabled}
              onChange={(v) => setSettings({ ...settings, mqtt_enabled: v })}
            />
          </div>

          {settings.mqtt_enabled && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="grid gap-4 pt-4 border-t border-slate-700/30"
            >
              <div className="space-y-2">
                <label className="text-xs font-semibold text-slate-500 uppercase">
                  Broker URL
                </label>
                <input
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:border-blue-500 focus:outline-none"
                  value={settings.mqtt_broker_url}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      mqtt_broker_url: e.target.value,
                    })
                  }
                  placeholder="mqtt://localhost:1883"
                />
              </div>
              <div className="space-y-2">
                <label className="text-xs font-semibold text-slate-500 uppercase">
                  Topic
                </label>
                <input
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm focus:border-blue-500 focus:outline-none"
                  value={settings.mqtt_topic}
                  onChange={(e) =>
                    setSettings({ ...settings, mqtt_topic: e.target.value })
                  }
                  placeholder="hyperstream/events"
                />
              </div>
            </motion.div>
          )}
        </div>
      </div>

      {/* ChatOps Settings */}
      <div className="bg-slate-800/50 rounded-xl p-6 border border-slate-700/50 backdrop-blur-sm">
        <SectionHeader icon={MessageSquare} title="ChatOps (Telegram)" />
        <div className="space-y-4">
          <Toggle
            label="Enable Telegram Bot"
            checked={settings.chatops_enabled || false}
            onChange={(val) =>
              setSettings({ ...settings, chatops_enabled: val })
            }
          />
          {settings.chatops_enabled && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              className="space-y-3 pt-2"
            >
              <div>
                <label className="block text-xs font-medium text-slate-400 mb-1">
                  Bot Token (from @BotFather)
                </label>
                <input
                  type="password"
                  value={settings.telegram_bot_token || ""}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      telegram_bot_token: e.target.value,
                    })
                  }
                  className="w-full bg-slate-900/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-purple-500 transition-colors"
                  placeholder="123456789:ABCdefGHIjklMNOpqrs..."
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-slate-400 mb-1">
                  Chat ID (Optional - Auto-detected)
                </label>
                <input
                  type="text"
                  value={settings.telegram_chat_id || ""}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      telegram_chat_id: e.target.value,
                    })
                  }
                  className="w-full bg-slate-900/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-purple-500 transition-colors"
                  placeholder="Automatically filled after first message"
                />
              </div>
            </motion.div>
          )}
        </div>
      </div>
    </div>
  );
};
