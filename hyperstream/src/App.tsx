import React, { useState, useEffect, useRef, useCallback } from "react";
import { AnimatePresence } from "framer-motion";
import { safeInvoke as invoke, safeListen as listen, safeGetWindowByLabel } from "./utils/tauri";
import { debug, error as logError } from "./utils/logger";
import { clearETAState } from "./utils/formatters";
import "./App.css";
import { Layout } from "./components/Layout";
import { DownloadList } from "./components/DownloadList";
import { ClipboardToast } from "./components/ClipboardToast";
import { DropTarget } from "./components/DropTarget";
import { RecoverableLazy } from "./components/RecoverableLazy";
import { ToastManager, ToastRef } from "./components/ToastManager";
import type { AddTorrentResult, DownloadProgressPayload, ClipboardUrlPayload, ExtensionDownloadPayload, BatchLink, ScheduledDownloadPayload, SavedDownload, AppSettings, DiscoveredMirror, DownloadTask, MirrorStat } from "./types";
import { toTaskStatus } from "./types";
import { findActiveTaskByUrl, isDuplicateDownloadError, normalizeDownloadUrl } from "./utils/downloadDedup";
import { buildExtensionDownloadHeaders } from "./utils/extensionDownload";

// Lazy-loaded surfaces to improve initial render time
const loadAddDownloadModal = () => import("./components/AddDownloadModal");
const loadSettingsPage = () => import("./components/SettingsPage");
const loadBatchDownloadModal = () => import("./components/BatchDownloadModal");
const loadScheduleModal = () => import("./components/ScheduleModal");
const loadSpiderModal = () => import("./components/SpiderModal");
const loadCrashRecoveryModal = () => import("./components/CrashRecoveryModal");
const loadStreamDetectorModal = () => import("./components/StreamDetectorModal");
const loadNetworkDiagnosticsModal = () => import("./components/NetworkDiagnosticsModal");
const loadMediaProcessingModal = () => import("./components/MediaProcessingModal");
const loadIpfsDownloadModal = () => import("./components/IpfsDownloadModal");
const loadAddTorrentModal = () => import("./components/AddTorrentModal");
const loadTorrentList = () => import("./components/TorrentList");
const loadFeedsTab = () => import("./components/FeedsTab");
const loadHistoryTab = () => import("./components/HistoryTab");
const loadActivityTab = () => import("./components/ActivityTab");
const loadQueueManager = () => import("./components/QueueManager");
const loadSearchTab = () => import("./components/SearchTab");
const loadPluginEditor = () => import("./components/PluginEditor");
const loadDownloadGroupTree = () => import("./components/DownloadGroupTree");

const resolveAddDownloadModal = (module: Awaited<ReturnType<typeof loadAddDownloadModal>>) => module.AddDownloadModal;
const resolveSettingsPage = (module: Awaited<ReturnType<typeof loadSettingsPage>>) => module.SettingsPage;
const resolveBatchDownloadModal = (module: Awaited<ReturnType<typeof loadBatchDownloadModal>>) => module.BatchDownloadModal;
const resolveScheduleModal = (module: Awaited<ReturnType<typeof loadScheduleModal>>) => module.ScheduleModal;
const resolveSpiderModal = (module: Awaited<ReturnType<typeof loadSpiderModal>>) => module.SpiderModal;
const resolveCrashRecoveryModal = (module: Awaited<ReturnType<typeof loadCrashRecoveryModal>>) => module.CrashRecoveryModal;
const resolveStreamDetectorModal = (module: Awaited<ReturnType<typeof loadStreamDetectorModal>>) => module.StreamDetectorModal;
const resolveNetworkDiagnosticsModal = (module: Awaited<ReturnType<typeof loadNetworkDiagnosticsModal>>) => module.NetworkDiagnosticsModal;
const resolveMediaProcessingModal = (module: Awaited<ReturnType<typeof loadMediaProcessingModal>>) => module.MediaProcessingModal;
const resolveIpfsDownloadModal = (module: Awaited<ReturnType<typeof loadIpfsDownloadModal>>) => module.IpfsDownloadModal;
const resolveAddTorrentModal = (module: Awaited<ReturnType<typeof loadAddTorrentModal>>) => module.AddTorrentModal;
const resolveTorrentList = (module: Awaited<ReturnType<typeof loadTorrentList>>) => module.TorrentList;
const resolveFeedsTab = (module: Awaited<ReturnType<typeof loadFeedsTab>>) => module.FeedsTab;
const resolveHistoryTab = (module: Awaited<ReturnType<typeof loadHistoryTab>>) => module.HistoryTab;
const resolveActivityTab = (module: Awaited<ReturnType<typeof loadActivityTab>>) => module.ActivityTab;
const resolveQueueManager = (module: Awaited<ReturnType<typeof loadQueueManager>>) => module.QueueManager;
const resolveSearchTab = (module: Awaited<ReturnType<typeof loadSearchTab>>) => module.SearchTab;
const resolvePluginEditor = (module: Awaited<ReturnType<typeof loadPluginEditor>>) => module.default;

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

