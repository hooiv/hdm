import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Plus, Trash2, Edit3, X, Check, Globe, Download, ChevronDown, ChevronUp
} from 'lucide-react';

interface HeaderPair {
  key: string;
  value: string;
}

interface SiteRule {
  id: string;
  name: string;
  pattern: string;
  enabled: boolean;
  max_connections: number | null;
  max_segments: number | null;
  speed_limit_bps: number | null;
  user_agent: string | null;
  referer: string | null;
  custom_headers: HeaderPair[];
  max_retries: number | null;
  retry_delay_ms: number | null;
  exponential_backoff: boolean | null;
  auth_username: string | null;
  auth_password: string | null;
  cookie: string | null;
  download_dir: string | null;
  force_dpi_evasion: boolean | null;
  skip_ssl_verify: boolean | null;
  min_file_size: number | null;
  file_extensions: string[];
  priority: number;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

const emptyRule = (): SiteRule => ({
  id: crypto.randomUUID(),
  name: '',
  pattern: '',
  enabled: true,
  max_connections: null,
  max_segments: null,
  speed_limit_bps: null,
  user_agent: null,
  referer: null,
  custom_headers: [],
  max_retries: null,
  retry_delay_ms: null,
  exponential_backoff: null,
  auth_username: null,
  auth_password: null,
  cookie: null,
  download_dir: null,
  force_dpi_evasion: null,
  skip_ssl_verify: null,
  min_file_size: null,
  file_extensions: [],
  priority: 0,
  notes: null,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});

export const SiteRulesTab: React.FC = () => {
  const [rules, setRules] = useState<SiteRule[]>([]);
  const [editingRule, setEditingRule] = useState<SiteRule | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [testUrl, setTestUrl] = useState('');
  const [testResult, setTestResult] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const loadRules = useCallback(async () => {
    try {
      const data = await invoke<SiteRule[]>('list_site_rules');
      setRules(data);
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { loadRules(); }, [loadRules]);

  const handleSave = async (rule: SiteRule) => {
    try {
      rule.updated_at = new Date().toISOString();
      if (isCreating) {
        await invoke('add_site_rule', { rule });
      } else {
        await invoke('update_site_rule', { rule });
      }
      setEditingRule(null);
      setIsCreating(false);
      loadRules();
    } catch { /* ignore */ }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm('Delete this site rule?')) return;
    try {
      await invoke('delete_site_rule', { id });
      loadRules();
    } catch { /* ignore */ }
  };

  const handleImportPresets = async () => {
    try {
      const count = await invoke<number>('import_site_rule_presets');
      loadRules();
      alert(`Imported ${count} preset rules.`);
    } catch { /* ignore */ }
  };

  const handleTest = async () => {
    if (!testUrl) return;
    try {
      const result = await invoke<{ matched_rules: string[]; max_connections: number | null; max_segments: number | null; user_agent: string | null }>('test_site_rule', { url: testUrl });
      setTestResult(
        result.matched_rules.length > 0
          ? `Matched: ${result.matched_rules.join(', ')}\nConnections: ${result.max_connections ?? 'default'}\nSegments: ${result.max_segments ?? 'default'}\nUser-Agent: ${result.user_agent ?? 'default'}`
          : 'No rules matched this URL.'
      );
    } catch (err) { setTestResult(`Error: ${err}`); }
  };

  const handleToggle = async (rule: SiteRule) => {
    rule.enabled = !rule.enabled;
    rule.updated_at = new Date().toISOString();
    try {
      await invoke('update_site_rule', { rule });
      loadRules();
    } catch { /* ignore */ }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-bold text-white">Site Rules</h3>
          <p className="text-xs text-slate-500 mt-0.5">
            Configure per-domain download settings (connections, headers, auth, speed limits)
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={handleImportPresets}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-amber-400 bg-amber-500/10 border border-amber-500/20 hover:bg-amber-500/20 transition-colors"
          >
            <Download size={12} className="inline mr-1" /> Import Presets
          </button>
          <button
            onClick={() => { setEditingRule(emptyRule()); setIsCreating(true); }}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-cyan-400 bg-cyan-500/10 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors"
          >
            <Plus size={12} className="inline mr-1" /> Add Rule
          </button>
        </div>
      </div>

      {/* Test URL */}
      <div className="p-3 rounded-lg bg-slate-900/50 border border-slate-700/30 space-y-2">
        <label className="text-xs text-slate-400 font-medium">Test URL against rules</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={testUrl}
            onChange={e => setTestUrl(e.target.value)}
            placeholder="https://example.com/file.zip"
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
          <pre className="text-[10px] text-slate-400 whitespace-pre-wrap font-mono bg-black/30 p-2 rounded">{testResult}</pre>
        )}
      </div>

      {/* Rule Editor Modal */}
      <AnimatePresence>
        {editingRule && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="p-4 rounded-xl bg-slate-900/80 border border-cyan-500/20 space-y-4"
          >
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-bold text-white">{isCreating ? 'New Site Rule' : 'Edit Site Rule'}</h4>
              <button onClick={() => { setEditingRule(null); setIsCreating(false); }} className="text-slate-400 hover:text-white"><X size={16} /></button>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <Field label="Name" value={editingRule.name} onChange={v => setEditingRule({ ...editingRule, name: v })} placeholder="GitHub Releases" />
              <Field label="Pattern (glob)" value={editingRule.pattern} onChange={v => setEditingRule({ ...editingRule, pattern: v })} placeholder="*.github.com" />
              <Field label="Max Connections" value={editingRule.max_connections?.toString() ?? ''} onChange={v => setEditingRule({ ...editingRule, max_connections: v ? parseInt(v) || null : null })} placeholder="8" type="number" />
              <Field label="Max Segments" value={editingRule.max_segments?.toString() ?? ''} onChange={v => setEditingRule({ ...editingRule, max_segments: v ? parseInt(v) || null : null })} placeholder="16" type="number" />
              <Field label="Speed Limit (bytes/s)" value={editingRule.speed_limit_bps?.toString() ?? ''} onChange={v => setEditingRule({ ...editingRule, speed_limit_bps: v ? parseInt(v) || null : null })} placeholder="0 = unlimited" type="number" />
              <Field label="User-Agent" value={editingRule.user_agent ?? ''} onChange={v => setEditingRule({ ...editingRule, user_agent: v || null })} placeholder="Custom UA string" />
              <Field label="Referer" value={editingRule.referer ?? ''} onChange={v => setEditingRule({ ...editingRule, referer: v || null })} placeholder="https://example.com" />
              <Field label="Cookie" value={editingRule.cookie ?? ''} onChange={v => setEditingRule({ ...editingRule, cookie: v || null })} placeholder="session=abc123" />
              <Field label="Auth Username" value={editingRule.auth_username ?? ''} onChange={v => setEditingRule({ ...editingRule, auth_username: v || null })} />
              <Field label="Auth Password" value={editingRule.auth_password ?? ''} onChange={v => setEditingRule({ ...editingRule, auth_password: v || null })} type="password" />
              <Field label="Download Dir" value={editingRule.download_dir ?? ''} onChange={v => setEditingRule({ ...editingRule, download_dir: v || null })} placeholder="/path/to/dir" />
              <Field label="Priority" value={editingRule.priority.toString()} onChange={v => setEditingRule({ ...editingRule, priority: parseInt(v) || 0 })} type="number" />
              <Field label="Max Retries" value={editingRule.max_retries?.toString() ?? ''} onChange={v => setEditingRule({ ...editingRule, max_retries: v ? parseInt(v) || null : null })} type="number" />
              <Field label="File Extensions" value={editingRule.file_extensions.join(', ')} onChange={v => setEditingRule({ ...editingRule, file_extensions: v.split(',').map(s => s.trim()).filter(Boolean) })} placeholder="zip, exe, rar" />
            </div>

            <div className="flex gap-4 text-xs">
              <label className="flex items-center gap-2 text-slate-400">
                <input type="checkbox" checked={editingRule.force_dpi_evasion ?? false} onChange={e => setEditingRule({ ...editingRule, force_dpi_evasion: e.target.checked })} className="rounded" />
                DPI Evasion
              </label>
              <label className="flex items-center gap-2 text-slate-400">
                <input type="checkbox" checked={editingRule.skip_ssl_verify ?? false} onChange={e => setEditingRule({ ...editingRule, skip_ssl_verify: e.target.checked })} className="rounded" />
                Skip SSL Verify
              </label>
              <label className="flex items-center gap-2 text-slate-400">
                <input type="checkbox" checked={editingRule.exponential_backoff ?? false} onChange={e => setEditingRule({ ...editingRule, exponential_backoff: e.target.checked })} className="rounded" />
                Exponential Backoff
              </label>
            </div>

            <div>
              <label className="text-[10px] text-slate-500 font-medium">Notes</label>
              <textarea
                value={editingRule.notes ?? ''}
                onChange={e => setEditingRule({ ...editingRule, notes: e.target.value || null })}
                className="w-full mt-1 px-3 py-2 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 resize-none h-16"
                placeholder="Optional notes..."
              />
            </div>

            <div className="flex justify-end gap-2">
              <button onClick={() => { setEditingRule(null); setIsCreating(false); }} className="px-4 py-2 rounded-lg text-xs text-slate-400 hover:text-white transition-colors">Cancel</button>
              <button
                onClick={() => handleSave(editingRule)}
                disabled={!editingRule.name || !editingRule.pattern}
                className="px-4 py-2 rounded-lg text-xs font-medium text-white bg-cyan-600 hover:bg-cyan-500 transition-colors disabled:opacity-40"
              >
                <Check size={12} className="inline mr-1" /> {isCreating ? 'Create' : 'Save'}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Rules List */}
      {rules.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-slate-500">
          <Globe size={32} className="mb-2 opacity-30" />
          <p className="text-sm">No site rules configured</p>
          <p className="text-xs mt-1 opacity-70">Import presets or add custom rules for per-site download behavior</p>
        </div>
      ) : (
        <div className="space-y-2">
          {rules.map(rule => (
            <div key={rule.id} className={`rounded-lg border transition-colors ${rule.enabled ? 'bg-slate-900/50 border-slate-700/30' : 'bg-slate-900/20 border-slate-800/30 opacity-60'}`}>
              <div
                className="flex items-center gap-3 p-3 cursor-pointer"
                onClick={() => setExpandedId(expandedId === rule.id ? null : rule.id)}
              >
                <button
                  onClick={e => { e.stopPropagation(); handleToggle(rule); }}
                  className={`w-8 h-4 rounded-full transition-colors relative ${rule.enabled ? 'bg-cyan-500' : 'bg-slate-700'}`}
                >
                  <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-white transition-all ${rule.enabled ? 'left-4' : 'left-0.5'}`} />
                </button>
                <Globe size={14} className={rule.enabled ? 'text-cyan-400' : 'text-slate-500'} />
                <span className="text-sm font-medium text-slate-200 flex-1">{rule.name}</span>
                <span className="text-[10px] text-slate-500 font-mono">{rule.pattern}</span>
                {rule.priority > 0 && (
                  <span className="text-[9px] px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20">P{rule.priority}</span>
                )}
                <div className="flex gap-1">
                  <button
                    onClick={e => { e.stopPropagation(); setEditingRule({ ...rule }); setIsCreating(false); }}
                    className="p-1 text-slate-500 hover:text-cyan-400 transition-colors"
                  >
                    <Edit3 size={12} />
                  </button>
                  <button
                    onClick={e => { e.stopPropagation(); handleDelete(rule.id); }}
                    className="p-1 text-slate-500 hover:text-red-400 transition-colors"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
                {expandedId === rule.id ? <ChevronUp size={14} className="text-slate-500" /> : <ChevronDown size={14} className="text-slate-500" />}
              </div>

              <AnimatePresence>
                {expandedId === rule.id && (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: 'auto', opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    className="overflow-hidden"
                  >
                    <div className="px-4 pb-3 grid grid-cols-3 gap-2 text-[10px] text-slate-400 border-t border-slate-800/50 pt-2">
                      {rule.max_connections && <span>Connections: <b className="text-slate-300">{rule.max_connections}</b></span>}
                      {rule.max_segments && <span>Segments: <b className="text-slate-300">{rule.max_segments}</b></span>}
                      {rule.speed_limit_bps && <span>Speed limit: <b className="text-slate-300">{(rule.speed_limit_bps / 1024).toFixed(0)} KB/s</b></span>}
                      {rule.user_agent && <span className="col-span-3 truncate">UA: <b className="text-slate-300">{rule.user_agent}</b></span>}
                      {rule.referer && <span className="col-span-3 truncate">Referer: <b className="text-slate-300">{rule.referer}</b></span>}
                      {rule.download_dir && <span className="col-span-3 truncate">Dir: <b className="text-slate-300">{rule.download_dir}</b></span>}
                      {rule.file_extensions.length > 0 && <span className="col-span-3">Extensions: <b className="text-slate-300">{rule.file_extensions.join(', ')}</b></span>}
                      {rule.force_dpi_evasion && <span className="text-amber-400">DPI Evasion</span>}
                      {rule.skip_ssl_verify && <span className="text-red-400">Skip SSL</span>}
                      {rule.notes && <span className="col-span-3 italic text-slate-500">{rule.notes}</span>}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

const Field: React.FC<{
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}> = ({ label, value, onChange, placeholder, type = 'text' }) => (
  <div>
    <label className="text-[10px] text-slate-500 font-medium">{label}</label>
    <input
      type={type}
      value={value}
      onChange={e => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50"
    />
  </div>
);
