import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import type { DownloadTask } from '../types';
import type {
    WaybackSnapshot,
} from '../types';

export function useDownloadActions(task: DownloadTask) {
    const toast = useToast();
    const toastRef = useRef(toast);
    toastRef.current = toast;

    const handleSetPriority = useCallback(async () => {
        const level = prompt('Set priority:\n\ncritical - Max speed\nhigh - 75%\nnormal - 50% (default)\nlow - 25%\nbackground - 10%\n\nEnter level:');
        if (!level) return;
        try {
            const result = await invoke<string>('set_download_priority', { id: task.id, level });
            toastRef.current.info(`⚡ ${result}`);
        } catch (err) { toastRef.current.error('QoS failed: ' + err); }
    }, [task.id]);

    const handleRefreshUrl = useCallback(async () => {
        const newUrl = prompt("Enter the new URL to refresh this download:");
        if (newUrl && newUrl.trim() !== "") {
            try {
                await invoke('refresh_download_url', { id: task.id, newUrl: newUrl.trim() });
                toastRef.current.success('✅ Download URL refreshed successfully. Click Resume to retry.');
            } catch (err) { toastRef.current.error('Refresh failed: ' + err); }
        }
    }, [task.id]);

    const handleWaybackCheck = useCallback(async () => {
        try {
            const snapshot = await invoke<WaybackSnapshot | null>('check_wayback_availability', { url: task.url });
            if (snapshot) {
                const downloadUrl = await invoke<string>('get_wayback_url', { waybackUrl: snapshot.url });
                if (confirm(`Found in Wayback Machine!\n\nArchived: ${snapshot.timestamp}\n\nUse archived URL to retry download?`)) {
                    await invoke('refresh_download_url', { id: task.id, newUrl: downloadUrl });
                    toastRef.current.success('✅ URL refreshed with Wayback archive. Click Resume to retry.');
                }
            } else {
                toastRef.current.error('❌ No archived version found in the Wayback Machine.');
            }
        } catch (err) { toastRef.current.error('Wayback check failed: ' + err); }
    }, [task.id, task.url]);

    return {
        handleSetPriority,
        handleRefreshUrl,
        handleWaybackCheck,
    };
}
