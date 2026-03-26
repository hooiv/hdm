/**
 * Production-Grade Progress Update Hook
 * 
 * Prevents the 30fps progress update storm from forcing full re-renders.
 * Uses a debounce strategy: collect updates into a batch, then apply once per frame.
 * 
 * This is critical for smooth UI at 100+ downloads.
 */

import { useEffect, useRef, useCallback } from 'react';
import { useDownloadStore } from '../stores/appStore';
import { safeListen } from '../utils/tauri';
import type { DownloadProgressPayload } from '../types';

const UPDATE_BATCH_INTERVAL_MS = 16; // 60fps = 16ms per frame

interface PendingUpdate {
  taskId: string;
  payload: DownloadProgressPayload;
}

/**
 * Hook to efficiently handle download progress updates
 * Batches updates and applies them once per frame to prevent re-render thrashing
 */
export function useProgressUpdates() {
  const updateTaskProgress = useDownloadStore((state) => state.updateTaskProgress);
  const updateBatchRef = useRef<Map<string, PendingUpdate>>(new Map());
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  const flushUpdates = useCallback(() => {
    const batch = updateBatchRef.current;
    if (batch.size === 0) return;

    // Apply all batched updates at once
    batch.forEach(({ taskId, payload }) => {
      const progress = payload.total > 0 ? (payload.downloaded / payload.total) * 100 : 0;
      updateTaskProgress(taskId, {
        downloaded: payload.downloaded,
        total: payload.total,
        progress,
      });
    });

    batch.clear();
  }, [updateTaskProgress]);

  const addUpdate = useCallback((taskId: string, payload: DownloadProgressPayload) => {
    const batch = updateBatchRef.current;
    batch.set(taskId, { taskId, payload });

    // Schedule flush if not already scheduled
    if (!timeoutRef.current) {
      timeoutRef.current = setTimeout(() => {
        flushUpdates();
        timeoutRef.current = null;
      }, UPDATE_BATCH_INTERVAL_MS);
    }
  }, [flushUpdates]);

  // Clean up on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      flushUpdates();
    };
  }, [flushUpdates]);

  return { addUpdate };
}

/**
 * Initialize progress event listener
 * Integrates with the batched update system for efficient rendering
 */
export function useProgressEventListener() {
  const { addUpdate } = useProgressUpdates();

  useEffect(() => {
    const unlistenPromise = safeListen<DownloadProgressPayload>(
      'download_progress',
      (event: { payload: DownloadProgressPayload }) => {
        const payload = event.payload;
        addUpdate(payload.id, payload);
      }
    );

    return () => {
      unlistenPromise.then((f: () => void) => f());
    };
  }, [addUpdate]);
}
