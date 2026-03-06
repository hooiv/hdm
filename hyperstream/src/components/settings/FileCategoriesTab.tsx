import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Plus, Trash2, Edit3, X, Check, RotateCcw, FolderOpen,
  Video, Music, FileText, Archive, Package, Image, Code, Download, Type, File
} from 'lucide-react';

interface FileCategory {
  id: string;
  name: string;
  icon: string;
  color: string;
  extensions: string[];
  subdirectory: string | null;
  auto_move: boolean;
  priority: number;
  builtin: boolean;
  enabled: boolean;
}

interface CategoryStats {
  category_id: string;
  category_name: string;
  icon: string;
  color: string;
  file_count: number;
  total_size: number;
}

const iconMap: Record<string, React.ReactNode> = {
  video: <Video size={14} />,
  music: <Music size={14} />,
  'file-text': <FileText size={14} />,
  archive: <Archive size={14} />,
  package: <Package size={14} />,
  image: <Image size={14} />,
  code: <Code size={14} />,
  download: <Download size={14} />,
  type: <Type size={14} />,
  file: <File size={14} />,
};

const formatSize = (bytes: number) => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

const emptyCategory = (): FileCategory => ({
  id: crypto.randomUUID(),
  name: '',
  icon: 'file',
  color: '#636E72',
  extensions: [],
  subdirectory: null,
  auto_move: false,
  priority: 0,
  builtin: false,
  enabled: true,
});

