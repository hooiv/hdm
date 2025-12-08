import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { Layout } from "./components/Layout";
import { AddDownloadModal } from "./components/AddDownloadModal";
import { DownloadList } from "./components/DownloadList";
import { DownloadTask } from "./components/DownloadItem";
import { SettingsPage } from "./components/SettingsPage";
import { ClipboardToast } from "./components/ClipboardToast";
import { BatchDownloadModal } from "./components/BatchDownloadModal";
import { ScheduleModal } from "./components/ScheduleModal";
import { DropTarget } from "./components/DropTarget";
import { SpiderModal } from "./components/SpiderModal";
import { ToastManager, ToastRef } from "./components/ToastManager";
import { AddTorrentModal } from "./components/AddTorrentModal";
import { TorrentList } from "./components/TorrentList";

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
  const [activeTab, setActiveTab] = useState<'downloads' | 'torrents'>('downloads');

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

  const startDownload = async (url: string, filename: string) => {
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
        path: `C:\\Users\\aditya\\Desktop\\${filename}`
      });
    } catch (error) {
      console.error(error);
      setTasks(prev => prev.map(t => t.id === downloadId ? { ...t, status: 'Error' } : t));
    }
  };

  const pauseDownload = async (id: string) => {
    const task = tasks.find(t => t.id === id);
    if (!task) return;

    try {
      await invoke("pause_download", {
        id,
        url: task.url,
        path: `C:\\Users\\aditya\\Desktop\\${task.filename}`,
        filename: task.filename,
        downloaded: task.downloaded,
        total: task.total
      });
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Paused', speed: 0 } : t));
    } catch (error) {
      console.error("Failed to pause:", error);
    }
  };

  const resumeDownload = async (id: string) => {
    const task = tasks.find(t => t.id === id);
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
  };

  const deleteDownload = async (id: string) => {
    try {
      await invoke("remove_download_entry", { id });
      setTasks(prev => prev.filter(t => t.id !== id));
      lastUpdate.current.delete(id);
    } catch (error) {
      console.error("Failed to delete:", error);
    }
  };

  const handleSpeedLimitChange = async (limitKbps: number) => {
    try {
      console.log(`Setting speed limit to ${limitKbps} KB/s`);
      // The Rust command expects u64, so ensure we pass a number
      await invoke("set_speed_limit", { limitKbps: Math.floor(limitKbps) });
    } catch (error) {
      console.error("Failed to set speed limit:", error);
    }
  };

  const handleClipboardDownload = () => {
    if (clipboardData) {
      startDownload(clipboardData.url, clipboardData.filename);
      setClipboardData(null);
    }
  };

  const moveTask = async (id: string, direction: 'up' | 'down') => {
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
      // Optional: Revert state if failed, but for simple reorder just logging is often enough MVP
    }
  };

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

  return (
    <>
      <Layout
        onAddClick={() => setIsModalOpen(true)}
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
              onPause={pauseDownload}
              onResume={resumeDownload}
              onDelete={deleteDownload}
              onMoveUp={(id) => moveTask(id, 'up')}
              onMoveDown={(id) => moveTask(id, 'down')}
            />
          )
        ) : (
          <TorrentList onPlay={handleStream} />
        )}
      </Layout>
      <AddDownloadModal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        onStart={startDownload}
      />
      <AddTorrentModal
        isOpen={isTorrentModalOpen}
        onClose={() => setIsTorrentModalOpen(false)}
        onAdd={(magnet) => {
          // We will implement handleAddTorrent later involving Rust invoke
          console.log("Adding magnet:", magnet);
          invoke("add_magnet_link", { magnet }).catch(console.error);
        }}
      />
      {isSettingsOpen && (
        <SettingsPage onClose={() => setIsSettingsOpen(false)} />
      )}
      {clipboardData && (
        <ClipboardToast
          message="URL detected in clipboard"
          filename={clipboardData.filename}
          onDownload={handleClipboardDownload}
          onDismiss={() => setClipboardData(null)}
        />
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
      <ScheduleModal
        isOpen={isScheduleOpen}
        onClose={() => setIsScheduleOpen(false)}
      />
      <SpiderModal
        isOpen={isSpiderOpen}
        onClose={() => setIsSpiderOpen(false)}
        onDownload={(files) => {
          files.forEach(f => startDownload(f.url, f.filename));
          setIsSpiderOpen(false);
        }}
      />
      <DropTarget onDrop={(_url) => {
        setIsModalOpen(true);
        // Could auto-fill URL here if modal supports it
      }} />
      <ToastManager ref={toastRef} />
    </>
  );
}

export default App;