interface DownloadSpotlightRequest {
  taskId: string;
  token: number;
}

const tabChunkLoaders = {
  torrents: loadTorrentList,
  feeds: loadFeedsTab,
  search: loadSearchTab,
  plugins: loadPluginEditor,
  history: loadHistoryTab,
  activity: loadActivityTab,
  queue: loadQueueManager,
  groups: loadDownloadGroupTree,
} as const;

type ActiveTab = 'downloads' | keyof typeof tabChunkLoaders;

const tabLoadingFallback = (
  <div className="flex-1 flex items-center justify-center text-slate-500">Loading view...</div>
);

const renderModalLoadFailure = ({
  title,
  message,
  onClose,
  closeLabel = "Close",
  retryLabel = "Retry loading modal",
}: {
  title: string;
  message: string;
  onClose: () => void;
  closeLabel?: string;
  retryLabel?: string;
}) => (error: Error, retry: () => void) => (
  <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
    <div className="w-full max-w-lg rounded-2xl border border-amber-500/20 bg-slate-900/95 p-6 shadow-2xl">
      <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl border border-amber-500/20 bg-amber-500/10 text-2xl text-amber-300">
        ⚠️
      </div>
      <h3 className="text-center text-lg font-semibold text-slate-100">{title}</h3>
      <p className="mt-2 text-center text-sm leading-6 text-slate-400">{message}</p>
      <div className="mt-4 rounded-xl border border-amber-500/10 bg-black/20 px-3 py-2">
        <code className="text-xs text-amber-300 break-all">{error.message}</code>
      </div>
      <div className="mt-5 flex flex-wrap items-center justify-center gap-3">
        <button
          onClick={onClose}
          className="inline-flex items-center justify-center rounded-xl border border-slate-700 bg-slate-800 px-4 py-2 text-sm font-medium text-slate-200 transition-colors hover:bg-slate-700"
        >
          {closeLabel}
        </button>
        <button
          onClick={retry}
          className="inline-flex items-center justify-center rounded-xl bg-cyan-500/20 px-4 py-2 text-sm font-medium text-cyan-300 transition-colors hover:bg-cyan-500/30"
        >
          {retryLabel}
        </button>
      </div>
    </div>
  </div>
);