export const FileCategoriesTab: React.FC = () => {
  const [categories, setCategories] = useState<FileCategory[]>([]);
  const [stats, setStats] = useState<CategoryStats[]>([]);
  const [editingCat, setEditingCat] = useState<FileCategory | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [testFile, setTestFile] = useState('');
  const [testResult, setTestResult] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const [cats, st] = await Promise.all([
        invoke<FileCategory[]>('list_file_categories'),
        invoke<CategoryStats[]>('get_file_category_stats', { downloadDir: '' }).catch(() => []),
      ]);
      setCategories(cats);
      setStats(st);
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  const handleSave = async (cat: FileCategory) => {
    try {
      if (isCreating) {
        await invoke('add_file_category', { category: cat });
      } else {
        await invoke('update_file_category', { category: cat });
      }
      setEditingCat(null);
      setIsCreating(false);
      loadData();
    } catch { /* ignore */ }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm('Delete this category?')) return;
    try {
      await invoke('delete_file_category', { id });
      loadData();
    } catch { /* ignore */ }
  };

  const handleReset = async () => {
    if (!window.confirm('Reset all categories to defaults? Custom categories will be lost.')) return;
    try {
      await invoke('reset_file_categories');
      loadData();
    } catch { /* ignore */ }
  };

  const handleToggle = async (cat: FileCategory) => {
    try {
      await invoke('update_file_category', { category: { ...cat, enabled: !cat.enabled } });
      loadData();
    } catch { /* ignore */ }
  };

  const handleTest = async () => {
    if (!testFile) return;
    try {
      const result = await invoke<{ category_name: string; icon: string; color: string; should_move: boolean; target_dir: string | null }>('categorize_file', { filename: testFile });
      setTestResult(`Category: ${result.category_name}${result.should_move ? ` → ${result.target_dir}` : ''}`);
    } catch (err) { setTestResult(`Error: ${err}`); }
  };

  const statsMap = Object.fromEntries(stats.map(s => [s.category_id, s]));

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-bold text-white">File Categories</h3>
          <p className="text-xs text-slate-500 mt-0.5">
            Auto-organize downloads into categorized directories by file type
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={handleReset}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-amber-400 bg-amber-500/10 border border-amber-500/20 hover:bg-amber-500/20 transition-colors"
          >
            <RotateCcw size={12} className="inline mr-1" /> Reset Defaults
          </button>
          <button
            onClick={() => { setEditingCat(emptyCategory()); setIsCreating(true); }}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-cyan-400 bg-cyan-500/10 border border-cyan-500/20 hover:bg-cyan-500/20 transition-colors"
          >
            <Plus size={12} className="inline mr-1" /> Add Category
          </button>
        </div>
      </div>

      {/* Test Filename */}
      <div className="p-3 rounded-lg bg-slate-900/50 border border-slate-700/30 space-y-2">
        <label className="text-xs text-slate-400 font-medium">Test filename categorization</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={testFile}
            onChange={e => setTestFile(e.target.value)}
            placeholder="myfile.mp4"
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

      {/* Category Editor */}
      <AnimatePresence>
        {editingCat && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="p-4 rounded-xl bg-slate-900/80 border border-cyan-500/20 space-y-4"
          >
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-bold text-white">{isCreating ? 'New Category' : 'Edit Category'}</h4>
              <button onClick={() => { setEditingCat(null); setIsCreating(false); }} className="text-slate-400 hover:text-white"><X size={16} /></button>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Name</label>
                <input value={editingCat.name} onChange={e => setEditingCat({ ...editingCat, name: e.target.value })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50" placeholder="My Category" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Color</label>
                <div className="flex gap-2 mt-1">
                  <input type="color" value={editingCat.color} onChange={e => setEditingCat({ ...editingCat, color: e.target.value })} className="w-8 h-8 rounded border border-slate-700 cursor-pointer bg-transparent" />
                  <input value={editingCat.color} onChange={e => setEditingCat({ ...editingCat, color: e.target.value })} className="flex-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono" />
                </div>
              </div>
              <div className="col-span-2">
                <label className="text-[10px] text-slate-500 font-medium">Extensions (comma-separated, no dots)</label>
                <input value={editingCat.extensions.join(', ')} onChange={e => setEditingCat({ ...editingCat, extensions: e.target.value.split(',').map(s => s.trim().toLowerCase()).filter(Boolean) })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50 font-mono" placeholder="mp4, mkv, avi" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Subdirectory</label>
                <input value={editingCat.subdirectory ?? ''} onChange={e => setEditingCat({ ...editingCat, subdirectory: e.target.value || null })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50" placeholder="Videos" />
              </div>
              <div>
                <label className="text-[10px] text-slate-500 font-medium">Priority</label>
                <input type="number" value={editingCat.priority} onChange={e => setEditingCat({ ...editingCat, priority: parseInt(e.target.value) || 0 })} className="w-full mt-1 px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-xs text-slate-200 focus:outline-none focus:border-cyan-500/50" />
              </div>
            </div>
            <div className="flex gap-4 text-xs">
              <label className="flex items-center gap-2 text-slate-400">
                <input type="checkbox" checked={editingCat.auto_move} onChange={e => setEditingCat({ ...editingCat, auto_move: e.target.checked })} className="rounded" />
                Auto-move to subdirectory
              </label>
              <label className="flex items-center gap-2 text-slate-400">
                <input type="checkbox" checked={editingCat.enabled} onChange={e => setEditingCat({ ...editingCat, enabled: e.target.checked })} className="rounded" />
                Enabled
              </label>
            </div>
            <div className="flex justify-end gap-2">
              <button onClick={() => { setEditingCat(null); setIsCreating(false); }} className="px-4 py-2 rounded-lg text-xs text-slate-400 hover:text-white transition-colors">Cancel</button>
              <button
                onClick={() => handleSave(editingCat)}
                disabled={!editingCat.name || editingCat.extensions.length === 0}
                className="px-4 py-2 rounded-lg text-xs font-medium text-white bg-cyan-600 hover:bg-cyan-500 transition-colors disabled:opacity-40"
              >
                <Check size={12} className="inline mr-1" /> {isCreating ? 'Create' : 'Save'}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Categories Grid */}
      <div className="grid grid-cols-1 gap-2">
        {categories.map(cat => {
          const s = statsMap[cat.id];
          return (
            <div
              key={cat.id}
              className={`flex items-center gap-3 p-3 rounded-lg border transition-colors ${cat.enabled ? 'bg-slate-900/50 border-slate-700/30' : 'bg-slate-900/20 border-slate-800/30 opacity-50'}`}
            >
              <button
                onClick={() => handleToggle(cat)}
                className={`w-8 h-4 rounded-full transition-colors relative flex-shrink-0 ${cat.enabled ? 'bg-cyan-500' : 'bg-slate-700'}`}
              >
                <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-white transition-all ${cat.enabled ? 'left-4' : 'left-0.5'}`} />
              </button>
              <div className="flex-shrink-0" style={{ color: cat.color }}>
                {iconMap[cat.icon] || <File size={14} />}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-slate-200">{cat.name}</span>
                  {cat.builtin && <span className="text-[9px] px-1 py-0.5 rounded bg-slate-700/50 text-slate-500">built-in</span>}
                  {cat.auto_move && cat.subdirectory && (
                    <span className="text-[9px] px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 flex items-center gap-1">
                      <FolderOpen size={8} /> {cat.subdirectory}
                    </span>
                  )}
                </div>
                <div className="text-[10px] text-slate-500 font-mono truncate mt-0.5">
                  {cat.extensions.slice(0, 12).join(', ')}{cat.extensions.length > 12 ? ` +${cat.extensions.length - 12}` : ''}
                </div>
              </div>
              {s && (
                <div className="text-right flex-shrink-0">
                  <div className="text-xs font-medium text-slate-300">{s.file_count} files</div>
                  <div className="text-[10px] text-slate-500">{formatSize(s.total_size)}</div>
                </div>
              )}
              <div className="flex gap-1 flex-shrink-0">
                <button
                  onClick={() => { setEditingCat({ ...cat }); setIsCreating(false); }}
                  className="p-1 text-slate-500 hover:text-cyan-400 transition-colors"
                >
                  <Edit3 size={12} />
                </button>
                {!cat.builtin && (
                  <button
                    onClick={() => handleDelete(cat.id)}
                    className="p-1 text-slate-500 hover:text-red-400 transition-colors"
                  >
                    <Trash2 size={12} />
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
