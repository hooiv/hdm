import React, { useState, useEffect, useRef, useCallback, Suspense } from "react";
import { AnimatePresence } from "framer-motion";
import { safeInvoke as invoke, safeListen as listen, safeGetWindowByLabel } from "./utils/tauri";
import { debug, error as logError } from "./utils/logger";
import "./App.css";
import { Layout } from "./components/Layout";
import { DownloadList } from "./components/DownloadList";
import { ClipboardToast } from "./components/ClipboardToast";
import { DropTarget } from "./components/DropTarget";
import { ToastManager, ToastRef } from "./components/ToastManager";
import { TorrentList } from "./components/TorrentList";
import { FeedsTab } from "./components/FeedsTab";
import { SearchTab } from "./components/SearchTab";
import type { DownloadProgressPayload, ClipboardUrlPayload, ExtensionDownloadPayload, BatchLink, ScheduledDownloadPayload, SavedDownload, AppSettings, DownloadTask } from "./types";
import { toTaskStatus } from "./types";

// Lazy load modals to improve initial render time
const AddDownloadModal = React.lazy(() => import("./components/AddDownloadModal").then(m => ({ default: m.AddDownloadModal })));
const SettingsPage = React.lazy(() => import("./components/SettingsPage").then(m => ({ default: m.SettingsPage })));
const BatchDownloadModal = React.lazy(() => import("./components/BatchDownloadModal").then(m => ({ default: m.BatchDownloadModal })));
const ScheduleModal = React.lazy(() => import("./components/ScheduleModal").then(m => ({ default: m.ScheduleModal })));
const SpiderModal = React.lazy(() => import("./components/SpiderModal").then(m => ({ default: m.SpiderModal })));
const AddTorrentModal = React.lazy(() => import("./components/AddTorrentModal").then(m => ({ default: m.AddTorrentModal })));
const PluginEditor = React.lazy(() => import("./components/PluginEditor"));

import { GlobalTelemetry } from './components/GlobalTelemetry';

// Generate unique ID for downloads
let nextId = 1;
const generateId = () => {
  return `dl_${Date.now()}_${nextId++}`;
};

interface ClipboardData {
  url: string;
  filename: string;
}

