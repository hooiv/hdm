import React, { useState, useEffect, useRef, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { Layout } from "./components/Layout";
import { DownloadList } from "./components/DownloadList";
import { DownloadTask } from "./components/DownloadItem";
import { ClipboardToast } from "./components/ClipboardToast";
import { DropTarget } from "./components/DropTarget";
import { ToastManager, ToastRef } from "./components/ToastManager";
import { TorrentList } from "./components/TorrentList";
import { FeedsTab } from "./components/FeedsTab";
import { SearchTab } from "./components/SearchTab";

// Lazy load modals to improve initial render time
const AddDownloadModal = React.lazy(() => import("./components/AddDownloadModal").then(m => ({ default: m.AddDownloadModal })));
const SettingsPage = React.lazy(() => import("./components/SettingsPage").then(m => ({ default: m.SettingsPage })));
const BatchDownloadModal = React.lazy(() => import("./components/BatchDownloadModal").then(m => ({ default: m.BatchDownloadModal })));
const ScheduleModal = React.lazy(() => import("./components/ScheduleModal").then(m => ({ default: m.ScheduleModal })));
const SpiderModal = React.lazy(() => import("./components/SpiderModal").then(m => ({ default: m.SpiderModal })));
const AddTorrentModal = React.lazy(() => import("./components/AddTorrentModal").then(m => ({ default: m.AddTorrentModal })));
const PluginEditor = React.lazy(() => import("./components/PluginEditor"));

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
  const [batchLinks, setBatchLinks] = useState<Array<{ url: string; filename: string }>>([]);
  const [activeTab, setActiveTab] = useState<'downloads' | 'torrents' | 'feeds' | 'search' | 'plugins'>('downloads');

  const [isOverlayVisible, setIsOverlayVisible] = useState(false);

  const toggleOverlay = async () => {
    // Lazy import to avoid issues in non-tauri env or initial load
    const { Window } = await import("@tauri-apps/api/window");
    const overlay = await Window.getByLabel("overlay");
    if (overlay) {
      if (isOverlayVisible) {
        await overlay.hide();
      } else {
        await overlay.show();
      }
      setIsOverlayVisible(!isOverlayVisible);
    }
  };

  // Use useRef for mutable state that doesn't trigger re-renders
  const lastUpdate = useRef<Map<string, { time: number, bytes: number, speed: number }>>(new Map());
  const toastRef = useRef<ToastRef>(null);
  // Track completed IDs to avoid duplicate toasts
  const completedIds = useRef<Set<string>>(new Set());

  useEffect(() => {
    const unlistenPromise = listen('download_progress', (event: any) => {
      const { id, downloaded, total } = event.payload;

      setTasks(prevTasks => {
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

            // Unpack segments from tuple format
            const segments = event.payload.segments ? event.payload.segments.map((s: any) => ({
              id: s[0],
              start_byte: s[1],
              end_byte: s[2],
              downloaded_cursor: s[3],
              state: ['Idle', 'Downloading', 'Paused', 'Complete', 'Error'][s[4]] || 'Idle',
              speed_bps: s[5]
            })) : [];

            const newTask: DownloadTask = {
              ...task,
              progress: Math.min((downloaded / total) * 100, 100),
              downloaded,
              total,
              speed,
              status: downloaded >= total ? 'Done' : 'Downloading',
              segments
            };

            if (newTask.status === 'Done' && !completedIds.current.has(id)) {
              completedIds.current.add(id);
              toastRef.current?.addToast(`Download Complete: ${task.filename}`, 'success');
            }

            return newTask;
          }
          return task;
        });
      });
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Load saved downloads on app start
  useEffect(() => {
    const loadSavedDownloads = async () => {
      try {
        const saved: any[] = await invoke('get_downloads');
        if (saved.length > 0) {
          const loadedTasks: DownloadTask[] = saved.map(d => ({
            id: d.id,
            filename: d.filename,
            url: d.url,
            progress: (d.downloaded_bytes / d.total_size) * 100,
            downloaded: d.downloaded_bytes,
            total: d.total_size,
            speed: 0,
            status: d.status as 'Paused' | 'Done' | 'Error' | 'Downloading'
          }));
          setTasks(loadedTasks);
          console.log('Loaded saved downloads:', loadedTasks);
        }
      } catch (error) {
        console.error('Failed to load saved downloads:', error);
      }
    };
    loadSavedDownloads();
  }, []);

  // Listen for downloads from browser extension
  useEffect(() => {
    const unlistenPromise = listen('extension_download', (event: any) => {
      const { url, filename } = event.payload;
      console.log('Extension download received:', url, filename);
      const extractedFilename = filename || url.split('/').pop()?.split('?')[0] || 'download';
      startDownload(url, extractedFilename);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for clipboard URLs
  useEffect(() => {
    const unlistenPromise = listen('clipboard_url', (event: any) => {
      const { url, filename } = event.payload;
      console.log('Clipboard URL detected:', url, filename);
      setClipboardData({ url, filename });

      // Auto-dismiss after 10 seconds
      setTimeout(() => {
        setClipboardData(prev => prev?.url === url ? null : prev);
      }, 10000);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for batch links from browser extension
  useEffect(() => {
    const unlistenPromise = listen('batch_links', (event: any) => {
      const links = event.payload as Array<{ url: string; filename: string }>;
      console.log('Batch links received:', links.length);
      setBatchLinks(links);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  // Listen for scheduled downloads starting
  useEffect(() => {
    const unlistenPromise = listen('scheduled_download_start', (event: any) => {
      const { url, filename } = event.payload;
      console.log('Scheduled download starting:', url, filename);
      startDownload(url, filename);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  /* ------------------ Memoized Handlers ------------------ */

  const startDownload = async (url: string, filename: string, force: boolean = false) => {
    // startDownload touches state heavily, we can keep it as is or memoize if needed.
    // It's usually called from modals so it's not passed to list items directly.
    // But consistent style is good.
    const downloadId = generateId();

    const newTask: DownloadTask = {
      id: downloadId,
      filename,
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
        path: `C:\\Users\\aditya\\Desktop\\${filename}`,
        force
      });
    } catch (error) {
      console.error(error);
      setTasks(prev => prev.map(t => t.id === downloadId ? { ...t, status: 'Error' } : t));
    }
  };



  // Wait, I can't change the component signature easily right now without breaking `DownloadList`.
  // Let's implement the ref pattern for `tasks` to break the dependency cycle.

  const tasksRef = useRef(tasks);
  useEffect(() => { tasksRef.current = tasks; }, [tasks]);

  const pauseDownloadMemo = React.useCallback(async (id: string) => {
    const task = tasksRef.current.find(t => t.id === id);
    if (!task) return;

    try {
      await invoke("pause_download", {
        id
      });
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Paused', speed: 0 } : t));
    } catch (error) {
      console.error("Failed to pause:", error);
    }
  }, []); // Stable!

  const resumeDownloadMemo = React.useCallback(async (id: string) => {
    const task = tasksRef.current.find(t => t.id === id);
    if (task) {
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Downloading' } : t));
      try {
        await invoke("start_download", {
          id: task.id,
          url: task.url,
          path: `C:\\Users\\aditya\\Desktop\\${task.filename}`
        });
      } catch (error) {
        console.error("Failed to resume:", error);
        setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Error' } : t));
      }
    }
  }, []); // Stable

  const deleteDownloadMemo = React.useCallback(async (id: string) => {
    try {
      await invoke("remove_download_entry", { id });
      setTasks(prev => prev.filter(t => t.id !== id));
      lastUpdate.current.delete(id);
    } catch (error) {
      console.error("Failed to delete:", error);
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
      console.error("Failed to persist move:", error);
    }
  }, []); // Stable

  const handleStream = async (id: number) => {
    try {
      const url = await invoke<string>('play_torrent', { id });
      console.log("Streaming URL:", url);
      // Open the URL in default player/browser
      await invoke('open_file', { path: url });
    } catch (e) {
      console.error("Stream failed", e);
      // toastRef might be null if not using conditional, but typically safe inside handler
      toastRef.current?.addToast("Stream error: " + e, 'error');
    }
  };

  // Calculate stats
  const stats = {
    total: tasks.length,
    downloading: tasks.filter(t => t.status === 'Downloading').length,
    completed: tasks.filter(t => t.status === 'Done').length,
    totalBytes: tasks.reduce((sum, t) => sum + t.downloaded, 0),
  };

  const handleSpeedLimitChange = (limit: number) => {
    // TODO: Implement speed limit change
    console.log("Speed limit changed:", limit);
    // Invoke backend command if exists
    // invoke("set_global_speed_limit", { limitBytes: limit });
  };

  const handleClipboardDownload = (url: string, filename: string) => {
    startDownload(url, filename);
    setClipboardData(null);
  };

  return (
    <>
      <Layout
        onAddClick={async (e) => {
          if (e.shiftKey) {
            try {
              const text = await navigator.clipboard.readText();
              if (text && (text.startsWith('http') || text.startsWith('magnet:'))) {
                console.log("Force Download Key detected. Adding:", text);
                if (text.startsWith('magnet:')) {
                  invoke("add_magnet_link", { magnet: text }).catch(console.error);
                } else {
                  const filename = text.split('/').pop()?.split('?')[0] || 'clipboard_download';
                  startDownload(text, filename);
                }
                toastRef.current?.addToast(`Started Force Download: ${text}`, 'success');
              } else {
                toastRef.current?.addToast(`Clipboard does not contain a valid URL`, 'error');
              }
            } catch (err) {
              console.error("Failed to read clipboard:", err);
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
      >
        {activeTab === 'downloads' ? (
          tasks.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon">📥</div>
              <h3>No Downloads Yet</h3>
              <p>Click "+ Add Url" to start your first download, or copy a download link to your clipboard.</p>
            </div>
          ) : (
            <DownloadList
              tasks={tasks}
              onPause={pauseDownloadMemo}
              onResume={resumeDownloadMemo}
              onDelete={deleteDownloadMemo}
              onMoveUp={(id) => moveTaskMemo(id, 'up')}
              onMoveDown={(id) => moveTaskMemo(id, 'down')}
            />
          )
        ) : activeTab === 'torrents' ? (
          <TorrentList onPlay={handleStream} />
        ) : activeTab === 'feeds' ? (
          <FeedsTab />
        ) : activeTab === 'plugins' ? (
          <PluginEditor />
        ) : (
          <SearchTab />
        )}
      </Layout>
      <Suspense fallback={null}>
        {isModalOpen && (
          <AddDownloadModal
            isOpen={isModalOpen}
            onClose={() => setIsModalOpen(false)}
            onStart={startDownload}
          />
        )}
        {isTorrentModalOpen && (
          <AddTorrentModal
            isOpen={isTorrentModalOpen}
            onClose={() => setIsTorrentModalOpen(false)}
            onAdd={(magnet) => {
              console.log("Adding magnet:", magnet);
              invoke("add_magnet_link", { magnet }).catch(console.error);
            }}
          />
        )}
        {isSettingsOpen && (
          <SettingsPage isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
        )}
        {batchLinks.length > 0 && (
          <BatchDownloadModal
            isOpen={batchLinks.length > 0}
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

      {clipboardData && (
        <ClipboardToast
          message="URL detected in clipboard"
          filename={clipboardData.filename}
          onDownload={() => handleClipboardDownload(clipboardData.url, clipboardData.filename)}
          onDismiss={() => setClipboardData(null)}
        />
      )}
      <DropTarget onDrop={(_url) => {
        setIsModalOpen(true);
        // Could auto-fill URL here if modal supports it
      }} />
      <ToastManager ref={toastRef} />
    </>
  );
}

export default App;
