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