function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isScheduleOpen, setIsScheduleOpen] = useState(false);
  const [isSpiderOpen, setIsSpiderOpen] = useState(false);
  const [isTorrentModalOpen, setIsTorrentModalOpen] = useState(false);
  const [clipboardData, setClipboardData] = useState<ClipboardData | null>(null);
  const [batchLinks, setBatchLinks] = useState<BatchLink[]>([]);
  const [droppedUrl, setDroppedUrl] = useState<string | undefined>(undefined);
  const [activeTab, setActiveTab] = useState<'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins'>('downloads');
  const [downloadDir, setDownloadDir] = useState<string>('');

  const [, setIsOverlayVisible] = useState(false);
  const isOverlayVisibleRef = useRef(false);

  const toggleOverlay = useCallback(async () => {
    // toggling overlay visibility using the statically imported Window API
    const overlay = await safeGetWindowByLabel("overlay");
    if (overlay) {
      if (isOverlayVisibleRef.current) {
        await overlay.hide();
      } else {
        await overlay.show();
      }
      isOverlayVisibleRef.current = !isOverlayVisibleRef.current;
      setIsOverlayVisible(isOverlayVisibleRef.current);
    }
  }, []);

  // keyboard shortcut: Ctrl+Shift+O toggles overlay
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        toggleOverlay();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [toggleOverlay]);

  // Use useRef for mutable state that doesn't trigger re-renders
  const lastUpdate = useRef<Map<string, { time: number, bytes: number, speed: number }>>(new Map());
  const toastRef = useRef<ToastRef>(null);
  // Track completed IDs to avoid duplicate toasts
  const completedIds = useRef<Set<string>>(new Set());
  // Track auto-remove timers so they can be cleaned on unmount
  const autoRemoveTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  // Stable ref for startDownload to avoid stale closures in event listeners
  const startDownloadRef = useRef<(url: string, filename: string, force?: boolean, customHeaders?: Record<string,string>) => Promise<void>>(null!);

  useEffect(() => {
    const unlistenPromise = listen<DownloadProgressPayload>('download_progress', (event) => {
      const { id, downloaded, total } = event.payload;

      setTasks(prevTasks => {
        // detect new task arrival
        const exists = prevTasks.some(t => t.id === id);

        // Unpack segments from tuple format (shared by both new & existing paths)
        const segments = event.payload.segments ? event.payload.segments.map((s) => ({
          id: s[0],
          start_byte: s[1],
          end_byte: s[2],
          downloaded_cursor: s[3],
          state: ['Idle', 'Downloading', 'Paused', 'Complete', 'Error'][s[4]] || 'Idle',
          speed_bps: s[5]
        })) : [];

        if (!exists) {
          // Progress for unknown task — create a placeholder and start tracking it
          safeGetWindowByLabel("overlay").then(o => o?.show());
          isOverlayVisibleRef.current = true;
          setIsOverlayVisible(true);

          const now = Date.now();
          lastUpdate.current.set(id, { time: now, bytes: downloaded, speed: 0 });

          // Fetch real filename/URL from backend so the task is resumable
          invoke<SavedDownload[]>('get_downloads').then(data => {
            const match = data.find(d => d.id === id);
            if (match) {
              setTasks(curr => curr.map(t =>
                t.id === id && !t.url
                  ? { ...t, filename: match.filename, url: match.url }
                  : t
              ));
            }
          }).catch(() => {});

          const newTask: DownloadTask = {
            id,
            filename: 'Downloading...',
            url: '',
            progress: total > 0 ? Math.min((downloaded / total) * 100, 100) : 0,
            downloaded,
            total,
            speed: 0,
            status: total > 0 && downloaded >= total ? 'Done' : 'Downloading',
            segments,
          };
          return [...prevTasks, newTask];
        }

        return prevTasks.map(task => {
          if (task.id === id) {
            const now = Date.now();
            const last = lastUpdate.current.get(id);
            let speed = last?.speed || 0;

            if (last) {
              const timeDiff = (now - last.time) / 1000;
              const bytesDiff = downloaded - last.bytes;

              if (bytesDiff < 0) {
                lastUpdate.current.set(id, { time: now, bytes: downloaded, speed: 0 });
                speed = 0;
              } else if (timeDiff >= 0.3 && bytesDiff > 0) {
                speed = bytesDiff / timeDiff;
                lastUpdate.current.set(id, { time: now, bytes: downloaded, speed });
              }
            } else {
              lastUpdate.current.set(id, { time: now, bytes: downloaded, speed: 0 });
            }

            const newTask: DownloadTask = {
              ...task,
              progress: total > 0 ? Math.min((downloaded / total) * 100, 100) : 0,
              downloaded,
              total,
              speed,
              status: total > 0 && downloaded >= total ? 'Done' : 'Downloading',
              segments
            };

            if (newTask.status === 'Done' && !completedIds.current.has(id)) {
              completedIds.current.add(id);
              toastRef.current?.addToast(`Download Complete: ${task.filename}`, 'success');
              // auto-remove after 30 seconds to keep overlay/queue tidy
              const timer = setTimeout(() => {
                invoke("remove_download_entry", { id }).catch(() => {});
                setTasks(curr => curr.filter(t => t.id !== id));
                lastUpdate.current.delete(id);
                completedIds.current.delete(id);
                autoRemoveTimers.current.delete(id);
              }, 30000);
              autoRemoveTimers.current.set(id, timer);
            }

            return newTask;
          }
          return task;
        });
      });
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
      // Clear all pending auto-remove timers
      for (const timer of autoRemoveTimers.current.values()) {
        clearTimeout(timer);
      }
      autoRemoveTimers.current.clear();
    };
  }, []);

  // Load settings + saved downloads on app start
  useEffect(() => {
    const loadInitialData = async () => {
      try {
        // Load download directory from settings
        const settings = await invoke<AppSettings>('get_settings');
        const dir = settings?.download_dir || 'Downloads';
        setDownloadDir(dir);
      } catch (e) {
        logError('Failed to load settings:', e);
        toastRef.current?.addToast('Failed to load settings', 'error');
        setDownloadDir('Downloads');
      }
      try {
        const saved = await invoke<SavedDownload[]>('get_downloads');
        if (saved.length > 0) {
          const loadedTasks: DownloadTask[] = saved.map(d => ({
            id: d.id,
            filename: d.filename,
            url: d.url,
            progress: d.total_size > 0 ? (d.downloaded_bytes / d.total_size) * 100 : 0,
            downloaded: d.downloaded_bytes,
            total: d.total_size,
            speed: 0,
            status: toTaskStatus(d.status)
          }));
          setTasks(loadedTasks);
        }
      } catch (error) {
        logError('Failed to load saved downloads:', error);
        toastRef.current?.addToast('Failed to load saved downloads', 'error');
      }
    };
    loadInitialData();
  }, []);

  // Listen for downloads from browser extension
  useEffect(() => {
    const unlistenPromise = listen<ExtensionDownloadPayload>('extension_download', (event) => {
      const { url, filename } = event.payload;
      debug('Extension download received:', url, filename);
      const extractedFilename = filename || url.split('/').pop()?.split('?')[0] || 'download';
      startDownloadRef.current(url, extractedFilename);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for clipboard URLs
  useEffect(() => {
    let dismissTimer: ReturnType<typeof setTimeout> | null = null;
    const unlistenPromise = listen<ClipboardUrlPayload>('clipboard_url', (event) => {
      const { url, filename } = event.payload;
      debug('Clipboard URL detected:', url, filename);
      setClipboardData({ url, filename });

      // Clear previous timer to avoid stale dismissals
      if (dismissTimer) clearTimeout(dismissTimer);
      // Auto-dismiss after 10 seconds
      dismissTimer = setTimeout(() => {
        setClipboardData(prev => prev?.url === url ? null : prev);
      }, 10000);
    });

    return () => {
      if (dismissTimer) clearTimeout(dismissTimer);
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for batch links from browser extension
  useEffect(() => {
    const unlistenPromise = listen<BatchLink[]>('batch_links', (event) => {
      const links = event.payload;
      debug('Batch links received:', links.length);
      setBatchLinks(links);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for scheduled downloads starting
  useEffect(() => {
    const unlistenPromise = listen<ScheduledDownloadPayload>('scheduled_download_start', (event) => {
      const { url, filename } = event.payload;
      debug('Scheduled download starting:', url, filename);
      startDownloadRef.current(url, filename);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for URL refresh events (update task URL when address is hot-swapped)
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; url: string }>('download_refreshed', (event) => {
      const { id, url } = event.payload;
      debug('Download URL refreshed:', id, url);
      setTasks(prev => prev.map(t => {
        if (t.id !== id) return t;
        // Only reset Error status to Paused; leave Done/Downloading/Paused as-is
        const newStatus = t.status === 'Error' ? 'Paused' as const : t.status;
        return { ...t, url, status: newStatus };
      }));
      toastRef.current?.addToast('Download address refreshed — click Resume to retry', 'success');
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  /* ------------------ Memoized Handlers ------------------ */

  /** Sanitize a filename to prevent path traversal and Windows reserved name issues. */
  const sanitizeFilename = (name: string): string => {
    // Strip directory separators and parent-directory traversal
    let safe = name.replace(/[/\\]/g, '_').replace(/\.\./g, '_');
    // Remove leading dots (hidden files on unix) and control characters
    safe = safe.replace(/^\.+/, '').replace(/[\x00-\x1f\x7f]/g, '');
    // Remove Windows illegal filename characters
    safe = safe.replace(/[<>:"|?*]/g, '_');
    // Strip trailing dots and spaces (Windows silently drops them, causing mismatches)
    safe = safe.replace(/[\s.]+$/, '');
    // Block Windows reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    const reserved = /^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(\.|$)/i;
    if (reserved.test(safe)) {
      safe = '_' + safe;
    }
    // Fallback if nothing remains
    return safe.trim() || 'download';
  };

  const startDownload = async (url: string, filename: string, force: boolean = false, customHeaders?: Record<string, string>) => {
    const safeFilename = sanitizeFilename(filename);
    const downloadId = generateId();

    const newTask: DownloadTask = {
      id: downloadId,
      filename: safeFilename,
      url,
      progress: 0,
      downloaded: 0,
      total: 0,
      speed: 0,
      status: 'Downloading'
    };

    lastUpdate.current.delete(downloadId);

    // Add to existing tasks instead of replacing
    setTasks(prev => [...prev, newTask]);

    try {
      await invoke("start_download", {
        id: downloadId,
        url,
        path: `${downloadDir}/${safeFilename}`,
        force,
        customHeaders: customHeaders || null
      });
    } catch (error) {
      logError(error);
      toastRef.current?.addToast(`Failed to start download: ${error}`, 'error');
      setTasks(prev => prev.map(t => t.id === downloadId ? { ...t, status: 'Error' } : t));
    }
  };

  // Keep startDownloadRef in sync so event listeners always have latest closure
  startDownloadRef.current = startDownload;

  // Stable ref for tasks to avoid dependency cycles in memoized callbacks
  const tasksRef = useRef(tasks);
  useEffect(() => { tasksRef.current = tasks; }, [tasks]);

  // Stable ref for downloadDir to avoid stale closures in memoized callbacks
  const downloadDirRef = useRef(downloadDir);
  useEffect(() => { downloadDirRef.current = downloadDir; }, [downloadDir]);

  const pauseDownloadMemo = React.useCallback(async (id: string) => {
    const task = tasksRef.current.find(t => t.id === id);
    if (!task) return;

    try {
      await invoke("pause_download", {
        id
      });
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Paused', speed: 0 } : t));
    } catch (error) {
      logError("Failed to pause:", error);
      toastRef.current?.addToast('Failed to pause download', 'error');
    }
  }, []); // Stable!

  const resumeDownloadMemo = React.useCallback(async (id: string) => {
    const task = tasksRef.current.find(t => t.id === id);
    if (task) {
      if (!task.url) {
        toastRef.current?.addToast('Cannot resume: download has no URL', 'error');
        return;
      }
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Downloading' } : t));
      try {
        await invoke("start_download", {
          id: task.id,
          url: task.url,
          path: `${downloadDirRef.current}/${task.filename}`
        });
      } catch (error) {
        logError("Failed to resume:", error);
        toastRef.current?.addToast('Failed to resume download', 'error');
        setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Error' } : t));
      }
    }
  }, []); // Stable

  const deleteDownloadMemo = React.useCallback(async (id: string) => {
    try {
      await invoke("remove_download_entry", { id });
      setTasks(prev => prev.filter(t => t.id !== id));
      lastUpdate.current.delete(id);
      completedIds.current.delete(id);
      // Cancel any pending auto-remove timer for this download
      const timer = autoRemoveTimers.current.get(id);
      if (timer) { clearTimeout(timer); autoRemoveTimers.current.delete(id); }
    } catch (error) {
      logError("Failed to delete:", error);
      toastRef.current?.addToast('Failed to delete download', 'error');
    }
  }, []); // Stable

  const moveTaskMemo = React.useCallback(async (id: string, direction: 'up' | 'down') => {
    // Optimistic update
    setTasks(prev => {
      const index = prev.findIndex(t => t.id === id);
      if (index === -1) return prev;

      const newTasks = [...prev];
      if (direction === 'up' && index > 0) {
        [newTasks[index], newTasks[index - 1]] = [newTasks[index - 1], newTasks[index]];
      } else if (direction === 'down' && index < newTasks.length - 1) {
        [newTasks[index], newTasks[index + 1]] = [newTasks[index + 1], newTasks[index]];
      }
      return newTasks;
    });

    try {
      await invoke("move_download_item", { id, direction });
    } catch (error) {
      logError("Failed to persist move:", error);
      toastRef.current?.addToast('Failed to move download', 'error');
    }
  }, []); // Stable

  const moveUpMemo = React.useCallback((id: string) => moveTaskMemo(id, 'up'), [moveTaskMemo]);
  const moveDownMemo = React.useCallback((id: string) => moveTaskMemo(id, 'down'), [moveTaskMemo]);

  const handleStream = async (id: number) => {
    try {
      const url = await invoke<string>('play_torrent', { id });
      debug("Streaming URL:", url);
      // Open the URL in default player/browser
      await invoke('open_file', { path: url });
    } catch (e) {
      logError("Stream failed", e);
      // toastRef might be null if not using conditional, but typically safe inside handler
      toastRef.current?.addToast("Stream error: " + e, 'error');
    }
  };

  // Calculate stats (memoized to avoid re-computation at 30fps progress updates)
  const stats = React.useMemo(() => ({
    total: tasks.length,
    downloading: tasks.filter(t => t.status === 'Downloading').length,
    completed: tasks.filter(t => t.status === 'Done').length,
    totalBytes: tasks.reduce((sum, t) => sum + t.downloaded, 0),
  }), [tasks]);

  const handleSpeedLimitChange = (limit: number) => {
    // Layout sends values in KB/s (512, 1024, 5120, 10240), backend expects KB/s
    invoke("set_speed_limit", { limitKbps: limit }).catch((err) => {
      logError("Failed to set speed limit:", err);
      toastRef.current?.addToast("Failed to set speed limit", "error");
    });
  };

  const pauseAll = () => {
    const toPause = tasks.filter(t => t.status === 'Downloading');
    if (toPause.length === 0) return;
    setTasks(prev => prev.map(x =>
      toPause.some(p => p.id === x.id) ? { ...x, status: 'Paused' as const, speed: 0 } : x
    ));
    toPause.forEach(t => {
      invoke('pause_download', { id: t.id }).catch((err) => logError('Failed to pause:', t.id, err));
    });
  };

  const resumeAll = () => {
    const toResume = tasks.filter(t => (t.status === 'Paused' || t.status === 'Error') && t.url);
    if (toResume.length === 0) return;
    setTasks(prev => prev.map(x =>
      toResume.some(r => r.id === x.id) ? { ...x, status: 'Downloading' as const } : x
    ));
    toResume.forEach(t => {
      invoke('start_download', {
        id: t.id,
        url: t.url,
        path: `${downloadDirRef.current}/${t.filename}`
      }).catch((err) => {
        logError('Failed to resume:', t.id, err);
        setTasks(prev => prev.map(x => x.id === t.id ? { ...x, status: 'Error' as const } : x));
      });
    });
  };

  const handleClipboardDownload = (url: string, filename: string) => {
    startDownload(url, filename);
    setClipboardData(null);
  };

  const globalSpeed = React.useMemo(() => tasks
    .filter(d => d.status === 'Downloading')
    .reduce((acc, curr) => acc + (curr.speed || 0), 0), [tasks]);

  return (
    <>
      <Layout
        onAddClick={async (e) => {
          if (e.shiftKey) {
            try {
              const text = await navigator.clipboard.readText();
              if (text && (text.startsWith('http') || text.startsWith('magnet:'))) {
                debug("Force Download Key detected. Adding:", text);
                if (text.startsWith('magnet:')) {
                  invoke("add_magnet_link", { magnet: text }).catch((err) => {
                    logError("Magnet link failed:", err);
                    toastRef.current?.addToast(`Failed to add magnet link: ${err}`, 'error');
                  });
                } else {
                  const filename = text.split('/').pop()?.split('?')[0] || 'clipboard_download';
                  startDownload(text, filename);
                }
                toastRef.current?.addToast(`Started Force Download: ${text}`, 'success');
              } else {
                toastRef.current?.addToast(`Clipboard does not contain a valid URL`, 'error');
              }
            } catch (err) {
              logError("Failed to read clipboard:", err);
              toastRef.current?.addToast(`Failed to read clipboard`, 'error');
            }
          } else {
            setIsModalOpen(true);
          }
        }}
        onAddTorrentClick={() => setIsTorrentModalOpen(true)}
        onScheduleClick={() => setIsScheduleOpen(true)}
        onSpiderClick={() => setIsSpiderOpen(true)}
        onSettingsClick={() => setIsSettingsOpen(true)}
        onOverlayClick={toggleOverlay}
        stats={stats}
        onSpeedLimitChange={handleSpeedLimitChange}
        activeTab={activeTab}
        onTabChange={setActiveTab}
        globalSpeed={globalSpeed}
      >
        {activeTab === 'downloads' ? (
          <div className="flex flex-col h-full">
            <GlobalTelemetry tasks={tasks} />
            {tasks.length > 0 && (
              <div className="px-4 pb-2 flex flex-col sm:flex-row sm:items-center sm:gap-4">
                {stats.completed > 0 && (
                  <button
                    onClick={() => {
                      const completedTasks = tasks.filter(t => t.status === 'Done');
                      completedTasks.forEach(t => {
                        invoke("remove_download_entry", { id: t.id }).catch(() => {});
                        lastUpdate.current.delete(t.id);
                        completedIds.current.delete(t.id);
                        const timer = autoRemoveTimers.current.get(t.id);
                        if (timer) { clearTimeout(timer); autoRemoveTimers.current.delete(t.id); }
                      });
                      setTasks(prev => prev.filter(t => t.status !== 'Done'));
                    }}
                    className="text-xs text-red-400 hover:text-red-200 underline"
                  >
                    Clear completed ({stats.completed})
                  </button>
                )}
                <div className="flex gap-2 mt-2 sm:mt-0">
                  <button
                    onClick={pauseAll}
                    disabled={stats.downloading === 0}
                    className="text-xs px-2 py-1 bg-amber-600 hover:bg-amber-500 disabled:opacity-40 disabled:cursor-not-allowed rounded text-white transition-colors"
                  >Pause All</button>
                  <button
                    onClick={resumeAll}
                    disabled={tasks.filter(t => t.status === 'Paused' || t.status === 'Error').length === 0}
                    className="text-xs px-2 py-1 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed rounded text-white transition-colors"
                  >Resume All</button>
                </div>
              </div>
            )}
            {tasks.length > 0 ? (
              <DownloadList
                tasks={tasks}
                onPause={pauseDownloadMemo}
                onResume={resumeDownloadMemo}
                onDelete={deleteDownloadMemo}
                onMoveUp={moveUpMemo}
                onMoveDown={moveDownMemo}
                downloadDir={downloadDir}
              />
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center text-slate-500 opacity-60">
                <div className="w-24 h-24 mb-6 rounded-full bg-white/5 flex items-center justify-center shadow-inner">
                  <span className="text-4xl">📥</span>
                </div>
                <h3 className="text-lg font-semibold text-slate-300">No Active Downloads</h3>
                <p className="text-sm">Add a URL or use the browser extension to start.</p>
              </div>
            )}
          </div>
        ) : activeTab === 'torrents' ? (
          <TorrentList onPlay={handleStream} />
        ) : activeTab === 'feeds' ? (
          <FeedsTab />
        ) : activeTab === 'plugins' ? (
          <Suspense fallback={<div className="flex-1 flex items-center justify-center text-slate-500">Loading plugins...</div>}>
            <PluginEditor />
          </Suspense>
        ) : (
          <SearchTab onStartDownload={startDownload} />
        )}
      </Layout>
      <Suspense fallback={null}>
        {isModalOpen && (
          <AddDownloadModal
            isOpen={isModalOpen}
            onClose={() => { setIsModalOpen(false); setDroppedUrl(undefined); }}
            onStart={startDownload}
            initialUrl={droppedUrl}
          />
        )}
        {isTorrentModalOpen && (
          <AddTorrentModal
            isOpen={isTorrentModalOpen}
            onClose={() => setIsTorrentModalOpen(false)}
            onAdd={(magnet) => {
              debug("Adding magnet:", magnet);
              invoke("add_magnet_link", { magnet }).catch((err) => {
                logError("Magnet link failed:", err);
                toastRef.current?.addToast(`Failed to add magnet: ${err}`, 'error');
              });
            }}
          />
        )}
        {isSettingsOpen && (
          <SettingsPage isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
        )}
        {batchLinks.length > 0 && (
          <BatchDownloadModal
            isOpen={true}
            links={batchLinks}
            onClose={() => setBatchLinks([])}
            onDownload={(links) => {
              links.forEach(link => startDownload(link.url, link.filename));
              setBatchLinks([]);
            }}
          />
        )}
        {isScheduleOpen && (
          <ScheduleModal
            isOpen={isScheduleOpen}
            onClose={() => setIsScheduleOpen(false)}
          />
        )}
        {isSpiderOpen && (
          <SpiderModal
            isOpen={isSpiderOpen}
            onClose={() => setIsSpiderOpen(false)}
            onDownload={(files) => {
              files.forEach(f => startDownload(f.url, f.filename));
              setIsSpiderOpen(false);
            }}
          />
        )}
      </Suspense>

      <AnimatePresence>
        {clipboardData && (
          <ClipboardToast
            message="URL detected in clipboard"
            filename={clipboardData.filename}
            onDownload={() => handleClipboardDownload(clipboardData.url, clipboardData.filename)}
            onDismiss={() => setClipboardData(null)}
          />
        )}
      </AnimatePresence>
      <DropTarget onDrop={React.useCallback((url: string) => {
        setDroppedUrl(url);
        setIsModalOpen(true);
      }, [])} />
      <ToastManager ref={toastRef} />
    </>
  );
}

export default App;
