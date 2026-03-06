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
    id: number;
    name: string;
    info_hash: string;
    total_size: number;
    downloaded: number;
    uploaded: number;
    progress_percent: number;
    /** Download speed in bytes/s */
    speed_download: number;
    /** Upload speed in bytes/s */
    speed_upload: number;
    /** Number of unchoked / active peers */
    peers_live: number;
    /** Total peers seen (live + queued + connecting) */
    peers_total: number;
    /** "initializing" | "live" | "paused" | "error" */
    state: string;
    /** Estimated seconds remaining, or null */
    eta_secs: number | null;
    /** Upload/download ratio */
    ratio: number;
    /** Queue priority: "high" | "normal" | "low" */
    priority: string;
    pinned: boolean;
    /** "queue" | "seeding_policy" | null */
    auto_pause_reason: string | null;
    save_path: string;
    error: string | null;
    finished: boolean;
}

export interface TorrentFileInfo {
    id: number;
    name: string;
    size: number;
    downloaded: number;
    progress_percent: number;
    included: boolean;
}

export interface TorrentBulkActionResult {
    attempted: number;
    succeeded: number;
    failed: number;
    failed_ids: number[];
}

export interface AddTorrentResult {
    id: number;
    warnings: string[];
}

export interface TorrentActionFailedEvent {
    timestamp_ms: number;
    severity: string;
    category: string;
    action: string;
    id: number | null;
    error: string;
}

export interface TorrentDiagnostics {
    generated_at_ms: number;
    auto_manage_queue: boolean;
    max_active_downloads: number;
    auto_stop_seeding: boolean;
    seed_ratio_limit: number;
    seed_time_limit_mins: number;
    total_torrents: number;
    live_torrents: number;
    paused_torrents: number;
    error_torrents: number;
    initializing_torrents: number;
    completed_torrents: number;
    pinned_torrents: number;
    queue_auto_paused: number;
    seeding_policy_auto_paused: number;
    recent_error_count: number;
    recent_warning_count: number;
    recent_errors: TorrentActionFailedEvent[];
    recent_warnings: TorrentActionFailedEvent[];
    recent_failures: TorrentActionFailedEvent[];
    torrents: TorrentStatus[];
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
    torrent_max_active_downloads: number;
    torrent_auto_manage_queue: boolean;
    torrent_auto_stop_seeding: boolean;
    torrent_seed_ratio_limit: number;
    torrent_seed_time_limit_mins: number;
    torrent_priority_overrides: Record<string, string>;
    torrent_pinned_hashes: string[];
    // Quiet Hours
    quiet_hours_enabled: boolean;
    quiet_hours_start: number;
    quiet_hours_end: number;
    quiet_hours_action: string;
    quiet_hours_throttle_kbps: number;
    // Speed Profiles
    speed_profiles_enabled: boolean;
    speed_profiles: SpeedProfile[];
}

/** Time-based speed limit profile */
export interface SpeedProfile {
    name: string;
    start_time: string;
    end_time: string;
    speed_limit_kbps: number;
    days: number[];
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

/** Status record returned by HTTP API for active downloads */
export interface DownloadStatus {
    id: string;
    url: string;
    filename?: string;
    downloaded: number;
    total: number;
    speed_bps?: number;
    status: string;
}

export interface ControlRequest {
    id: string;
    action: 'pause' | 'cancel';
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

// ---- HLS helpers ---------------------------------------------------------

/** Variant entry from an HLS master playlist */
export interface HlsVariant {
    bandwidth: number;
    resolution?: string;
    url: string;
}

/** Stream details returned by the `parse_hls_stream` command */
export interface HlsStream {
    variants: HlsVariant[];
    segments: Array<{ url: string; duration: number; sequence: number; key_uri?: string; key_iv?: string }>;
    target_duration: number;
    is_master: boolean;
}

/** Upscale result from upscale_image */
export interface UpscaleResult {
    success: boolean;
    upscaled_path?: string;
    message?: string;
}

// Represents an active download inside the UI (including segments when available)
export interface MirrorStat {
    url: string;
    source: string;
    avg_speed_bps: number;
    total_bytes: number;
    success_count: number;
    error_count: number;
    supports_range: boolean;
    latency_ms: number;
    disabled: boolean;
}

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
    mirrorStats?: MirrorStat[];
    // used by overlays/trackers for internal timing, not persisted
    lastUpdate?: number;
}
