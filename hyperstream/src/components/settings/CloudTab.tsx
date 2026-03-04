import React from "react";
import { Cloud } from "lucide-react";
import { SettingsData } from "./types";
import { Toggle, SectionHeader } from "./SharedComponents";
import { motion } from "framer-motion";

interface CloudTabProps {
  settings: SettingsData;
  setSettings: (s: SettingsData) => void;
}

export const CloudTab: React.FC<CloudTabProps> = ({
  settings,
  setSettings,
}) => {
  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <SectionHeader icon={Cloud} title="Cloud Bridge" />
      <div className="bg-slate-800/20 rounded-xl p-5 border border-slate-700/30">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h4 className="text-slate-200 font-medium">S3 Storage</h4>
            <p className="text-sm text-slate-500">
              Upload finished downloads to Cloud
            </p>
          </div>
          <Toggle
            checked={settings.cloud_enabled}
            onChange={(v) => setSettings({ ...settings, cloud_enabled: v })}
          />
        </div>

        {settings.cloud_enabled && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            className="grid gap-4 md:grid-cols-2 pt-2 border-t border-slate-700/30"
          >
            <div className="space-y-2 pt-4">
              <label className="text-xs font-semibold text-slate-500 uppercase">
                Endpoint
              </label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                value={settings.cloud_endpoint ?? ''}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cloud_endpoint: e.target.value,
                  })
                }
                placeholder="s3.amazonaws.com"
              />
            </div>
            <div className="space-y-2 pt-4">
              <label className="text-xs font-semibold text-slate-500 uppercase">
                Bucket
              </label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                value={settings.cloud_bucket ?? ''}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cloud_bucket: e.target.value,
                  })
                }
                placeholder="MyBucket"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-semibold text-slate-500 uppercase">
                Access Key
              </label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                value={settings.cloud_access_key ?? ''}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cloud_access_key: e.target.value,
                  })
                }
                type="password"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-semibold text-slate-500 uppercase">
                Secret Key
              </label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                value={settings.cloud_secret_key ?? ''}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cloud_secret_key: e.target.value,
                  })
                }
                type="password"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-semibold text-slate-500 uppercase">
                Region
              </label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 text-sm"
                value={settings.cloud_region ?? ''}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cloud_region: e.target.value,
                  })
                }
                placeholder="us-east-1"
              />
            </div>
          </motion.div>
        )}
      </div>
    </div>
  );
};
