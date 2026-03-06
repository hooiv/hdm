import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { Plus, Trash2, X, Check, Globe, MapPin, Shield } from 'lucide-react';

interface GeofenceRule {
  id: string;
  url_pattern: string;
  region: string;
  proxy_type: string;
  proxy_address: string;
  enabled: boolean;
}

interface PresetRegion {
  region: string;
  description: string;
  proxy: string;
}

const proxyTypes = ['direct', 'tor', 'socks5', 'http'] as const;

const regionColors: Record<string, string> = {
  US: 'text-blue-400 bg-blue-500/10 border-blue-500/20',
  EU: 'text-emerald-400 bg-emerald-500/10 border-emerald-500/20',
  JP: 'text-pink-400 bg-pink-500/10 border-pink-500/20',
  TOR: 'text-purple-400 bg-purple-500/10 border-purple-500/20',
  DIRECT: 'text-slate-400 bg-slate-500/10 border-slate-500/20',
};

const getRegionStyle = (region: string) =>
  regionColors[region.toUpperCase()] || 'text-cyan-400 bg-cyan-500/10 border-cyan-500/20';

export const GeofenceTab: React.FC = () => {
  const [rules, setRules] = useState<GeofenceRule[]>([]);
  const [presets, setPresets] = useState<PresetRegion[]>([]);
  const [isAdding, setIsAdding] = useState(false);
  const [form, setForm] = useState({ url_pattern: '', region: '', proxy_type: 'direct', proxy_address: '' });
  const [testUrl, setTestUrl] = useState('');
  const [testResult, setTestResult] = useState<string | null>(null);

  const loadRules = useCallback(async () => {
    try {
      const [data, pr] = await Promise.all([
        invoke<GeofenceRule[]>('get_geofence_rules'),
        invoke<PresetRegion[]>('get_preset_regions'),
      ]);
      setRules(data);
      setPresets(pr);
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { loadRules(); }, [loadRules]);

  const handleAdd = async () => {
    if (!form.url_pattern || !form.region) return;
    try {
      await invoke('set_geofence_rule', {
        urlPattern: form.url_pattern,
        region: form.region,
        proxyType: form.proxy_type,
        proxyAddress: form.proxy_address,
      });
      setIsAdding(false);
      setForm({ url_pattern: '', region: '', proxy_type: 'direct', proxy_address: '' });
      loadRules();
    } catch { /* ignore */ }
  };

  const handleRemove = async (ruleId: string) => {
    try {
      await invoke('remove_geofence_rule', { ruleId });
      loadRules();
    } catch { /* ignore */ }
  };

  const handleToggle = async (ruleId: string) => {
    try {
      await invoke('toggle_geofence_rule', { ruleId });
      loadRules();
    } catch { /* ignore */ }
  };

  const handleTest = async () => {
    if (!testUrl) return;
    try {
      const result = await invoke<GeofenceRule | null>('match_geofence_cmd', { url: testUrl });
      setTestResult(result
        ? `Matched → Region: ${result.region}, Proxy: ${result.proxy_type} (${result.proxy_address || 'direct'})`
        : 'No rule matches — direct connection will be used.');
    } catch (err) { setTestResult(`Error: ${err}`); }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-bold text-white">Geofencing & Regional Routing</h3>
          <p className="text-xs text-slate-500 mt-0.5">
            Route downloads through region-specific proxies based on URL patterns
          </p>
        </div>
        <button
          onClick={() => setIsAdding(true)}
          className="px-3 py-1.5 rounded-lg text-xs font-medium text-cyan-400 bg-cyan-500/10 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors"
        >
          <Plus size={12} className="inline mr-1" /> Add Rule
        </button>
      </div>

      {/* Preset Regions Reference */}
      <div className="p-3 rounded-lg bg-slate-900/50 border border-slate-700/30">
        <div className="text-xs text-slate-400 font-medium mb-2 flex items-center gap-1.5">
          <MapPin size={12} /> Available Regions
        </div>
        <div className="flex flex-wrap gap-2">
          {presets.map(p => (
            <span
              key={p.region}
              className={`text-[10px] px-2 py-1 rounded border ${getRegionStyle(p.region)}`}
            >
              <b>{p.region}</b> — {p.description}
            </span>
          ))}
        </div>
      </div>

      {/* Test URL */}
      <div className="p-3 rounded-lg bg-slate-900/50 border border-slate-700/30 space-y-2">
        <label className="text-xs text-slate-400 font-medium flex items-center gap-1.5">
          <Shield size={12} /> Test URL routing
        </label>
        <div className="flex gap-2">
          <input
            type="text"
            value={testUrl}
            onChange={e => setTestUrl(e.target.value)}
            placeholder="https://example.jp/file.zip"
            className="flex-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-sm text-slate-200 focus:outline-none focus:border-cyan-500/50"
          />
          <button
            onClick={handleTest}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-blue-400 bg-blue-500/10 border border-blue-500/20 hover:bg-blue-500/20 transition-colors"
          >
            Test
          </button>
        </div>
        {testResult && (
          <div className="text-[10px] text-slate-400 font-mono bg-black/30 p-2 rounded">{testResult}</div>
        )}
      </div>

      {/* Add Rule Form */}
      <AnimatePresence>
        {isAdding && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="p-4 rounded-xl bg-slate-900/80 border border-cyan-500/20 space-y-3"
          >
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-bold text-white">New Geofence Rule</h4>
              <button onClick={() => setIsAdding(false)} className="text-slate-400 hover:text-white"><X size={16} /></button>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-[10px] text-slate-500 font-medium">URL Pattern (regex)</label>
                <input value={form.url_pattern} onChange={e => setForm({ ...form, url_pattern: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono" placeholder=".*\\.jp/" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Region</label>
                <input value={form.region} onChange={e => setForm({ ...form, region: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50" placeholder="JP" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Proxy Type</label>
                <select value={form.proxy_type} onChange={e => setForm({ ...form, proxy_type: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50">
                  {proxyTypes.map(t => <option key={t} value={t}>{t}</option>)}
                </select>
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Proxy Address</label>
                <input value={form.proxy_address} onChange={e => setForm({ ...form, proxy_address: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono" placeholder="socks5://127.0.0.1:9050" />
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button onClick={() => setIsAdding(false)} className="px-4 py-2 rounded-lg text-xs text-slate-400 hover:text-white transition-colors">Cancel</button>
              <button
                onClick={handleAdd}
                disabled={!form.url_pattern || !form.region}
                className="px-4 py-2 rounded-lg text-xs font-medium text-white bg-cyan-600 hover:bg-cyan-500 transition-colors disabled:opacity-40"
              >
                <Check size={12} className="inline mr-1" /> Create
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Rules List */}
      {rules.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-slate-500">
          <Globe size={32} className="mb-2 opacity-30" />
          <p className="text-sm">No geofence rules configured</p>
          <p className="text-xs mt-1 opacity-70">Add rules to route downloads through region-specific proxies</p>
        </div>
      ) : (
        <div className="space-y-2">
          {rules.map(rule => (
            <div key={rule.id} className={`flex items-center gap-3 p-3 rounded-lg border transition-colors ${rule.enabled ? 'bg-slate-900/50 border-slate-700/30' : 'bg-slate-900/20 border-slate-800/30 opacity-50'}`}>
              <button
                onClick={() => handleToggle(rule.id)}
                className={`w-8 h-4 rounded-full transition-colors relative flex-shrink-0 ${rule.enabled ? 'bg-cyan-500' : 'bg-slate-700'}`}
              >
                <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-white transition-all ${rule.enabled ? 'left-4' : 'left-0.5'}`} />
              </button>
              <span className={`text-[10px] px-2 py-0.5 rounded border font-bold ${getRegionStyle(rule.region)}`}>
                {rule.region}
              </span>
              <span className="text-xs font-mono text-slate-400 flex-1 truncate">{rule.url_pattern}</span>
              <span className="text-[10px] text-slate-500">{rule.proxy_type}</span>
              {rule.proxy_address && <span className="text-[10px] text-slate-600 font-mono truncate max-w-[140px]">{rule.proxy_address}</span>}
              <button
                onClick={() => handleRemove(rule.id)}
                className="p-1 text-slate-500 hover:text-red-400 transition-colors flex-shrink-0"
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
