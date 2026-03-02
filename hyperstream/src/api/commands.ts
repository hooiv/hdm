/**
 * Typed API layer for all Tauri backend commands.
 * Provides full TypeScript type safety and IntelliSense for every invoke() call.
 * 
 * Usage: import { api } from '@/api/commands';
 *        const result = await api.findMirrors('/path/to/file');
 *        // result is fully typed as MirrorResult
 */
import { invoke } from '@tauri-apps/api/core';

// ============ Response Types ============

export interface ScrubResult {
    output_path: string;
    fields_removed: string[];
    bytes_removed: number;
}

export interface EphemeralShareResult {
    url: string;
    expires_at: string;
}

export interface NotarizeResult {
    hash: string;
    tsr_path: string;
    timestamp: string;
}

export interface MirrorResult {
    mirrors_found: number;
    sha256: string;
    md5: string;
    mirrors: { url: string; source: string }[];
}

export interface C2PAResult {
    description: string;
    has_jumbf_manifest: boolean;
    has_xmp_c2pa: boolean;
    has_adobe_provenance: boolean;
}

export interface StegoResult {
    output_path: string;
    bits_used: number;
}

export interface StegoExtractResult {
    message: string;
}

export interface ExtractResult {
    files_extracted: number;
    destination: string;
}

export interface SqlQueryResult {
    columns: string[];
    rows: Record<string, string>[];
    total_rows: number;
}

export interface SubtitleResult {
    status: string;
    method: string;
    srt_path: string;
    subtitle_lines: number;
    note?: string;
}

export interface QosStats {
    downloads: Record<string, { priority: string; bandwidth_limit: number }>;
    global_limit: number;
}

export interface ModOptimizerResult {
    total_files: number;
    duplicate_groups: number;
    wasted_bytes: number;
    wasted_mb: number;
    duplicates: Record<string, string[]>;
}

export interface DlnaDevice {
    name: string;
    location: string;
    device_type: string;
}

export interface UsbDrive {
    number: number;
    model: string;
    size_display: string;
}

export interface MountedDrive {
    letter: string;
    path: string;
    status: string;
}

export interface GeofenceRule {
    id: string;
    url_pattern: string;
    region: string;
    proxy_type: string;
    proxy_address: string;
    enabled: boolean;
}

export interface WaybackResult {
    available: boolean;
    closest_url?: string;
    timestamp?: string;
}

export interface ApiFuzzResult {
    total_tested: number;
    successful: number;
    results: { url: string; status: number; size: number }[];
}

export interface BandwidthArbResult {
    fastest_url: string;
    fastest_speed: number;
    results: { url: string; speed: number; status: string }[];
}

// ============ Typed API Commands ============

export const api = {
    // --- Core Downloads ---
    startDownload: (id: string, url: string, path: string, force?: boolean, customHeaders?: Record<string, string>) =>
        invoke<void>('start_download', { id, url, path, force, customHeaders }),
    pauseDownload: (id: string) =>
        invoke<void>('pause_download', { id }),
    getDownloads: () =>
        invoke<import('../types').SavedDownload[]>('get_downloads'),
    removeDownload: (id: string) =>
        invoke<void>('remove_download_entry', { id }),

    // --- Settings ---
    getSettings: () =>
        invoke<import('../types').AppSettings>('get_settings'),
    saveSettings: (json: Record<string, unknown>) =>
        invoke<void>('save_settings', { json }),

    // --- File Operations ---
    openFile: (path: string) =>
        invoke<void>('open_file', { path }),
    openFolder: (path: string) =>
        invoke<void>('open_folder', { path }),

    // --- Security & Privacy ---
    scrubMetadata: (path: string) =>
        invoke<ScrubResult>('scrub_metadata', { path }),
    notarizeFile: (path: string) =>
        invoke<NotarizeResult>('notarize_file', { path }),
    validateC2pa: (path: string) =>
        invoke<C2PAResult>('validate_c2pa', { path }),
    runInSandbox: (path: string) =>
        invoke<string>('run_in_sandbox', { path }),
    stegoHide: (imagePath: string, secretData: string) =>
        invoke<StegoResult>('stego_hide', { imagePath, secretData }),
    stegoExtract: (imagePath: string) =>
        invoke<StegoExtractResult>('stego_extract', { imagePath }),

    // --- Network & Discovery ---
    findMirrors: (path: string) =>
        invoke<MirrorResult>('find_mirrors', { path }),
    startEphemeralShare: (path: string, timeoutMins: number) =>
        invoke<EphemeralShareResult>('start_ephemeral_share', { path, timeoutMins }),
    checkWayback: (url: string) =>
        invoke<WaybackResult>('check_wayback', { url }),
    apiFuzzUrl: (url: string) =>
        invoke<ApiFuzzResult>('fuzz_url', { url }),
    bandwidthArbitrage: (urls: string[]) =>
        invoke<BandwidthArbResult>('bandwidth_arbitrage', { urls }),

    // --- Media ---
    generateSubtitles: (videoPath: string) =>
        invoke<SubtitleResult>('generate_subtitles', { videoPath }),
    discoverDlna: () =>
        invoke<DlnaDevice[]>('discover_dlna'),
    castToDlna: (filePath: string, deviceLocation: string) =>
        invoke<string>('cast_to_dlna', { filePath, deviceLocation }),

    // --- Archives ---
    autoExtract: (path: string, destination: string | null) =>
        invoke<ExtractResult>('auto_extract_archive', { path, destination }),
    flashToUsb: (isoPath: string, driveNumber: number) =>
        invoke<string>('flash_to_usb', { isoPath, driveNumber }),
    listUsbDrives: () =>
        invoke<UsbDrive[]>('list_usb_drives'),

    // --- Data Query ---
    queryFile: (path: string, sql: string) =>
        invoke<SqlQueryResult>('query_file', { path, sql }),

    // --- QoS ---
    setDownloadPriority: (id: string, level: string) =>
        invoke<string>('set_download_priority', { id, level }),
    getQosStats: () =>
        invoke<QosStats>('get_qos_stats'),

    // --- Mod Optimizer ---
    optimizeMods: (paths: string[]) =>
        invoke<ModOptimizerResult>('optimize_mods', { paths }),

    // --- Cloud ---
    rcloneListRemotes: () =>
        invoke<string[]>('rclone_list_remotes'),
    rcloneTransfer: (source: string, destination: string) =>
        invoke<string>('rclone_transfer', { source, destination }),

    // --- Virtual Drive ---
    mountDrive: (path: string, letter: string) =>
        invoke<string>('mount_drive', { path, letter }),
    unmountDrive: (letter: string) =>
        invoke<string>('unmount_drive', { letter }),
    listVirtualDrives: () =>
        invoke<MountedDrive[]>('list_virtual_drives'),

    // --- Geofence ---
    setGeofenceRule: (urlPattern: string, region: string, proxyType: string, proxyAddress: string) =>
        invoke<string>('set_geofence_rule', { urlPattern, region, proxyType, proxyAddress }),
    getGeofenceRules: () =>
        invoke<GeofenceRule[]>('get_geofence_rules'),

    // --- Tor ---
    initTor: () =>
        invoke<number>('init_tor_network'),
    getTorStatus: () =>
        invoke<number | null>('get_tor_status'),

    // --- TUI ---
    launchTuiDashboard: () =>
        invoke<string>('launch_tui_dashboard'),

    // --- IPFS ---
    downloadIpfs: (cid: string, outputPath: string) =>
        invoke<string>('download_ipfs', { cid, outputPath }),
};
