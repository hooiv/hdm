import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { Layout } from "./components/Layout";
import { AddDownloadModal } from "./components/AddDownloadModal";
import { DownloadList } from "./components/DownloadList";
import { DownloadTask } from "./components/DownloadItem";

function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Use useRef for mutable state that doesn't trigger re-renders
  const lastUpdate = useRef<Map<string, { time: number, bytes: number, speed: number }>>(new Map());

  useEffect(() => {
    const unlistenPromise = listen('download_progress', (event: any) => {
      const { downloaded, total } = event.payload;

      setTasks(prevTasks => {
        return prevTasks.map(task => {
          if (task.id === '1') {
            const now = Date.now();
            const last = lastUpdate.current.get('1');
            let speed = last?.speed || 0;

            if (last) {
              const timeDiff = (now - last.time) / 1000;
              const bytesDiff = downloaded - last.bytes;

              if (bytesDiff < 0) {
                lastUpdate.current.set('1', { time: now, bytes: downloaded, speed: 0 });
                speed = 0;
              } else if (timeDiff >= 0.3 && bytesDiff > 0) {
                speed = bytesDiff / timeDiff;
                lastUpdate.current.set('1', { time: now, bytes: downloaded, speed });
              }
            } else {
              lastUpdate.current.set('1', { time: now, bytes: downloaded, speed: 0 });
            }

            const newTask: DownloadTask = {
              ...task,
              progress: Math.min((downloaded / total) * 100, 100),
              downloaded,
              total,
              speed,
              status: downloaded >= total ? 'Done' : 'Downloading'
            };
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

  const startDownload = async (url: string, filename: string) => {
    const newTask: DownloadTask = {
      id: '1',
      filename,
      url,
      progress: 0,
      downloaded: 0,
      total: 0,
      speed: 0,
      status: 'Downloading'
    };

    lastUpdate.current.delete('1');
    setTasks([newTask]);

    try {
      await invoke("start_download", {
        id: newTask.id,
        url,
        path: `C:\\Users\\aditya\\Desktop\\${filename}`
      });
    } catch (error) {
      console.error(error);
      setTasks(prev => prev.map(t => t.id === '1' ? { ...t, status: 'Error' } : t));
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

  return (
    <>
      <Layout onAddClick={() => setIsModalOpen(true)}>
        <DownloadList tasks={tasks} onPause={pauseDownload} onResume={resumeDownload} onDelete={deleteDownload} />
      </Layout>
      <AddDownloadModal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        onStart={startDownload}
      />
    </>
  );
}

export default App;
