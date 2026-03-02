/**
 * Custom hook that extracts all download action handlers from DownloadItem.
 * Each handler is memoized with useCallback for performance.
 * Replaces ~25 inline async handlers with clean, testable functions.
 */
import { useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import type { DownloadTask } from '../components/DownloadItem';
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
} from '../api/commands';

export function useDownloadActions(task: DownloadTask, filePath: string) {
    const toast = useToast();

    const handleScrubMetadata = useCallback(async () => {
        try {
            const result = await invoke<ScrubResult>('scrub_metadata', { path: filePath });
            toast.success(`✅ Metadata scrubbed!\nRemoved: ${result.fields_removed.length} fields (${result.bytes_removed} bytes)`);
        } catch (err) { toast.error('Scrub failed: ' + err); }
    }, [filePath]);

    const handleEphemeralShare = useCallback(async () => {
        try {
            const result = await invoke<EphemeralShareResult>('start_ephemeral_share', { path: filePath, timeoutMins: 60 });
            navigator.clipboard?.writeText(result.url);
            return result.url;
        } catch (err) { toast.error('Share failed: ' + err); return null; }
    }, [filePath]);

    const handleSandbox = useCallback(async () => {
        try {
            const result = await invoke<string>('run_in_sandbox', { path: filePath });
            toast.success(`🛡️ ${result}`);
        } catch (err) { toast.error('Sandbox launch failed: ' + err); }
    }, [filePath]);

    const handleNotarize = useCallback(async () => {
        try {
            toast.info('📜 Submitting to Timestamp Authority...');
            const result = await invoke<NotarizeResult>('notarize_file', { path: filePath });
            toast.success(`📜 Notarized!\nSHA-256: ${result.hash}\nTSR saved: ${result.tsr_path}\nTimestamp: ${result.timestamp}`);
        } catch (err) { toast.error('Notarization failed: ' + err); }
    }, [filePath]);

    const handleFindMirrors = useCallback(async () => {
        try {
            toast.info('🔍 Searching for mirrors...');
            const result = await invoke<MirrorResult>('find_mirrors', { path: filePath });
            const mirrorList = result.mirrors?.map((m) => `${m.source}: ${m.url}`).join('\n') || 'None found';
            toast.success(`🔍 Found ${result.mirrors_found} mirror(s)\nSHA-256: ${result.sha256}\nMD5: ${result.md5}\n\n${mirrorList}`);
        } catch (err) { toast.error('Mirror search failed: ' + err); }
    }, [filePath]);

    const handleFlashToUsb = useCallback(async () => {
        try {
            const drives = await invoke<UsbDrive[]>('list_usb_drives');
            if (!drives || drives.length === 0) {
                toast.error('No USB drives found. Insert a USB drive and try again.');
                return;
            }
            const driveList = drives.map((d) => `Drive ${d.number}: ${d.model} (${d.size_display})`).join('\n');
            const choice = prompt(`⚡ Select USB drive to flash:\n\n${driveList}\n\n⚠️ WARNING: ALL DATA WILL BE ERASED!\n\nEnter drive number:`);
            if (choice === null) return;
            const driveNum = parseInt(choice);
            if (isNaN(driveNum)) { toast.error('Invalid drive number'); return; }
            if (!confirm(`⚠️ FINAL WARNING: This will ERASE ALL DATA on Drive ${driveNum}. Continue?`)) return;
            const result = await invoke<string>('flash_to_usb', { isoPath: filePath, driveNumber: driveNum });
            toast.info(`⚡ ${result}`);
        } catch (err) { toast.error('Flash failed: ' + err); }
    }, [filePath]);

    const handleValidateC2pa = useCallback(async () => {
        try {
            const result = await invoke<C2PAResult>('validate_c2pa', { path: filePath });
            toast.info(`${result.description}\n\nJUMBF: ${result.has_jumbf_manifest}\nXMP C2PA: ${result.has_xmp_c2pa}\nAdobe: ${result.has_adobe_provenance}`);
        } catch (err) { toast.error('C2PA validation failed: ' + err); }
    }, [filePath]);

    const handleApiFuzz = useCallback(async () => {
        try {
            toast.info('🔧 Fuzzing URL for alternate endpoints...');
            const result = await invoke<ApiFuzzResult>('fuzz_url', { url: task.url });
            const hits = result.results?.filter((r) => r.status >= 200 && r.status < 400);
            const hitList = hits?.slice(0, 10).map((r) => `[${r.status}] ${r.url}`).join('\n') || 'None';
            toast.success(`🔧 API Fuzz Complete\nTested: ${result.total_tested}\nHits: ${hits?.length || 0}\n\n${hitList}`);
        } catch (err) { toast.error('API Fuzz failed: ' + err); }
    }, [task.url]);

    const handleStegoHide = useCallback(async () => {
        const secret = prompt('Enter secret message to hide:');
        if (!secret) return;
        try {
            const result = await invoke<StegoResult>('stego_hide', { imagePath: filePath, secretData: secret });
            toast.success(`🔒 Secret hidden!\nOutput: ${result.output_path}\nBits used: ${result.bits_used}`);
        } catch (err) { toast.error('Stego hide failed: ' + err); }
    }, [filePath]);

    const handleStegoExtract = useCallback(async () => {
        try {
            const result = await invoke<StegoExtractResult>('stego_extract', { imagePath: filePath });
            toast.success(`🔓 Secret extracted!\n\n${result.message}`);
        } catch (err) { toast.error('Stego extract failed: ' + err); }
    }, [filePath]);

    const handleAutoExtract = useCallback(async () => {
        try {
            toast.info('📦 Extracting archive...');
            const result = await invoke<ExtractResult>('auto_extract_archive', { path: filePath, destination: null });
            toast.success(`📦 Extracted ${result.files_extracted} files to:\n${result.destination}`);
        } catch (err) { toast.error('Extract failed: ' + err); }
    }, [filePath]);

    const handleSqlQuery = useCallback(async () => {
        const sql = prompt('Enter SQL query:\n\nExample: SELECT * FROM file WHERE column > 10 LIMIT 20');
        if (!sql) return;
        try {
            const result = await invoke<SqlQueryResult>('query_file', { path: filePath, sql });
            const preview = JSON.stringify(result.rows?.slice(0, 5), null, 2);
            toast.info(`📊 Query Results\nTotal: ${result.total_rows} rows\nColumns: ${result.columns?.join(', ')}\n\nFirst 5:\n${preview}`);
        } catch (err) { toast.error('Query failed: ' + err); }
    }, [filePath]);

    const handleDlnaCast = useCallback(async () => {
        try {
            const devices = await invoke<DlnaDevice[]>('discover_dlna');
            if (!devices || devices.length === 0) {
                toast.error('No DLNA devices found on your network.');
                return;
            }
            const list = devices.map((d, i) => `${i + 1}. ${d.name}`).join('\n');
            const choice = prompt(`📺 Select device:\n\n${list}\n\nEnter number:`);
            if (!choice) return;
            const idx = parseInt(choice) - 1;
            if (idx < 0 || idx >= devices.length) { toast.error('Invalid choice'); return; }
            const result = await invoke<string>('cast_to_dlna', { filePath: filePath, deviceLocation: devices[idx].location });
            toast.info(`📺 ${result}`);
        } catch (err) { toast.error('Cast failed: ' + err); }
    }, [filePath]);

    const handleGenerateSubtitles = useCallback(async () => {
        try {
            toast.info('🎬 Generating subtitles...');
            const result = await invoke<SubtitleResult>('generate_subtitles', { videoPath: filePath });
            toast.success(`🎬 Subtitles ${result.status}!\nMethod: ${result.method}\nSRT: ${result.srt_path}\nSegments: ${result.subtitle_lines}${result.note ? '\n\nNote: ' + result.note : ''}`);
        } catch (err) { toast.error('Subtitle generation failed: ' + err); }
    }, [filePath]);

    const handleSetPriority = useCallback(async () => {
        const level = prompt('Set priority:\n\ncritical - Max speed\nhigh - 75%\nnormal - 50% (default)\nlow - 25%\nbackground - 10%\n\nEnter level:');
        if (!level) return;
        try {
            const result = await invoke<string>('set_download_priority', { id: task.id, level });
            toast.info(`⚡ ${result}`);
        } catch (err) { toast.error('QoS failed: ' + err); }
    }, [task.id]);

    const handleAiUpscale = useCallback(async () => {
        try {
            toast.info('✨ AI Upscaling Started (Mock Real-ESRGAN)...');
            const result = await invoke<UpscaleResult>('upscale_image', { path: filePath });
            if (result.success) {
                toast.success(`✨ Success! Saved to: ${result.upscaled_path}`);
            } else {
                toast.error(`❌ Upscale failed: ${result.message}`);
            }
        } catch (err) {
            toast.error('AI Upscale Error: ' + err);
        }
    }, [filePath]);

    const handleOptimizeMods = useCallback(async () => {
        try {
            toast.info('🎮 Scanning for duplicates...');
            const result = await invoke<ModOptimizerResult>('optimize_mods', { paths: [filePath] });
            toast.success(`🎮 Scan Complete!\nFiles: ${result.total_files}\nDuplicate Groups: ${result.duplicate_groups}\nWasted: ${result.wasted_mb?.toFixed(1)} MB`);
        } catch (err) { toast.error('Optimize failed: ' + err); }
    }, [filePath]);

    const handleRefreshUrl = useCallback(async () => {
        const newUrl = prompt("Enter the new URL to refresh this download:");
        if (newUrl && newUrl.trim() !== "") {
            try {
                await invoke('refresh_download_url', { id: task.id, newUrl: newUrl.trim() });
                toast.success('✅ Download URL refreshed successfully. Click Resume to retry.');
            } catch (err) { toast.error('Refresh failed: ' + err); }
        }
    }, [task.id]);

    const handleWaybackCheck = useCallback(async () => {
        try {
            const snapshot = await invoke<WaybackSnapshot | null>('check_wayback_availability', { url: task.url });
            if (snapshot) {
                const downloadUrl = await invoke<string>('get_wayback_url', { waybackUrl: snapshot.url });
                if (confirm(`Found in Wayback Machine!\n\nArchived: ${snapshot.timestamp}\n\nUse archived URL to retry download?`)) {
                    await invoke('refresh_download_url', { id: task.id, newUrl: downloadUrl });
                    toast.success('✅ URL refreshed with Wayback archive. Click Resume to retry.');
                }
            } else {
                toast.error('❌ No archived version found in the Wayback Machine.');
            }
        } catch (err) { toast.error('Wayback check failed: ' + err); }
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
        handleStegoHide,
        handleStegoExtract,
        handleAutoExtract,
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
