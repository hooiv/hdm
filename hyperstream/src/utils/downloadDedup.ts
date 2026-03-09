import type { DownloadTask } from "../types";

const DUPLICATE_DOWNLOAD_ERROR_FRAGMENT = "already active or queued";

export function normalizeDownloadUrl(rawUrl?: string): string {
  const trimmed = rawUrl?.trim() ?? "";
  if (!trimmed) return "";

  try {
    const parsed = new URL(trimmed);
    parsed.hash = "";

    if ((parsed.protocol === "http:" && parsed.port === "80") || (parsed.protocol === "https:" && parsed.port === "443")) {
      parsed.port = "";
    }

    return parsed.toString();
  } catch {
    return trimmed;
  }
}

export function isDuplicateDownloadError(error: unknown): boolean {
  return String(error ?? "").toLowerCase().includes(DUPLICATE_DOWNLOAD_ERROR_FRAGMENT);
}

export function findActiveTaskByUrl(tasks: DownloadTask[], url?: string, excludeId?: string): DownloadTask | undefined {
  const normalized = normalizeDownloadUrl(url);
  if (!normalized) return undefined;

  return tasks.find((task) => (
    task.id !== excludeId
    && task.status === "Downloading"
    && normalizeDownloadUrl(task.url) === normalized
  ));
}