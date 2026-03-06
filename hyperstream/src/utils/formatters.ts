// Shared formatting utilities used across components

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0 || !Number.isFinite(bytes)) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

export function formatSpeed(bytesPerSec: number): string {
  return formatBytes(bytesPerSec) + '/s';
}

// ETA smoothing state: per-download EMA of seconds remaining
const etaState = new Map<string, number>();

export function formatETA(remainingBytes: number, speed: number, downloadId?: string): string {
  if (remainingBytes <= 0 && speed > 0) return 'Done';
  if (speed <= 0 || remainingBytes <= 0 || !Number.isFinite(remainingBytes) || !Number.isFinite(speed)) return '--:--';
  const rawSeconds = remainingBytes / speed;

  let seconds: number;
  if (downloadId) {
    const prev = etaState.get(downloadId);
    if (prev !== undefined) {
      // Heavier damping (α=0.15) to prevent ETA jitter
      seconds = Math.floor(0.15 * rawSeconds + 0.85 * prev);
    } else {
      seconds = Math.floor(rawSeconds);
    }
    etaState.set(downloadId, seconds);
  } else {
    seconds = Math.floor(rawSeconds);
  }

  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}m ${secs}s`;
  }
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${mins}m`;
}

export function clearETAState(downloadId: string): void {
  etaState.delete(downloadId);
}
