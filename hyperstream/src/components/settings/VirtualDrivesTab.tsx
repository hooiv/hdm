import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { Plus, X, Check, HardDrive, Trash2, RefreshCw } from 'lucide-react';

interface MountedDrive {
  letter: string;
  path: string;
  status: string;
}

export const VirtualDrivesTab: React.FC = () => {
  const [drives, setDrives] = useState<MountedDrive[]>([]);
  const [isAdding, setIsAdding] = useState(false);
  const [form, setForm] = useState({ path: '', letter: '' });
  const [error, setError] = useState<string | null>(null);

  const loadDrives = useCallback(async () => {
    try {
      const data = await invoke<MountedDrive[]>('list_virtual_drives');
      setDrives(data);
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { loadDrives(); }, [loadDrives]);

  const handleMount = async () => {
    if (!form.path || !form.letter) return;
    setError(null);
    try {
      await invoke('mount_drive', { path: form.path, letter: form.letter.toUpperCase() });
      setIsAdding(false);
      setForm({ path: '', letter: '' });
      loadDrives();
    } catch (err) { setError(String(err)); }
  };

  const handleUnmount = async (letter: string) => {
    try {
      await invoke('unmount_drive', { letter });
      loadDrives();
    } catch { /* ignore */ }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-bold text-white">Virtual Drives</h3>
          <p className="text-xs text-slate-500 mt-0.5">
            Mount download folders as Windows drive letters for quick access
          </p>
        </div>
        <div className="flex gap-2">
          <button onClick={loadDrives} className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-white/10 transition-colors">
            <RefreshCw size={14} />
          </button>
          <button
            onClick={() => setIsAdding(true)}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-cyan-400 bg-cyan-500/10 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors"
          >
            <Plus size={12} className="inline mr-1" /> Mount Drive
          </button>
        </div>
      </div>

      <AnimatePresence>
        {isAdding && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="p-4 rounded-xl bg-slate-900/80 border border-cyan-500/20 space-y-3"
          >
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-bold text-white">Mount New Drive</h4>
              <button onClick={() => { setIsAdding(false); setError(null); }} className="text-slate-400 hover:text-white"><X size={16} /></button>
            </div>
            <div className="grid grid-cols-[1fr_80px] gap-3">
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Folder Path (must be within download directory)</label>
                <input value={form.path} onChange={e => setForm({ ...form, path: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono" placeholder="C:\Users\you\Downloads\Videos" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Letter</label>
                <input
                  value={form.letter}
                  onChange={e => setForm({ ...form, letter: e.target.value.replace(/[^a-zA-Z]/g, '').slice(0, 1) })}
                  className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono text-center uppercase"
                  placeholder="Z"
                  maxLength={1}
                />
              </div>
            </div>
            {error && <div className="text-[10px] text-red-400 bg-red-500/10 px-3 py-1.5 rounded">{error}</div>}
            <div className="flex justify-end gap-2">
              <button onClick={() => { setIsAdding(false); setError(null); }} className="px-4 py-2 rounded-lg text-xs text-slate-400 hover:text-white transition-colors">Cancel</button>
              <button
                onClick={handleMount}
                disabled={!form.path || !form.letter}
                className="px-4 py-2 rounded-lg text-xs font-medium text-white bg-cyan-600 hover:bg-cyan-500 transition-colors disabled:opacity-40"
              >
                <Check size={12} className="inline mr-1" /> Mount
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {drives.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-slate-500">
          <HardDrive size={32} className="mb-2 opacity-30" />
          <p className="text-sm">No virtual drives mounted</p>
          <p className="text-xs mt-1 opacity-70">Mount a download folder as a drive letter for quick file access</p>
        </div>
      ) : (
        <div className="space-y-2">
          {drives.map(d => (
            <div key={d.letter} className="flex items-center gap-3 p-3 rounded-lg bg-slate-900/50 border border-slate-700/30">
              <div className="w-10 h-10 rounded-lg bg-blue-500/10 border border-blue-500/20 flex items-center justify-center text-blue-400 font-bold text-sm">
                {d.letter}:
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-sm text-slate-200 font-mono truncate">{d.path}</div>
                <div className="text-[10px] text-emerald-400 mt-0.5">{d.status}</div>
              </div>
              <button
                onClick={() => handleUnmount(d.letter)}
                className="p-1.5 text-slate-500 hover:text-red-400 transition-colors"
                title="Unmount drive"
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
