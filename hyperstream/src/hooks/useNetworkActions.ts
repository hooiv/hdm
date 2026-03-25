import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import { formatBytes } from '../utils/formatters';
import type { DiscoveredMirror, DownloadTask } from '../types';
import type {
    WaybackSnapshot,
    MirrorResult,
    ApiFuzzResult,
    ReplayResult,
} from '../api/commands';

interface UseNetworkActionsOptions {
    onDiscoveredMirrors?: (taskId: string, mirrors: DiscoveredMirror[]) => void;
}

export function useNetworkActions(task: DownloadTask, options: UseNetworkActionsOptions = {}) {
    const toast = useToast();
    const toastRef = useRef(toast);
    toastRef.current = toast;
    const { onDiscoveredMirrors } = options;

    const handleFindMirrors = useCallback(async () => {
        try {
            toastRef.current.info('🔍 Searching for mirrors...');
            const result = await invoke<MirrorResult>('find_mirrors', { path: '' });
            const topMirrors = result.mirrors.slice(0, 5).map((m, index) => {
                const badges = [m.direct ? 'direct' : 'discovery', m.confidence];
                if (m.hostname) badges.push(m.hostname);
                if (m.content_length) badges.push(formatBytes(m.content_length));
                if (m.supports_range) badges.push('range');
                return `${index + 1}. ${m.source} — ${badges.join(' — ')}${m.note ? `
   ${m.note}` : ''}
   ${m.url}`;
            }).join('
');

            toastRef.current.success(
                `🔍 Mirror intelligence complete
` +
                `File: ${result.filename} (${formatBytes(result.file_size)})
` +
                `Candidates: ${result.mirrors_found} • Direct: ${result.direct_mirrors_found} • Probe-ready: ${result.probe_ready_mirrors_found}
` +
                `SHA-256: ${result.sha256}

` +
                `${topMirrors || 'No mirror candidates found.'}`
            );

            const usableMirrors: DiscoveredMirror[] = result.mirrors
                .filter((m) => m.direct && m.probe_ready)
                .slice(0, 5)
                .map((m) => ({
                    url: m.url,
                    source: m.source,
                    confidence: m.confidence,
                    hostname: m.hostname || undefined,
                    supportsRange: m.supports_range ?? undefined,
                    note: m.note ?? undefined,
                }));

            onDiscoveredMirrors?.(task.id, usableMirrors);

            if (usableMirrors.length > 0) {
                toastRef.current.info(`🪞 Saved ${usableMirrors.length} mirror candidate${usableMirrors.length === 1 ? '' : 's'} for accelerated resume.`);
            } else {
                toastRef.current.info('🪞 No direct recovery-ready mirrors were found to save for resume.');
            }

            const probeCandidates = usableMirrors
                .map((m) => [m.url, m.source] as [string, string]);

            if (task.url && probeCandidates.length > 0 && confirm(`Found ${probeCandidates.length} direct mirror(s). Probe them against the current URL now?`)) {
                toastRef.current.info('📡 Probing discovered mirrors...');
                const probeResults = await invoke<{ url: string; source: string; latency_ms: number; supports_range: boolean; avg_speed_bps: number; disabled: boolean }[]>('probe_mirrors', {
                    primaryUrl: task.url,
                    mirrorUrls: probeCandidates,
                });
                const ranked = probeResults
                    .filter((m) => !m.disabled || m.avg_speed_bps > 0)
                    .sort((a, b) => b.avg_speed_bps - a.avg_speed_bps || a.latency_ms - b.latency_ms);
                const summary = ranked.slice(0, 5).map((m, index) =>
                    `${index === 0 ? '🏆' : '  '} ${m.source} — ${formatBytes(m.avg_speed_bps)}/s — ${m.latency_ms < 999999 ? `${m.latency_ms}ms` : '—'}${m.supports_range ? ' — range' : ''}`
                ).join('
');

                toastRef.current.success(`📡 Mirror probe complete
${summary || 'No mirrors responded during probe.'}`);
            }
        } catch (err) { toastRef.current.error('Mirror search failed: ' + err); }
    }, [onDiscoveredMirrors, task.id, task.url]);

    const handleApiFuzz = useCallback(async () => {
        if (!task.url) {
            toastRef.current.error('No URL available for API fuzzing');
            return;
        }
        try {
            toastRef.current.info('Fuzzing URL for alternate endpoints...');
            const result = await invoke<ApiFuzzResult>('fuzz_url', { url: task.url });
            const interesting = result.mutations?.filter((m) => m.interesting) || [];
            const hitList = interesting.slice(0, 10).map((m) => `[${m.status_code}] ${m.mutated_url}`).join('
') || 'None';
            toastRef.current.success(`API Fuzz Complete
Tested: ${result.mutations?.length || 0}
Interesting: ${interesting.length}

${hitList}`);
        } catch (err) { toastRef.current.error('API Fuzz failed: ' + err); }
    }, [task.url]);

    const handleApiReplay = useCallback(async () => {
        if (!task.url) {
            toastRef.current.error('No URL available for replay');
            return;
        }
        try {
            toastRef.current.info('Replaying HTTP request...');
            const result = await invoke<ReplayResult>('replay_request', {
                url: task.url,
                method: 'GET',
                headers: null,
                body: null,
            });
            const headerLines = Object.entries(result.headers).slice(0, 8)
                .map(([k, v]) => `  ${k}: ${v}`).join('
');
            toastRef.current.success(
                `HTTP ${result.status_code} — ${result.response_time_ms}ms — ${result.body_size} bytes

Headers:
${headerLines}

Body preview:
${result.body_preview.slice(0, 200)}`
            );
        } catch (err) { toastRef.current.error('HTTP Replay failed: ' + err); }
    }, [task.url]);

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
                if (confirm(`Found in Wayback Machine!

Archived: ${snapshot.timestamp}

Use archived URL to retry download?`)) {
                    await invoke('refresh_download_url', { id: task.id, newUrl: downloadUrl });
                    toastRef.current.success('✅ URL refreshed with Wayback archive. Click Resume to retry.');
                }
            } else {
                toastRef.current.error('❌ No archived version found in the Wayback Machine.');
            }
        } catch (err) { toastRef.current.error('Wayback check failed: ' + err); }
    }, [task.id, task.url]);

    return {
        handleFindMirrors,
        handleApiFuzz,
        handleApiReplay,
        handleRefreshUrl,
        handleWaybackCheck,
    };
}
