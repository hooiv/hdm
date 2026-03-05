/**
 * Safe wrappers around Tauri APIs that gracefully degrade in non-Tauri environments
 * (e.g. when previewing the frontend in a regular browser via Vite dev server).
 */

const isTauri = () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

/**
 * Safe invoke — returns a rejected promise with a descriptive message
 * when not running inside Tauri.
 */
export async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (!isTauri()) {
        throw new Error(`[tauri-stub] invoke("${cmd}") called outside Tauri runtime`);
    }
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
}

/**
 * Safe listen — returns a no-op unlisten function when not running inside Tauri.
 */
export async function safeListen<T>(
    event: string,
    handler: (event: { payload: T }) => void,
): Promise<() => void> {
    if (!isTauri()) {
        // Return a no-op unlisten function
        return () => {};
    }
    const { listen } = await import('@tauri-apps/api/event');
    return listen<T>(event, handler);
}

/**
 * Safe Window.getByLabel — returns null when not running inside Tauri.
 */
export async function safeGetWindowByLabel(label: string) {
    if (!isTauri()) {
        return null;
    }
    const { Window } = await import('@tauri-apps/api/window');
    return Window.getByLabel(label);
}

export { isTauri };
