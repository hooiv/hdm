export interface GrabbedFile {
    url: string;
    filename: string;
    file_type: string;
    size: number | null;
}

export interface SpiderOptions {
    url: string;
    max_depth: number;
    extensions: string[];
}

export interface TorrentStatus {
    id: number; // usize -> number
    name: string;
    progress_percent: number;
    speed_download: number; // u64 -> number
    speed_upload: number; // u64 -> number
    peers: number;
    state: string;
}

export interface Segment {
    id: number;
    start_byte: number;
    end_byte: number;
    downloaded_cursor: number;
    state: string; // "Idle" | "Downloading" | "Paused" | "Complete" | "Error"
    speed_bps: number;
}

// [id, start, end, cursor, state(0-4), speed]
export type SlimSegment = [number, number, number, number, number, number];

/** Matches the Rust SavedDownload struct from persistence.rs */
export interface SavedDownload {
    id: string;
    url: string;
    path: string;
    filename: string;
    total_size: number;
    downloaded_bytes: number;
    status: string; // Rust sends "Paused" | "Error" | "Done" | "Downloading"
}

/** Safely coerce a backend status string to a DownloadTask status union */
export function toTaskStatus(s: string): DownloadTask['status'] {
    if (s === 'Downloading' || s === 'Paused' || s === 'Error' || s === 'Done') return s;
    if (s === 'Complete' || s === 'Completed') return 'Done'; // backend persists "Complete"/"Completed", UI uses "Done"
    return 'Paused'; // safe default for unknown values
}

/** Matches the Rust Settings struct from settings.rs */
export interface AppSettings {
    download_dir: string;
    segments: number;
    speed_limit_kbps: number;
    clipboard_monitor: boolean;
    auto_start_extension: boolean;
    use_category_folders: boolean;
    dpi_evasion: boolean;
    ja3_enabled: boolean;
    use_tor: boolean;
    min_threads: number;
    max_threads: number;
    telegram_bot_token: string | null;
    telegram_chat_id: string | null;
    chatops_enabled: boolean;
    proxy_enabled: boolean;
    proxy_type: string;
    proxy_host: string;
    proxy_port: number;
    proxy_username: string | null;
    proxy_password: string | null;
    cloud_enabled: boolean;
    cloud_endpoint: string | null;
    cloud_bucket: string | null;
    cloud_region: string | null;
    cloud_access_key: string | null;
    cloud_secret_key: string | null;
    last_sync_host: string | null;
    auto_extract_archives: boolean;
    cleanup_archives_after_extract: boolean;
    p2p_enabled: boolean;
    p2p_upload_limit_kbps: number | null;
    custom_sound_start: string | null;
    custom_sound_complete: string | null;
    custom_sound_error: string | null;
    auto_scrub_metadata: boolean;
    vpn_auto_connect: boolean;
    vpn_connection_name: string | null;
    mqtt_enabled: boolean;
    mqtt_broker_url: string;
    mqtt_topic: string;
    prevent_sleep_during_download: boolean;
    pause_on_low_battery: boolean;
}

/** Download progress event payload from Tauri backend */
export interface DownloadProgressPayload {
    id: string;
    downloaded: number;
    total: number;
    segments?: SlimSegment[];
}

/** Clipboard URL event payload */
export interface ClipboardUrlPayload {
    url: string;
    filename: string;
}

/** Extension download event payload */
export interface ExtensionDownloadPayload {
    url: string;
    filename: string;
}

/** Batch link entry from browser extension */
export interface BatchLink {
    url: string;
    filename: string;
}

/** Scheduled download start payload */
export interface ScheduledDownloadPayload {
    url: string;
    filename: string;
}

/** Docker image info from fetch_docker_manifest */
export interface DockerImageInfo {
    name: string;
    tag: string;
    layers: DockerLayer[];
}

/** Docker layer info */
export interface DockerLayer {
    digest: string;
    size: number;
    url: string;
    headers: Record<string, string>;
}

/** Wayback snapshot from check_wayback_availability */
export interface WaybackSnapshot {
    available: boolean;
    url: string;
    timestamp: string;
    status: string;
}

/** Upscale result from upscale_image */
export interface UpscaleResult {
    success: boolean;
    upscaled_path?: string;
    message?: string;
}

// Represents an active download inside the UI (including segments when available)
export interface DownloadTask {
    id: string;
    filename: string;
    url?: string;
    progress: number; // 0-100
    downloaded: number; // bytes
    total: number; // bytes
    speed: number; // bytes/sec
    status: 'Downloading' | 'Paused' | 'Error' | 'Done';
    segments?: Segment[];
    // used by overlays/trackers for internal timing, not persisted
    lastUpdate?: number;
}
