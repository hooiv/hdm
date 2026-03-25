import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import { formatBytes } from '../utils/formatters';
import type {
    ScrubResult,
    EphemeralShareResult,
    NotarizeResult,
    C2PAResult,
    StegoResult,
    StegoExtractResult,
    ExtractResult,
    SqlQueryResult,
    UsbDrive,
} from '../api/commands';

export function useFileActions(filePath: string) {
    const toast = useToast();
    const toastRef = useRef(toast);
    toastRef.current = toast;

    const handleScrubMetadata = useCallback(async () => {
        try {
            const result = await invoke<ScrubResult>('scrub_metadata', { path: filePath });
            toastRef.current.success(`✅ Metadata scrubbed!
Removed: ${result.fields_removed.length} fields (${result.bytes_removed} bytes)`);
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
            toastRef.current.success(`📜 Notarized!
SHA-256: ${result.hash}
TSR saved: ${result.tsr_path}
Timestamp: ${result.timestamp}`);
        } catch (err) { toastRef.current.error('Notarization failed: ' + err); }
    }, [filePath]);

    const handleFlashToUsb = useCallback(async () => {
        try {
            const drives = await invoke<UsbDrive[]>('list_usb_drives');
            if (!drives || drives.length === 0) {
                toastRef.current.error('No USB drives found. Insert a USB drive and try again.');
                return;
            }
            const driveList = drives.map((d) => `Drive ${d.number}: ${d.model} (${d.size_display})`).join('
');
            const choice = prompt(`⚡ Select USB drive to flash:

${driveList}

⚠️ WARNING: ALL DATA WILL BE ERASED!

Enter drive number:`);
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
            toastRef.current.info(`${result.description}

JUMBF: ${result.has_jumbf_manifest}
XMP C2PA: ${result.has_xmp_c2pa}
Adobe: ${result.has_adobe_provenance}`);
        } catch (err) { toastRef.current.error('C2PA validation failed: ' + err); }
    }, [filePath]);

    const handleStegoHide = useCallback(async () => {
        const secret = prompt('Enter secret message to hide:');
        if (!secret) return;
        try {
            const result = await invoke<StegoResult>('stego_hide', { imagePath: filePath, secretData: secret });
            toastRef.current.success(`🔒 Secret hidden!
Output: ${result.output_path}
Bits used: ${result.bits_used}`);
        } catch (err) { toastRef.current.error('Stego hide failed: ' + err); }
    }, [filePath]);

    const handleStegoExtract = useCallback(async () => {
        try {
            const result = await invoke<StegoExtractResult>('stego_extract', { imagePath: filePath });
            toastRef.current.success(`🔓 Secret extracted!

${result.message}`);
        } catch (err) { toastRef.current.error('Stego extract failed: ' + err); }
    }, [filePath]);

    const handleAutoExtract = useCallback(async () => {
        try {
            toastRef.current.info('📦 Extracting archive...');
            const result = await invoke<ExtractResult>('auto_extract_archive', { path: filePath, destination: null });
            toastRef.current.success(`📦 Extracted ${result.files_extracted} files to:
${result.destination}`);
        } catch (err) { toastRef.current.error('Extract failed: ' + err); }
    }, [filePath]);

    const handleVerifyChecksum = useCallback(async () => {
        const expected = prompt('Enter expected checksum (sha256:abc... or md5:abc... or plain hash):');
        if (!expected) {
            // No expected hash — compute all checksums
            try {
                toastRef.current.info('Computing checksums...');
                const results = await invoke<{ algorithm: string; hash: string; file_size: number }[]>('compute_file_checksums', { path: filePath });
                const lines = results.map(r => `${r.algorithm}: ${r.hash}`).join('
');
                toastRef.current.success(`File Checksums (${formatBytes(results[0]?.file_size ?? 0)}):

${lines}`);
            } catch (err) { toastRef.current.error('Checksum failed: ' + err); }
            return;
        }
        try {
            toastRef.current.info('Verifying checksum...');
            const result = await invoke<{ algorithm: string; hash: string; verified: boolean | null; expected: string | null }>('verify_download_checksum', { path: filePath, expected });
            if (result.verified) {
                toastRef.current.success(`✅ Checksum VERIFIED
${result.algorithm}: ${result.hash}`);
            } else {
                toastRef.current.error(`❌ Checksum MISMATCH
${result.algorithm}
Expected: ${result.expected}
Actual:   ${result.hash}`);
            }
        } catch (err) { toastRef.current.error('Checksum verification failed: ' + err); }
    }, [filePath]);

    const handleSqlQuery = useCallback(async () => {
        const sql = prompt('Enter SQL query:

Example: SELECT * FROM file WHERE column > 10 LIMIT 20');
        if (!sql) return;
        try {
            const result = await invoke<SqlQueryResult>('query_file', { path: filePath, sql });
            const preview = JSON.stringify(result.rows?.slice(0, 5), null, 2);
            toastRef.current.info(`📊 Query Results
Total: ${result.total_rows} rows
Columns: ${result.columns?.join(', ')}

First 5:
${preview}`);
        } catch (err) { toastRef.current.error('Query failed: ' + err); }
    }, [filePath]);

    return {
        handleScrubMetadata,
        handleEphemeralShare,
        handleSandbox,
        handleNotarize,
        handleFlashToUsb,
        handleValidateC2pa,
        handleStegoHide,
        handleStegoExtract,
        handleAutoExtract,
        handleVerifyChecksum,
        handleSqlQuery,
    };
}