function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isScheduleOpen, setIsScheduleOpen] = useState(false);
  const [isSpiderOpen, setIsSpiderOpen] = useState(false);
  const [isCrashRecoveryOpen, setIsCrashRecoveryOpen] = useState(false);
  const [isStreamDetectorOpen, setIsStreamDetectorOpen] = useState(false);
  const [isNetworkDiagOpen, setIsNetworkDiagOpen] = useState(false);
  const [isMediaProcessingOpen, setIsMediaProcessingOpen] = useState(false);
  const [isIpfsOpen, setIsIpfsOpen] = useState(false);
  const [isTorrentModalOpen, setIsTorrentModalOpen] = useState(false);
  const [clipboardData, setClipboardData] = useState<ClipboardData | null>(null);
  const [batchLinks, setBatchLinks] = useState<BatchLink[]>([]);
  const [droppedUrl, setDroppedUrl] = useState<string | undefined>(undefined);
  const [activeTab, setActiveTab] = useState<ActiveTab>('downloads');
  const [downloadSpotlight, setDownloadSpotlight] = useState<DownloadSpotlightRequest | null>(null);
  const [downloadDir, setDownloadDir] = useState<string>('');

  const [, setIsOverlayVisible] = useState(false);
  const isOverlayVisibleRef = useRef(false);
  const prefetchedTabsRef = useRef(new Set<keyof typeof tabChunkLoaders>());

  const prefetchTab = useCallback((tab: ActiveTab) => {
    if (tab === 'downloads' || prefetchedTabsRef.current.has(tab)) {
      return;
    }

    prefetchedTabsRef.current.add(tab);
    void tabChunkLoaders[tab]();
  }, []);

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

  // keyboard shortcuts — refs updated after pauseAll/resumeAll are defined
  const pauseAllRef = useRef<(() => void) | null>(null);
  const resumeAllRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        toggleOverlay();
      } else if (e.ctrlKey && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setIsModalOpen(true);
      } else if (e.ctrlKey && e.key.toLowerCase() === 'p') {
        e.preventDefault();
        pauseAllRef.current?.();
      } else if (e.ctrlKey && e.key.toLowerCase() === 'r') {
        e.preventDefault();
        resumeAllRef.current?.();
      } else if (e.ctrlKey && e.key === ',') {
        e.preventDefault();
        setIsSettingsOpen(true);
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
  const pendingDownloadUrlsRef = useRef<Map<string, string>>(new Map());
  const downloadSpotlightCounterRef = useRef(0);
  // Track auto-remove timers so they can be cleaned on unmount
  const autoRemoveTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  // Stable ref for startDownload to avoid stale closures in event listeners
  const startDownloadRef = useRef<(url: string, filename: string, force?: boolean, customHeaders?: Record<string,string>, mirrors?: [string, string][], expectedChecksum?: string) => Promise<void>>(null!);

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
                  ? {
                      ...t,
                      filename: match.filename,
                      url: match.url,
                      expectedChecksum: match.expected_checksum || undefined,
                      integrityStatus: match.expected_checksum ? 'pending' : t.integrityStatus,
                    }
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
            dateAdded: Date.now(),
          };
          return [...prevTasks, newTask];
        }

        return prevTasks.map(task => {
          if (task.id === id) {
            const now = Date.now();
            const last = lastUpdate.current.get(id);
            let speed = last?.speed || 0;

            // Primary: use backend EMA-smoothed per-segment speeds
            const backendSpeed = segments.reduce((sum, s) => sum + (s.speed_bps || 0), 0);

            if (backendSpeed > 0) {
              // Backend already applies EMA — use directly with light frontend smoothing
              const alpha = 0.4;
              speed = last ? alpha * backendSpeed + (1 - alpha) * (last.speed || 0) : backendSpeed;
              lastUpdate.current.set(id, { time: now, bytes: downloaded, speed });
            } else if (last) {
              const timeDiff = (now - last.time) / 1000;
              const bytesDiff = downloaded - last.bytes;

              if (bytesDiff < 0) {
                lastUpdate.current.set(id, { time: now, bytes: downloaded, speed: 0 });
                speed = 0;
              } else if (timeDiff >= 0.3 && bytesDiff > 0) {
                const instantSpeed = bytesDiff / timeDiff;
                const alpha = 0.3;
                speed = alpha * instantSpeed + (1 - alpha) * (last.speed || 0);
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
                clearETAState(id);
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

  // Listen for mirror stats from multi-source downloads
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; mirrors: MirrorStat[] }>('mirror_stats', (event) => {
      const { id, mirrors } = event.payload;
      setTasks(prev => prev.map(t => t.id === id ? { ...t, mirrorStats: mirrors } : t));
    });
    return () => { unlistenPromise.then(unlisten => unlisten()); };
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
            expectedChecksum: d.expected_checksum || undefined,
            progress: d.total_size > 0 ? (d.downloaded_bytes / d.total_size) * 100 : 0,
            downloaded: d.downloaded_bytes,
            total: d.total_size,
            speed: 0,
            status: toTaskStatus(d.status),
            integrityStatus: d.expected_checksum ? 'pending' : undefined,
            dateAdded: Date.now(),
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
      const { url, filename, customHeaders, pageUrl, source } = event.payload;
      debug('Extension download received:', url, filename, source);
      const extractedFilename = filename || url.split('/').pop()?.split('?')[0] || 'download';
      startDownloadRef.current(
        url,
        extractedFilename,
        false,
        buildExtensionDownloadHeaders(customHeaders, pageUrl),
      );
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

  // Listen for quiet hours deferral notifications
  useEffect(() => {
    const unlistenPromise = listen<{ deferred_count: number; resume_at: string }>('quiet_hours_deferred', (event) => {
      const { deferred_count, resume_at } = event.payload;
      const resumeTime = new Date(resume_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      toastRef.current?.addToast(
        `${deferred_count} scheduled download${deferred_count > 1 ? 's' : ''} deferred — quiet hours active until ${resumeTime}`,
        'info'
      );
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

  // Listen for download errors from the backend monitor
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; error: string }>('download_error', (event) => {
      const { id, error } = event.payload;
      logError('Download error:', id, error);
      setTasks(prev => prev.map(t =>
        t.id === id ? { ...t, status: 'Error' as const, errorMessage: error, speed: 0 } : t
      ));
      const task = tasks.find(t => t.id === id);
      const name = task?.filename || id;
      toastRef.current?.addToast(`Download failed: ${name} — ${error}`, 'error');
    });
    return () => { unlistenPromise.then(unlisten => unlisten()); };
  }, [tasks]);

  // Listen for auto-retry events
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; attempt: number; max_retries: number }>('download_retry', (event) => {
      const { id, attempt, max_retries } = event.payload;
      debug('Download retry:', id, `attempt ${attempt}/${max_retries}`);
      setTasks(prev => prev.map(t =>
        t.id === id ? { ...t, status: 'Downloading' as const, errorMessage: undefined } : t
      ));
      const task = tasks.find(t => t.id === id);
      const name = task?.filename || id;
      toastRef.current?.addToast(`Retrying ${name} (attempt ${attempt}/${max_retries})`, 'info');
    });
    return () => { unlistenPromise.then(unlisten => unlisten()); };
  }, [tasks]);

  // Listen for integrity check results (auto-verify via Content-MD5 / sidecar files)
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; verified: boolean; method: string; algorithm: string; message: string }>('integrity_check', (event) => {
      const { id, verified, message } = event.payload;
      setTasks(prev => prev.map(t =>
        t.id === id ? { ...t, integrityStatus: verified ? 'verified' as const : 'failed' as const } : t
      ));
      if (verified) {
        toastRef.current?.addToast(`✓ Integrity verified: ${message}`, 'success');
      } else {
        toastRef.current?.addToast(`✗ Integrity failed: ${message}`, 'error');
      }
    });
    return () => { unlistenPromise.then(unlisten => unlisten()); };
  }, []);

  // Listen for queue-supplied checksum verification results
  useEffect(() => {
    const unlistenPromise = listen<{ id: string }>('integrity_check_passed', (event) => {
      setTasks(prev => prev.map(t =>
        t.id === event.payload.id ? { ...t, integrityStatus: 'verified' as const } : t
      ));
      toastRef.current?.addToast('Checksum verified ✓', 'success');
    });
    const unlisten2 = listen<{ id: string; error: string }>('integrity_check_failed', (event) => {
      setTasks(prev => prev.map(t =>
        t.id === event.payload.id ? { ...t, integrityStatus: 'failed' as const } : t
      ));
      toastRef.current?.addToast(`Checksum mismatch: ${event.payload.error}`, 'error');
    });
    return () => {
      unlistenPromise.then(unlisten => unlisten());
      unlisten2.then(unlisten => unlisten());
    };
  }, []);

  // Listen for virus scan results
  useEffect(() => {
    const unlistenPromise = listen<{ id: string; status: string; threat?: string }>('virus_scan_result', (event) => {
      const { id, status, threat } = event.payload;
      if (status === 'infected') {
        toastRef.current?.addToast(`⚠ Threat detected in download: ${threat}`, 'error');
      } else if (status === 'clean') {
        toastRef.current?.addToast('Virus scan: file is clean ✓', 'success');
      } else if (status === 'error') {
        toastRef.current?.addToast(`Virus scan error: ${threat}`, 'error');
      }
      setTasks(prev => prev.map(t =>
        t.id === id ? { ...t, virusScanStatus: status as 'clean' | 'infected' | 'scanning' } : t
      ));
    });
    return () => { unlistenPromise.then(unlisten => unlisten()); };
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

  const revealDownload = useCallback((taskId?: string) => {
    setActiveTab('downloads');

    if (!taskId) {
      setDownloadSpotlight(null);
      return;
    }

    downloadSpotlightCounterRef.current += 1;
    setDownloadSpotlight({
      taskId,
      token: downloadSpotlightCounterRef.current,
    });
  }, []);

  const startDownload = async (url: string, filename: string, force: boolean = false, customHeaders?: Record<string, string>, mirrors?: [string, string][], expectedChecksum?: string) => {
    const safeFilename = sanitizeFilename(filename);
    const downloadId = generateId();
    const normalizedUrl = normalizeDownloadUrl(url);
    const normalizedChecksum = expectedChecksum?.trim() || undefined;

    if (!force) {
      const existingTask = findActiveTaskByUrl(tasksRef.current, url);
      if (existingTask) {
        revealDownload(existingTask.id);
        toastRef.current?.addToast(`Already downloading: ${existingTask.filename}`, 'info');
        return;
      }

      const pendingTaskId = normalizedUrl ? pendingDownloadUrlsRef.current.get(normalizedUrl) : undefined;
      if (pendingTaskId) {
        revealDownload(pendingTaskId);
        toastRef.current?.addToast(`Download already starting: ${safeFilename}`, 'info');
        return;
      }
    }

    const newTask: DownloadTask = {
      id: downloadId,
      filename: safeFilename,
      url,
      expectedChecksum: normalizedChecksum,
      progress: 0,
      downloaded: 0,
      total: 0,
      speed: 0,
      status: 'Downloading',
      integrityStatus: normalizedChecksum ? 'pending' : undefined,
      dateAdded: Date.now(),
    };

    lastUpdate.current.delete(downloadId);
    if (normalizedUrl) {
      pendingDownloadUrlsRef.current.set(normalizedUrl, downloadId);
    }

    // Add to existing tasks instead of replacing
    setTasks(prev => [...prev, newTask]);

    try {
      if (mirrors && mirrors.length > 0) {
        await invoke("start_multi_source_download", {
          id: downloadId,
          primaryUrl: url,
          mirrors,
          path: `${downloadDir}/${safeFilename}`,
          customHeaders: customHeaders || null,
          expectedChecksum: normalizedChecksum || null,
        });
      } else {
        await invoke("start_download", {
          id: downloadId,
          url,
          path: `${downloadDir}/${safeFilename}`,
          force,
          customHeaders: customHeaders || null,
          expectedChecksum: normalizedChecksum || null,
        });
      }
    } catch (error) {
      if (isDuplicateDownloadError(error)) {
        debug('Duplicate download coalesced in UI:', url, error);
        setTasks(prev => prev.filter(t => t.id !== downloadId));
        const existingTask = findActiveTaskByUrl(tasksRef.current, url, downloadId);
        revealDownload(existingTask?.id);
        toastRef.current?.addToast(
          existingTask ? `Already downloading: ${existingTask.filename}` : 'This download is already active',
          'info'
        );
        return;
      }

      logError(error);
      toastRef.current?.addToast(`Failed to start download: ${error}`, 'error');
      setTasks(prev => prev.map(t => t.id === downloadId ? { ...t, status: 'Error' } : t));
    } finally {
      if (normalizedUrl && pendingDownloadUrlsRef.current.get(normalizedUrl) === downloadId) {
        pendingDownloadUrlsRef.current.delete(normalizedUrl);
      }
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

  const updateTaskDiscoveredMirrorsMemo = React.useCallback((id: string, mirrors: DiscoveredMirror[]) => {
    setTasks(prev => prev.map(task =>
      task.id === id
        ? { ...task, discoveredMirrors: mirrors.length > 0 ? mirrors : undefined }
        : task
    ));
  }, []);

  const getTaskResumeMirrorsMemo = React.useCallback((task: DownloadTask): [string, string][] => {
    if (!task.url || !task.discoveredMirrors || task.discoveredMirrors.length === 0) {
      return [];
    }

    const normalizeUrl = (value: string) => value.trim().replace(/#.*$/, '').replace(/\/+$/, '').toLowerCase();
    const primaryKey = normalizeUrl(task.url);
    const seen = new Set<string>([primaryKey]);
    const mirrors: [string, string][] = [];

    for (const mirror of task.discoveredMirrors) {
      const url = mirror.url?.trim();
      if (!url) continue;

      const key = normalizeUrl(url);
      if (!key || seen.has(key)) continue;

      seen.add(key);
      mirrors.push([url, mirror.source]);

      if (mirrors.length >= 5) break;
    }

    return mirrors;
  }, []);

  const startExistingDownloadMemo = React.useCallback(async (task: DownloadTask) => {
    if (!task.url) {
      throw new Error('Download has no URL');
    }

    const path = `${downloadDirRef.current}/${task.filename}`;
    const mirrors = getTaskResumeMirrorsMemo(task);

    if (mirrors.length > 0) {
      await invoke('start_multi_source_download', {
        id: task.id,
        primaryUrl: task.url,
        mirrors,
        path,
        customHeaders: null,
        expectedChecksum: task.expectedChecksum || null,
      });
      return;
    }

    await invoke('start_download', {
      id: task.id,
      url: task.url,
      path,
      force: false,
      customHeaders: null,
      expectedChecksum: task.expectedChecksum || null,
    });
  }, [getTaskResumeMirrorsMemo]);

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
      setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Downloading', errorMessage: undefined } : t));
      try {
        await startExistingDownloadMemo(task);
      } catch (error) {
        if (isDuplicateDownloadError(error)) {
          debug('Duplicate resume request coalesced in UI:', id, error);
          toastRef.current?.addToast(`Already downloading: ${task.filename}`, 'info');
          return;
        }

        logError("Failed to resume:", error);
        toastRef.current?.addToast('Failed to resume download', 'error');
        setTasks(prev => prev.map(t => t.id === id ? { ...t, status: 'Error' } : t));
      }
    }
  }, [startExistingDownloadMemo]);

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
      toResume.some(r => r.id === x.id) ? { ...x, status: 'Downloading' as const, errorMessage: undefined } : x
    ));
    toResume.forEach(t => {
      startExistingDownloadMemo(t).catch((err) => {
        if (isDuplicateDownloadError(err)) {
          debug('Duplicate resume-all request coalesced in UI:', t.id, err);
          return;
        }

        logError('Failed to resume:', t.id, err);
        setTasks(prev => prev.map(x => x.id === t.id ? { ...x, status: 'Error' as const } : x));
      });
    });
  };

  // Keep keyboard shortcut refs in sync
  pauseAllRef.current = pauseAll;
  resumeAllRef.current = resumeAll;

  const handleClearCompleted = () => {
    const completedTasks = tasks.filter(t => t.status === 'Done');
    completedTasks.forEach(t => {
      invoke("remove_download_entry", { id: t.id }).catch(() => {});
      lastUpdate.current.delete(t.id);
      completedIds.current.delete(t.id);
      const timer = autoRemoveTimers.current.get(t.id);
      if (timer) { clearTimeout(timer); autoRemoveTimers.current.delete(t.id); }
    });
    setTasks(prev => prev.filter(t => t.status !== 'Done'));
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
                let started = false;
                if (text.startsWith('magnet:')) {
                  try {
                    const result = await invoke<AddTorrentResult>("add_magnet_link", { magnet: text });
                    started = true;
                    if (result.warnings.length > 0) {
                      toastRef.current?.addToast(
                        `Torrent added with warning: ${result.warnings[0]}`,
                        'info',
                      );
                    }
                  } catch (err) {
                    logError("Magnet link failed:", err);
                    toastRef.current?.addToast(`Failed to add magnet link: ${err}`, 'error');
                  }
                } else {
                  const filename = text.split('/').pop()?.split('?')[0] || 'clipboard_download';
                  startDownload(text, filename);
                  started = true;
                }
                if (started) {
                  toastRef.current?.addToast(`Started Force Download: ${text}`, 'success');
                }
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
        onCrashRecoveryClick={() => setIsCrashRecoveryOpen(true)}
        onStreamDetectorClick={() => setIsStreamDetectorOpen(true)}
        onNetworkDiagClick={() => setIsNetworkDiagOpen(true)}
        onMediaProcessingClick={() => setIsMediaProcessingOpen(true)}
        onIpfsClick={() => setIsIpfsOpen(true)}
        onSettingsClick={() => setIsSettingsOpen(true)}
        onOverlayClick={toggleOverlay}
        stats={stats}
        onSpeedLimitChange={handleSpeedLimitChange}
        activeTab={activeTab}
        onTabChange={setActiveTab}
        onTabIntent={prefetchTab}
        globalSpeed={globalSpeed}
      >
        {activeTab === 'downloads' ? (
          <div className="flex flex-col h-full">
            <GlobalTelemetry tasks={tasks} />
            <DownloadList
              tasks={tasks}
              onPause={pauseDownloadMemo}
              onResume={resumeDownloadMemo}
              onDiscoveredMirrors={updateTaskDiscoveredMirrorsMemo}
              onDelete={deleteDownloadMemo}
              onMoveUp={moveUpMemo}
              onMoveDown={moveDownMemo}
              downloadDir={downloadDir}
              spotlightRequest={downloadSpotlight}
              onClearCompleted={handleClearCompleted}
              onAddDownload={() => setIsModalOpen(true)}
            />
          </div>
        ) : activeTab === 'torrents' ? (
          <RecoverableLazy
            loader={loadTorrentList}
            resolve={resolveTorrentList}
            componentProps={{ onPlay: handleStream }}
            loadingFallback={tabLoadingFallback}
            failureTitle="Torrents view unavailable"
            failureMessage="HyperStream couldn’t load the Torrents view. Retry without leaving your current downloads."
          />
        ) : activeTab === 'feeds' ? (
          <RecoverableLazy
            loader={loadFeedsTab}
            resolve={resolveFeedsTab}
            componentProps={{}}
            loadingFallback={tabLoadingFallback}
            failureTitle="Feeds view unavailable"
            failureMessage="HyperStream couldn’t load the Feeds view. Retry to restore your subscriptions and release feed updates."
          />
        ) : activeTab === 'plugins' ? (
          <RecoverableLazy
            loader={loadPluginEditor}
            resolve={resolvePluginEditor}
            componentProps={{}}
            loadingFallback={<div className="flex-1 flex items-center justify-center text-slate-500">Loading plugins...</div>}
            failureTitle="Plugins view unavailable"
            failureMessage="HyperStream couldn’t load the Plugins workspace. Retry to recover plugin management without restarting the app."
          />
        ) : activeTab === 'history' ? (
          <RecoverableLazy
            loader={loadHistoryTab}
            resolve={resolveHistoryTab}
            componentProps={{}}
            loadingFallback={tabLoadingFallback}
            failureTitle="History view unavailable"
            failureMessage="HyperStream couldn’t load your download history. Retry to restore the archived session view."
          />
        ) : activeTab === 'activity' ? (
          <RecoverableLazy
            loader={loadActivityTab}
            resolve={resolveActivityTab}
            componentProps={{}}
            loadingFallback={tabLoadingFallback}
            failureTitle="Activity view unavailable"
            failureMessage="HyperStream couldn’t load the activity log. Retry to recover recent download diagnostics."
          />
        ) : activeTab === 'queue' ? (
          <RecoverableLazy
            loader={loadQueueManager}
            resolve={resolveQueueManager}
            componentProps={{}}
            loadingFallback={tabLoadingFallback}
            failureTitle="Queue view unavailable"
            failureMessage="HyperStream couldn’t load the queue manager. Retry to recover scheduling and priority controls."
          />
        ) : (
          <RecoverableLazy
            loader={loadSearchTab}
            resolve={resolveSearchTab}
            componentProps={{ onStartDownload: startDownload }}
            loadingFallback={tabLoadingFallback}
            failureTitle="Discover view unavailable"
            failureMessage="HyperStream couldn’t load the Discover view. Retry to restore search-driven download discovery."
          />
        )}
      </Layout>
      {isModalOpen && (
        <RecoverableLazy
          loader={loadAddDownloadModal}
          resolve={resolveAddDownloadModal}
          componentProps={{
            isOpen: isModalOpen,
            onClose: () => { setIsModalOpen(false); setDroppedUrl(undefined); },
            onStart: startDownload,
            initialUrl: droppedUrl,
          }}
          loadingFallback={null}
          failureTitle="Add download unavailable"
          failureMessage="HyperStream couldn’t load the Add Download modal. Retry without losing the URL you were preparing."
          renderFailure={renderModalLoadFailure({
            title: "Add download unavailable",
            message: "HyperStream couldn’t load the Add Download modal. Retry without losing the URL you were preparing.",
            onClose: () => { setIsModalOpen(false); setDroppedUrl(undefined); },
          })}
        />
      )}
      {isTorrentModalOpen && (
        <RecoverableLazy
          loader={loadAddTorrentModal}
          resolve={resolveAddTorrentModal}
          componentProps={{
            isOpen: isTorrentModalOpen,
            onClose: () => setIsTorrentModalOpen(false),
            onAdd: async (magnet, savePath, paused, initialPriority, pinned) => {
              debug("Adding magnet:", magnet);
              try {
                return await invoke<AddTorrentResult>("add_magnet_link", {
                  magnet,
                  savePath: savePath || null,
                  paused,
                  initialPriority: initialPriority === 'normal' ? null : initialPriority,
                  pinned: pinned ? true : null,
                });
              } catch (err) {
                logError("Magnet link failed:", err);
                toastRef.current?.addToast(`Failed to add magnet: ${err}`, 'error');
                throw err;
              }
            },
          }}
          loadingFallback={null}
          failureTitle="Torrent import unavailable"
          failureMessage="HyperStream couldn’t load the torrent import modal. Retry without leaving your current session."
          renderFailure={renderModalLoadFailure({
            title: "Torrent import unavailable",
            message: "HyperStream couldn’t load the torrent import modal. Retry without leaving your current session.",
            onClose: () => setIsTorrentModalOpen(false),
          })}
        />
      )}
      {isSettingsOpen && (
        <RecoverableLazy
          loader={loadSettingsPage}
          resolve={resolveSettingsPage}
          componentProps={{ isOpen: isSettingsOpen, onClose: () => setIsSettingsOpen(false) }}
          loadingFallback={null}
          failureTitle="Settings unavailable"
          failureMessage="HyperStream couldn’t load Settings. Retry to recover preferences without restarting the app."
          renderFailure={renderModalLoadFailure({
            title: "Settings unavailable",
            message: "HyperStream couldn’t load Settings. Retry to recover preferences without restarting the app.",
            onClose: () => setIsSettingsOpen(false),
          })}
        />
      )}
      {batchLinks.length > 0 && (
        <RecoverableLazy
          loader={loadBatchDownloadModal}
          resolve={resolveBatchDownloadModal}
          componentProps={{
            isOpen: true,
            links: batchLinks,
            onClose: () => setBatchLinks([]),
            onDownload: (links) => {
              links.forEach(link => startDownload(link.url, link.filename));
              setBatchLinks([]);
            },
          }}
          loadingFallback={null}
          failureTitle="Batch download unavailable"
          failureMessage="HyperStream couldn’t load the batch download modal. Retry without losing the captured link set."
          renderFailure={renderModalLoadFailure({
            title: "Batch download unavailable",
            message: "HyperStream couldn’t load the batch download modal. Retry without losing the captured link set.",
            onClose: () => setBatchLinks([]),
          })}
        />
      )}
      {isScheduleOpen && (
        <RecoverableLazy
          loader={loadScheduleModal}
          resolve={resolveScheduleModal}
          componentProps={{ isOpen: isScheduleOpen, onClose: () => setIsScheduleOpen(false) }}
          loadingFallback={null}
          failureTitle="Scheduler unavailable"
          failureMessage="HyperStream couldn’t load the scheduler modal. Retry to recover delayed-download controls."
          renderFailure={renderModalLoadFailure({
            title: "Scheduler unavailable",
            message: "HyperStream couldn’t load the scheduler modal. Retry to recover delayed-download controls.",
            onClose: () => setIsScheduleOpen(false),
          })}
        />
      )}
      {isSpiderOpen && (
        <RecoverableLazy
          loader={loadSpiderModal}
          resolve={resolveSpiderModal}
          componentProps={{
            isOpen: isSpiderOpen,
            onClose: () => setIsSpiderOpen(false),
            onDownload: (files) => {
              files.forEach(f => startDownload(f.url, f.filename));
              setIsSpiderOpen(false);
            },
          }}
          loadingFallback={null}
          failureTitle="Site spider unavailable"
          failureMessage="HyperStream couldn’t load the site spider modal. Retry to recover bulk link discovery."
          renderFailure={renderModalLoadFailure({
            title: "Site spider unavailable",
            message: "HyperStream couldn’t load the site spider modal. Retry to recover bulk link discovery.",
            onClose: () => setIsSpiderOpen(false),
          })}
        />
      )}
      {isCrashRecoveryOpen && (
        <RecoverableLazy
          loader={loadCrashRecoveryModal}
          resolve={resolveCrashRecoveryModal}
          componentProps={{ isOpen: isCrashRecoveryOpen, onClose: () => setIsCrashRecoveryOpen(false) }}
          loadingFallback={null}
          failureTitle="Crash recovery unavailable"
          failureMessage="HyperStream couldn’t load crash recovery. Retry to restore the interrupted session workflow."
          renderFailure={renderModalLoadFailure({
            title: "Crash recovery unavailable",
            message: "HyperStream couldn’t load crash recovery. Retry to restore the interrupted session workflow.",
            onClose: () => setIsCrashRecoveryOpen(false),
          })}
        />
      )}
      {isStreamDetectorOpen && (
        <RecoverableLazy
          loader={loadStreamDetectorModal}
          resolve={resolveStreamDetectorModal}
          componentProps={{
            isOpen: isStreamDetectorOpen,
            onClose: () => setIsStreamDetectorOpen(false),
            onDownload: (url, filename) => {
              startDownload(url, filename || url.split('/').pop() || 'stream');
              setIsStreamDetectorOpen(false);
            },
          }}
          loadingFallback={null}
          failureTitle="Stream detector unavailable"
          failureMessage="HyperStream couldn’t load the stream detector modal. Retry to recover capture-and-download flow."
          renderFailure={renderModalLoadFailure({
            title: "Stream detector unavailable",
            message: "HyperStream couldn’t load the stream detector modal. Retry to recover capture-and-download flow.",
            onClose: () => setIsStreamDetectorOpen(false),
          })}
        />
      )}
      {isNetworkDiagOpen && (
        <RecoverableLazy
          loader={loadNetworkDiagnosticsModal}
          resolve={resolveNetworkDiagnosticsModal}
          componentProps={{ isOpen: isNetworkDiagOpen, onClose: () => setIsNetworkDiagOpen(false) }}
          loadingFallback={null}
          failureTitle="Network diagnostics unavailable"
          failureMessage="HyperStream couldn’t load network diagnostics. Retry to recover troubleshooting tools."
          renderFailure={renderModalLoadFailure({
            title: "Network diagnostics unavailable",
            message: "HyperStream couldn’t load network diagnostics. Retry to recover troubleshooting tools.",
            onClose: () => setIsNetworkDiagOpen(false),
          })}
        />
      )}
      {isMediaProcessingOpen && (
        <RecoverableLazy
          loader={loadMediaProcessingModal}
          resolve={resolveMediaProcessingModal}
          componentProps={{ isOpen: isMediaProcessingOpen, onClose: () => setIsMediaProcessingOpen(false) }}
          loadingFallback={null}
          failureTitle="Media processing unavailable"
          failureMessage="HyperStream couldn’t load media processing. Retry to recover post-download tooling."
          renderFailure={renderModalLoadFailure({
            title: "Media processing unavailable",
            message: "HyperStream couldn’t load media processing. Retry to recover post-download tooling.",
            onClose: () => setIsMediaProcessingOpen(false),
          })}
        />
      )}
      {isIpfsOpen && (
        <RecoverableLazy
          loader={loadIpfsDownloadModal}
          resolve={resolveIpfsDownloadModal}
          componentProps={{ isOpen: isIpfsOpen, onClose: () => setIsIpfsOpen(false) }}
          loadingFallback={null}
          failureTitle="IPFS download unavailable"
          failureMessage="HyperStream couldn’t load the IPFS modal. Retry to recover decentralized download setup."
          renderFailure={renderModalLoadFailure({
            title: "IPFS download unavailable",
            message: "HyperStream couldn’t load the IPFS modal. Retry to recover decentralized download setup.",
            onClose: () => setIsIpfsOpen(false),
          })}
        />
      )}

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
