import React, { useState, useEffect, useRef } from "react"; // Added React import
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../contexts/ToastContext";
import { error as logError } from '../utils/logger';
import { Save, Plus, Trash2, FileCode, Puzzle, Zap } from "lucide-react";

interface PluginMetadata {
  name: string;
  version: string;
  domains: string[];
}

interface OutputLine {
  id: number;
  text: string;
}

const PluginEditor: React.FC = () => {
  const [plugins, setPlugins] = useState<PluginMetadata[]>([]);
  const toast = useToast();
  const [selectedPlugin, setSelectedPlugin] = useState<string | null>(null);
  const [code, setCode] = useState<string>("");
  const [isDirty, setIsDirty] = useState(false);
  const [, setIsLoading] = useState(false);
  const MAX_OUTPUT_LINES = 200;
  const [output, setOutput] = useState<OutputLine[]>([]);
  const outputIdRef = useRef(0);

  useEffect(() => {
    loadPlugins();

    // Listen for plugin toasts or logs
    // (Assuming we have event listeners from tauri, but skipping for MVP simplicity)
  }, []);

  const loadPlugins = async () => {
    try {
      const list = await invoke<PluginMetadata[]>("get_all_plugins");
      setPlugins(list);
    } catch (e) {
      logError(e);
      toast.error('Failed to load plugins');
    }
  };

  const sanitizeName = (name: string) => name.replace(/[^a-zA-Z0-9_-]/g, "");

  const handleSelectPlugin = async (name: string) => {
    if (isDirty && selectedPlugin) {
      if (!confirm("Unsaved changes. Discard?")) return;
    }

    const safeName = sanitizeName(name);
    if (!safeName) return;

    try {
      setIsLoading(true);
      const content = await invoke<string>("get_plugin_source", {
        filename: safeName,
      });
      setCode(content);
      setSelectedPlugin(safeName);
      setIsDirty(false);
    } catch (e) {
      logError(e);
      toast.error("Failed to load plugin source: " + e);
    } finally {
      setIsLoading(false);
    }
  };

  const handleSave = async () => {
    if (!selectedPlugin) return;
    try {
      await invoke("save_plugin_source", {
        filename: selectedPlugin,
        content: code,
      });
      setIsDirty(false);
      // Better UI than alert
      setOutput((prev) => [
        { id: ++outputIdRef.current, text: `[${new Date().toLocaleTimeString()}] Saved ${selectedPlugin}.lua` },
        ...prev,
      ].slice(0, MAX_OUTPUT_LINES));
    } catch (e) {
      toast.error("Save failed: " + e);
    }
  };

  const handleCreate = async () => {
    const name = prompt("Enter plugin name (no spaces):");
    if (!name) return;

    // Simple validation
    const safeName = name.replace(/[^a-zA-Z0-9_-]/g, "");

    try {
      await invoke("save_plugin_source", {
        filename: safeName,
        content: `-- Plugin: ${safeName}\n-- Version: 1.0\n-- Domains: example.com\n\nplugin = {\n    name = "${safeName}",\n    version = "1.0"\n}\n\nfunction extract_stream(url)\n    -- extraction logic\nend`,
      });
      await loadPlugins();
      handleSelectPlugin(safeName);
    } catch (e) {
      toast.error("Create failed: " + e);
    }
  };

  const handleDelete = async (name: string) => {
    const safeName = sanitizeName(name);
    if (!safeName) return;
    if (!confirm(`Delete plugin ${safeName}?`)) return;
    try {
      await invoke("delete_plugin", { filename: safeName });
      if (selectedPlugin === safeName) {
        setSelectedPlugin(null);
        setCode("");
      }
      await loadPlugins();
    } catch (e) {
      toast.error("Delete failed: " + e);
    }
  };

  return (
    <div className="h-full flex overflow-hidden glass-panel rounded-2xl mx-4 my-2 border border-white/10 shadow-2xl backdrop-blur-xl bg-slate-900/60">
      {/* Sidebar */}
      <div className="w-64 border-r border-white/5 flex flex-col bg-slate-900/40">
        <div className="p-4 border-b border-white/5 flex justify-between items-center">
          <h2 className="font-bold text-cyan-400 flex items-center gap-2">
            <Puzzle size={18} /> Plugins
          </h2>
          <button
            onClick={handleCreate}
            className="p-1 hover:bg-white/10 rounded-full transition-colors text-emerald-400"
          >
            <Plus size={18} />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1">
          {plugins.map((p) => (
            <div
              key={p.name}
              onClick={() => handleSelectPlugin(p.name)}
              className={`group p-3 rounded-xl cursor-pointer transition-all border ${
                selectedPlugin === p.name
                  ? "bg-cyan-500/10 border-cyan-500/30 text-cyan-300 shadow-lg shadow-cyan-900/20"
                  : "hover:bg-white/5 border-transparent text-slate-400 hover:text-slate-200"
              }`}
            >
              <div className="flex justify-between items-start">
                <div className="flex items-center gap-2">
                  <FileCode
                    size={16}
                    className={
                      selectedPlugin === p.name
                        ? "text-cyan-400"
                        : "text-slate-500"
                    }
                  />
                  <span className="font-medium text-sm">{p.name}</span>
                </div>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDelete(p.name);
                  }}
                  className="opacity-0 group-hover:opacity-100 p-1 hover:text-red-400 text-slate-600 transition-opacity"
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="text-xs text-slate-500 mt-1 pl-6">
                v{p.version}
              </div>
            </div>
          ))}

          {plugins.length === 0 && (
            <div className="text-center p-8 text-slate-600 italic">
              No plugins found
            </div>
          )}
        </div>
      </div>

      {/* Main Editor Area */}
      <div className="flex-1 flex flex-col relative bg-[#1E1E1E]">
        {/* Editor Header */}
        <div className="h-12 border-b border-white/5 flex items-center justify-between px-4 bg-[#252526]">
          <div className="flex items-center gap-4">
            {selectedPlugin ? (
              <>
                <span className="text-sm text-slate-300 font-mono">
                  {selectedPlugin}.lua
                </span>
                {isDirty && (
                  <span className="text-amber-400 text-xs bg-amber-400/10 px-2 py-0.5 rounded-full">
                    ● Unsaved
                  </span>
                )}
              </>
            ) : (
              <span className="text-slate-500 text-sm italic">
                Select a plugin to edit
              </span>
            )}
          </div>

          {selectedPlugin && (
            <div className="flex items-center gap-2">
              <button
                onClick={handleSave}
                disabled={!isDirty}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all ${
                  isDirty
                    ? "bg-cyan-500 text-white shadow-lg shadow-cyan-500/20 hover:brightness-110"
                    : "bg-white/5 text-slate-500 cursor-not-allowed"
                }`}
              >
                <Save size={14} /> Save
              </button>
            </div>
          )}
        </div>

        {/* Editor Body */}
        <div className="flex-1 relative">
          {selectedPlugin ? (
            <textarea
              value={code}
              onChange={(e) => {
                setCode(e.target.value);
                setIsDirty(true);
              }}
              className="w-full h-full bg-[#1E1E1E] text-slate-300 font-mono text-sm p-4 resize-none focus:outline-none custom-scrollbar leading-relaxed"
              spellCheck="false"
              autoComplete="off"
            />
          ) : (
            <div className="absolute inset-0 flex flex-col items-center justify-center text-slate-600">
              <Zap size={48} className="mb-4 text-slate-700" />
              <p>Select or create a plugin to start coding</p>
            </div>
          )}
        </div>

        {/* Log / Terminal Panel */}
        <div className="h-32 border-t border-white/5 bg-[#1E1E1E] flex flex-col">
          <div className="px-4 py-1 bg-[#252526] text-xs font-bold text-slate-500 uppercase tracking-wider flex justify-between">
            <span>Output</span>
            <button onClick={() => setOutput([])}>
              <Trash2 size={12} />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-2 font-mono text-xs space-y-1 custom-scrollbar">
            {output.map((line) => (
              <div
                key={line.id}
                className="text-slate-400 border-b border-white/5 last:border-0 pb-1"
              >
                {line.text}
              </div>
            ))}
            {output.length === 0 && (
              <div className="text-slate-700 italic px-2">No output</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default PluginEditor;
