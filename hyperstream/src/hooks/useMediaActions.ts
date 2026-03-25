import { useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../contexts/ToastContext';
import type {
    UpscaleResult,
    SubtitleResult,
    ModOptimizerResult,
    DlnaDevice,
} from '../api/commands';

export function useMediaActions(filePath: string) {
    const toast = useToast();
    const toastRef = useRef(toast);
    toastRef.current = toast;

    const handleDlnaCast = useCallback(async () => {
        try {
            const devices = await invoke<DlnaDevice[]>('discover_dlna');
            if (!devices || devices.length === 0) {
                toastRef.current.error('No DLNA devices found on your network.');
                return;
            }
            const list = devices.map((d, i) => `${i + 1}. ${d.name}`).join('
');
            const choice = prompt(`📺 Select device:

${list}

Enter number:`);
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
            toastRef.current.success(`🎬 Subtitles ${result.status}!
Method: ${result.method}
SRT: ${result.srt_path}
Segments: ${result.subtitle_lines}${result.note ? '

Note: ' + result.note : ''}`);
        } catch (err) { toastRef.current.error('Subtitle generation failed: ' + err); }
    }, [filePath]);

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
            toastRef.current.success(`🎮 Scan Complete!
Files: ${result.total_files}
Duplicate Groups: ${result.duplicate_groups}
Wasted: ${result.wasted_mb?.toFixed(1)} MB`);
        } catch (err) { toastRef.current.error('Optimize failed: ' + err); }
    }, [filePath]);

    return {
        handleDlnaCast,
        handleGenerateSubtitles,
        handleAiUpscale,
        handleOptimizeMods,
    };
}
