/**
 * Custom hook that extracts all download action handlers from DownloadItem.
 * Each handler is memoized with useCallback for performance.
 * Replaces ~25 inline async handlers with clean, testable functions.
 */
import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import { formatBytes } from '../utils/formatters';
import type { DownloadTask } from '../types';
import type {
    WaybackSnapshot,
    UpscaleResult,
} from '../types';
import type {
    ScrubResult,
    EphemeralShareResult,
    NotarizeResult,
    MirrorResult,
    C2PAResult,
    StegoResult,
    StegoExtractResult,
    ExtractResult,
    SqlQueryResult,
    SubtitleResult,
    ModOptimizerResult,
    DlnaDevice,
    UsbDrive,
    ApiFuzzResult,
    ReplayResult,
} from '../api/commands';

export function useDownloadActions(task: DownloadTask, filePath: string) {
    const toast = useToast();
    // Keep a stable ref to toast so callbacks never capture a stale toast instance
    // without needing toast in every dependency array.
    const toastRef = useRef(toast);
    toastRef.current = toast;

    const handleScrubMetadata = useCallback(async () => {
        try {
            const result = await invoke<ScrubResult>('scrub_metadata', { path: filePath });
            toastRef.current.success(`✅ Metadata scrubbed!\nRemoved: ${result.fields_removed.length} fields (${result.bytes_removed} bytes)`);
        } catch (err) { toastRef.current.error('Scrub failed: ' + err); }
    }, [filePath]);

    const handleEphemeralShare = useCallback(async () => {
        try {
            const result = await invoke<EphemeralShareResult>('start_ephemeral_share', { path: filePath, timeoutMins: 60 });
            navigator.clipboard?.writeText(result.url);
            return result.url;
        } catch (err) { toastRef.current.error('Share failed: ' + err); return null; }
    }, [filePath]);

    const handleSandbox = useCallback(async () => {
        try {
            const result = await invoke<string>('run_in_sandbox', { path: filePath });
            toastRef.current.success(`🛡️ ${result}`);
        } catch (err) { toastRef.current.error('Sandbox launch failed: ' + err); }
    }, [filePath]);

    const handleNotarize = useCallback(async () => {
        try {
            toastRef.current.info('📜 Submitting to Timestamp Authority...');
            const result = await invoke<NotarizeResult>('notarize_file', { path: filePath });
            toastRef.current.success(`📜 Notarized!\nSHA-256: ${result.hash}\nTSR saved: ${result.tsr_path}\nTimestamp: ${result.timestamp}`);
        } catch (err) { toastRef.current.error('Notarization failed: ' + err); }
    }, [filePath]);

    const handleFindMirrors = useCallback(async () => {
        try {
            toastRef.current.info('🔍 Searching for mirrors...');
            const result = await invoke<MirrorResult>('find_mirrors', { path: filePath });
            const mirrorList = result.mirrors?.map((m) => `${m.source}: ${m.url}`).join('\n') || 'None found';
            toastRef.current.success(`🔍 Found ${result.mirrors_found} mirror(s)\nSHA-256: ${result.sha256}\nMD5: ${result.md5}\n\n${mirrorList}`);
        } catch (err) { toastRef.current.error('Mirror search failed: ' + err); }
    }, [filePath]);

    const handleFlashToUsb = useCallback(async () => {
        try {
            const drives = await invoke<UsbDrive[]>('list_usb_drives');
            if (!drives || drives.length === 0) {
                toastRef.current.error('No USB drives found. Insert a USB drive and try again.');
                return;
            }
            const driveList = drives.map((d) => `Drive ${d.number}: ${d.model} (${d.size_display})`).join('\n');
            const choice = prompt(`⚡ Select USB drive to flash:\n\n${driveList}\n\n⚠️ WARNING: ALL DATA WILL BE ERASED!\n\nEnter drive number:`);
            if (choice === null) return;
            const driveNum = parseInt(choice);
            if (isNaN(driveNum)) { toastRef.current.error('Invalid drive number'); return; }
            if (!confirm(`⚠️ FINAL WARNING: This will ERASE ALL DATA on Drive ${driveNum}. Continue?`)) return;
            const result = await invoke<string>('flash_to_usb', { isoPath: filePath, driveNumber: driveNum });
            toastRef.current.info(`⚡ ${result}`);
        } catch (err) { toastRef.current.error('Flash failed: ' + err); }
    }, [filePath]);

    const handleValidateC2pa = useCallback(async () => {
        try {
            const result = await invoke<C2PAResult>('validate_c2pa', { path: filePath });
            toastRef.current.info(`${result.description}\n\nJUMBF: ${result.has_jumbf_manifest}\nXMP C2PA: ${result.has_xmp_c2pa}\nAdobe: ${result.has_adobe_provenance}`);
        } catch (err) { toastRef.current.error('C2PA validation failed: ' + err); }
    }, [filePath]);

    const handleApiFuzz = useCallback(async () => {        if (!task.url) {
            toastRef.current.error('No URL available for API fuzzing');
            return;
        }        try {
            toastRef.current.info('Fuzzing URL for alternate endpoints...');
            const result = await invoke<ApiFuzzResult>('fuzz_url', { url: task.url });
            const interesting = result.mutations?.filter((m) => m.interesting) || [];
            const hitList = interesting.slice(0, 10).map((m) => `[${m.status_code}] ${m.mutated_url}`).join('\n') || 'None';
            toastRef.current.success(`API Fuzz Complete\nTested: ${result.mutations?.length || 0}\nInteresting: ${interesting.length}\n\n${hitList}`);
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
                .map(([k, v]) => `  ${k}: ${v}`).join('\n');
            toastRef.current.success(
                `HTTP ${result.status_code} — ${result.response_time_ms}ms — ${result.body_size} bytes\n\nHeaders:\n${headerLines}\n\nBody preview:\n${result.body_preview.slice(0, 200)}`
            );
        } catch (err) { toastRef.current.error('HTTP Replay failed: ' + err); }
    }, [task.url]);

    const handleStegoHide = useCallback(async () => {
        const secret = prompt('Enter secret message to hide:');
        if (!secret) return;
        try {
            const result = await invoke<StegoResult>('stego_hide', { imagePath: filePath, secretData: secret });
            toastRef.current.success(`🔒 Secret hidden!\nOutput: ${result.output_path}\nBits used: ${result.bits_used}`);
        } catch (err) { toastRef.current.error('Stego hide failed: ' + err); }
    }, [filePath]);

    const handleStegoExtract = useCallback(async () => {
        try {
            const result = await invoke<StegoExtractResult>('stego_extract', { imagePath: filePath });
            toastRef.current.success(`🔓 Secret extracted!\n\n${result.message}`);
        } catch (err) { toastRef.current.error('Stego extract failed: ' + err); }
    }, [filePath]);

    const handleAutoExtract = useCallback(async () => {
        try {
            toastRef.current.info('📦 Extracting archive...');
            const result = await invoke<ExtractResult>('auto_extract_archive', { path: filePath, destination: null });
            toastRef.current.success(`📦 Extracted ${result.files_extracted} files to:\n${result.destination}`);
        } catch (err) { toastRef.current.error('Extract failed: ' + err); }
    }, [filePath]);

    const handleVerifyChecksum = useCallback(async () => {
        const expected = prompt('Enter expected checksum (sha256:abc... or md5:abc... or plain hash):');
        if (!expected) {
            // No expected hash — compute all checksums
            try {
                toastRef.current.info('Computing checksums...');
                const results = await invoke<{ algorithm: string; hash: string; file_size: number }[]>('compute_file_checksums', { path: filePath });
                const lines = results.map(r => `${r.algorithm}: ${r.hash}`).join('\n');
                toastRef.current.success(`File Checksums (${formatBytes(results[0]?.file_size ?? 0)}):\n\n${lines}`);
            } catch (err) { toastRef.current.error('Checksum failed: ' + err); }
            return;
        }
        try {
            toastRef.current.info('Verifying checksum...');
            const result = await invoke<{ algorithm: string; hash: string; verified: boolean | null; expected: string | null }>('verify_download_checksum', { path: filePath, expected });
            if (result.verified) {
                toastRef.current.success(`✅ Checksum VERIFIED\n${result.algorithm}: ${result.hash}`);
            } else {
                toastRef.current.error(`❌ Checksum MISMATCH\n${result.algorithm}\nExpected: ${result.expected}\nActual:   ${result.hash}`);
            }
        } catch (err) { toastRef.current.error('Checksum verification failed: ' + err); }
    }, [filePath]);

    const handleSqlQuery = useCallback(async () => {
        const sql = prompt('Enter SQL query:\n\nExample: SELECT * FROM file WHERE column > 10 LIMIT 20');
        if (!sql) return;
        try {
            const result = await invoke<SqlQueryResult>('query_file', { path: filePath, sql });
            const preview = JSON.stringify(result.rows?.slice(0, 5), null, 2);
            toastRef.current.info(`📊 Query Results\nTotal: ${result.total_rows} rows\nColumns: ${result.columns?.join(', ')}\n\nFirst 5:\n${preview}`);
        } catch (err) { toastRef.current.error('Query failed: ' + err); }
    }, [filePath]);

    const handleDlnaCast = useCallback(async () => {
        try {
            const devices = await invoke<DlnaDevice[]>('discover_dlna');
            if (!devices || devices.length === 0) {
                toastRef.current.error('No DLNA devices found on your network.');
                return;
            }
            const list = devices.map((d, i) => `${i + 1}. ${d.name}`).join('\n');
            const choice = prompt(`📺 Select device:\n\n${list}\n\nEnter number:`);
            if (!choice) return;
            const idx = parseInt(choice) - 1;
            if (idx < 0 || idx >= devices.length) { toastRef.current.error('Invalid choice'); return; }
            const result = await invoke<string>('cast_to_dlna', { filePath: filePath, deviceLocation: devices[idx].location });
            toastRef.current.info(`📺 ${result}`);
        } catch (err) { toastRef.current.error('Cast failed: ' + err); }
    }, [filePath]);

    const handleGenerateSubtitles = useCallback(async () => {
        try {
            toastRef.current.info('🎬 Generating subtitles...');
            const result = await invoke<SubtitleResult>('generate_subtitles', { videoPath: filePath });
            toastRef.current.success(`🎬 Subtitles ${result.status}!\nMethod: ${result.method}\nSRT: ${result.srt_path}\nSegments: ${result.subtitle_lines}${result.note ? '\n\nNote: ' + result.note : ''}`);
        } catch (err) { toastRef.current.error('Subtitle generation failed: ' + err); }
    }, [filePath]);

    const handleSetPriority = useCallback(async () => {
        const level = prompt('Set priority:\n\ncritical - Max speed\nhigh - 75%\nnormal - 50% (default)\nlow - 25%\nbackground - 10%\n\nEnter level:');
        if (!level) return;
        try {
            const result = await invoke<string>('set_download_priority', { id: task.id, level });
            toastRef.current.info(`⚡ ${result}`);
        } catch (err) { toastRef.current.error('QoS failed: ' + err); }
    }, [task.id]);

    const handleAiUpscale = useCallback(async () => {
        try {
            toastRef.current.info('✨ AI Upscaling Started (Mock Real-ESRGAN)...');
            const result = await invoke<UpscaleResult>('upscale_image', { path: filePath });
            if (result.success) {
                toastRef.current.success(`✨ Success! Saved to: ${result.upscaled_path}`);
            } else {
                toastRef.current.error(`❌ Upscale failed: ${result.message}`);
            }
        } catch (err) {
            toastRef.current.error('AI Upscale Error: ' + err);
        }
    }, [filePath]);

    const handleOptimizeMods = useCallback(async () => {
        try {
            toastRef.current.info('🎮 Scanning for duplicates...');
            const result = await invoke<ModOptimizerResult>('optimize_mods', { paths: [filePath] });
            toastRef.current.success(`🎮 Scan Complete!\nFiles: ${result.total_files}\nDuplicate Groups: ${result.duplicate_groups}\nWasted: ${result.wasted_mb?.toFixed(1)} MB`);
        } catch (err) { toastRef.current.error('Optimize failed: ' + err); }
    }, [filePath]);

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
        handleScrubMetadata,
        handleEphemeralShare,
        handleSandbox,
        handleNotarize,
        handleFindMirrors,
        handleFlashToUsb,
        handleValidateC2pa,
        handleApiFuzz,
        handleApiReplay,
        handleStegoHide,
        handleStegoExtract,
        handleAutoExtract,
        handleVerifyChecksum,
        handleSqlQuery,
        handleDlnaCast,
        handleGenerateSubtitles,
        handleSetPriority,
        handleOptimizeMods,
        handleRefreshUrl,
        handleWaybackCheck,
        handleAiUpscale,
    };
}
