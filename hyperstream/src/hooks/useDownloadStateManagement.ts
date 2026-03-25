//! React hook for download state management
//! 
//! Provides a production-grade, type-safe interface for managing download states
//! from the React frontend.

import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback, useEffect } from 'react';

export interface DownloadStateInfo {
  id: string;
  url: string;
  filename: string;
  status: 'Pending' | 'Downloading' | 'Paused' | 'Completed' | 'Error' | 'Recovering';
  downloaded_bytes: number;
  total_size: number;
  progress_percent: number;
  last_active?: string;
  error_message?: string;
}

export interface ResumeValidityReport {
  download_id: string;
  level: 'safe' | 'caution' | 'warning' | 'blocked';
  can_resume: boolean;
  requires_confirmation: boolean;
  cannot_resume: boolean;
  checks_passed: string[];
  checks_warning: string[];
  checks_failed: string[];
  recommendation: string;
  suggested_retry_delay_secs?: number;
  should_restart_from_scratch: boolean;
  summary: string;
}

export interface DownloadDiagnostics {
  download_id: string;
  state: string;
  downloaded_bytes: number;
  total_size: number;
  progress_percent: number;
  recommendation: string;
  is_healthy: boolean;
  can_resume: boolean;
  warning_count: number;
}

export interface DownloadsHealthSummary {
  total_downloads: number;
  healthy_count: number;
  at_risk_count: number;
  failed_count: number;
  total_size_bytes: number;
  downloaded_bytes: number;
  overall_progress_percent: number;
}

/**
 * Hook for managing download state operations
 */
export function useDownloadStateManagement() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const getDownloadState = useCallback(
    async (id: string): Promise<DownloadStateInfo | null> => {
      try {
        setLoading(true);
        setError(null);
        return await invoke<DownloadStateInfo | null>('get_download_state', { id });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const getAllDownloadStates = useCallback(async (): Promise<DownloadStateInfo[]> => {
    try {
      setLoading(true);
      setError(null);
      return await invoke<DownloadStateInfo[]>('get_all_download_states');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  const validateResumeSafety = useCallback(
    async (id: string): Promise<ResumeValidityReport> => {
      try {
        setLoading(true);
        setError(null);
        return await invoke<ResumeValidityReport>('validate_resume_safety', { id });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const getDownloadDiagnostics = useCallback(
    async (id: string): Promise<DownloadDiagnostics> => {
      try {
        setLoading(true);
        setError(null);
        return await invoke<DownloadDiagnostics>('get_download_diagnostics', { id });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const getDownloadsHealthSummary = useCallback(
    async (): Promise<DownloadsHealthSummary> => {
      try {
        setLoading(true);
        setError(null);
        return await invoke<DownloadsHealthSummary>('get_downloads_health_summary');
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  return {
    loading,
    error,
    getDownloadState,
    getAllDownloadStates,
    validateResumeSafety,
    getDownloadDiagnostics,
    getDownloadsHealthSummary,
  };
}

/**
 * Hook for auto-validating resume safety before starting a download
 */
export function useResumeValidation(downloadId: string | null) {
  const { validateResumeSafety, loading, error } = useDownloadStateManagement();
  const [report, setReport] = useState<ResumeValidityReport | null>(null);

  useEffect(() => {
    if (!downloadId) {
      setReport(null);
      return;
    }

    const validate = async () => {
      try {
        const result = await validateResumeSafety(downloadId);
        setReport(result);
      } catch (err) {
        console.error('Resume validation failed:', err);
        setReport(null);
      }
    };

    validate();
  }, [downloadId, validateResumeSafety]);

  return { report, loading, error };
}

/**
 * Hook for monitoring download health
 */
export function useDownloadHealthMonitoring() {
  const { getDownloadsHealthSummary, loading, error } = useDownloadStateManagement();
  const [summary, setSummary] = useState<DownloadsHealthSummary | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await getDownloadsHealthSummary();
      setSummary(result);
    } catch (err) {
      console.error('Health monitoring failed:', err);
    }
  }, [getDownloadsHealthSummary]);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 10_000); // Refresh every 10 seconds
    return () => clearInterval(interval);
  }, [refresh]);

  return { summary, loading, error, refresh };
}

/**
 * Hook for getting diagnostics for a specific download
 */
export function useDownloadDiagnostics(downloadId: string | null) {
  const { getDownloadDiagnostics, loading, error } = useDownloadStateManagement();
  const [diagnostics, setDiagnostics] = useState<DownloadDiagnostics | null>(null);

  const refresh = useCallback(async () => {
    if (!downloadId) return;

    try {
      const result = await getDownloadDiagnostics(downloadId);
      setDiagnostics(result);
    } catch (err) {
      console.error('Diagnostics fetch failed:', err);
    }
  }, [downloadId, getDownloadDiagnostics]);

  useEffect(() => {
    refresh();
  }, [downloadId, refresh]);

  return { diagnostics, loading, error, refresh };
}

/**
 * Helper to determine if a download can be resumed safely
 */
export function canResumeDownload(report: ResumeValidityReport | null): boolean {
  if (!report) return false;
  return report.can_resume;
}

/**
 * Helper to determine if user confirmation is needed before resuming
 */
export function needsConfirmationToResume(report: ResumeValidityReport | null): boolean {
  if (!report) return false;
  return report.requires_confirmation;
}

/**
 * Helper to get human-readable status string
 */
export function getStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    'Downloading': 'Downloading',
    'Paused': 'Paused',
    'Completed': 'Complete',
    'Done': 'Complete',
    'Error': 'Error',
    'Recovering': 'Recovering',
    'Pending': 'Pending',
  };
  return labels[status] || 'Unknown';
}

/**
 * Helper to get color for status badge
 */
export function getStatusColor(status: string): string {
  const colors: Record<string, string> = {
    'Downloading': 'bg-blue-500/20 text-blue-400',
    'Completed': 'bg-green-500/20 text-green-400',
    'Done': 'bg-green-500/20 text-green-400',
    'Paused': 'bg-yellow-500/20 text-yellow-400',
    'Error': 'bg-red-500/20 text-red-400',
    'Recovering': 'bg-orange-500/20 text-orange-400',
    'Pending': 'bg-gray-500/20 text-gray-400',
  };
  return colors[status] || 'bg-gray-500/20 text-gray-400';
}
