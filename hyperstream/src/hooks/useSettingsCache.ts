//! useSettingsCache - React hook for reactive settings cache management
//! 
//! Provides:
//! - Real-time cache freshness visibility  
//! - Validation feedback on settings change
//! - Cache stats display for debugging
//! - Automatic cache invalidation on save

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface CacheStats {
  is_fresh: boolean;
  age_secs: number | null;
  generation: number;
  ttl_secs: number;
}

export interface ValidationErrorDetail {
  field: string;
  message: string;
  is_critical: boolean;
}

export interface ValidationReport {
  valid: boolean;
  errors: ValidationErrorDetail[];
  warnings: string[];
  timestamp: number;
}

export interface SettingsWithStats {
  settings: any; // Settings type from API
  cache_stats: CacheStats;
}

export interface SaveResult {
  success: boolean;
  message: string;
  validation_report: ValidationReport;
}

/**
 * Hook for managing settings with cache awareness
 * 
 * Provides reactive access to:
 * - Cache metrics (age, freshness, generation)
 * - Validation reports
 * - Save operations with validation
 * 
 * Usage:
 * ```tsx
 * const { cacheStats, validateDraft, saveDraft, isLoading } = useSettingsCache();
 * ```
 */
export function useSettingsCache() {
  const [cacheStats, setCacheStats] = useState<CacheStats | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [lastValidation, setLastValidation] = useState<ValidationReport | null>(null);
  const [isCacheFresh, setIsCacheFresh] = useState(true);
  const [cacheAgeMs, setCacheAgeMs] = useState(0);

  // Fetch cache statistics
  const refreshCacheStats = useCallback(async () => {
    try {
      const stats = await invoke<CacheStats>('get_settings_cache_stats');
      setCacheStats(stats);
      setIsCacheFresh(stats.is_fresh);
      if (stats.age_secs !== null && stats.age_secs !== undefined) {
        setCacheAgeMs(stats.age_secs * 1000);
      }
    } catch (err) {
      console.error('Failed to fetch cache stats:', err);
    }
  }, []);

  // Validate settings without saving
  const validateDraft = useCallback(async (settings: any): Promise<ValidationReport> => {
    try {
      setIsLoading(true);
      const report = await invoke<ValidationReport>('validate_settings', { settings });
      setLastValidation(report);
      return report;
    } catch (err) {
      console.error('Validation failed:', err);
      const errorReport: ValidationReport = {
        valid: false,
        errors: [
          {
            field: '_system',
            message: `Validation error: ${err}`,
            is_critical: true,
          },
        ],
        warnings: [],
        timestamp: Date.now() / 1000,
      };
      setLastValidation(errorReport);
      return errorReport;
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load settings with cache stats
  const loadSettingsWithStats = useCallback(async (): Promise<SettingsWithStats> => {
    try {
      setIsLoading(true);
      const data = await invoke<SettingsWithStats>('get_settings_with_stats');
      setCacheStats(data.cache_stats);
      setIsCacheFresh(data.cache_stats.is_fresh);
      if (data.cache_stats.age_secs !== null && data.cache_stats.age_secs !== undefined) {
        setCacheAgeMs(data.cache_stats.age_secs * 1000);
      }
      return data;
    } catch (err) {
      console.error('Failed to load settings with stats:', err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Save settings with validation
  const saveDraft = useCallback(
    async (settings: any): Promise<SaveResult> => {
      try {
        setIsLoading(true);
        const result = await invoke<SaveResult>('save_settings_with_validation', { settings });
        setLastValidation(result.validation_report);
        if (result.success) {
          // Refresh cache stats after successful save
          await refreshCacheStats();
        }
        return result;
      } catch (err) {
        console.error('Save failed:', err);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [refreshCacheStats]
  );

  // Reload settings from disk (invalidate cache)
  const reloadFromDisk = useCallback(async () => {
    try {
      setIsLoading(true);
      await invoke('reload_settings_from_disk');
      await refreshCacheStats();
    } catch (err) {
      console.error('Failed to reload from disk:', err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [refreshCacheStats]);

  // Validate specific field
  const validateField = useCallback(
    async (settings: any, fieldName: string): Promise<string[]> => {
      try {
        const errors = await invoke<string[]>('get_field_validation_errors', {
          settings,
          field: fieldName,
        });
        return errors;
      } catch (err) {
        console.error(`Field validation failed for ${fieldName}:`, err);
        return [];
      }
    },
    []
  );

  // Invalidate cache manually
  const invalidateCache = useCallback(async () => {
    try {
      await invoke('invalidate_settings_cache');
      setIsCacheFresh(false);
      setCacheAgeMs(0);
    } catch (err) {
      console.error('Failed to invalidate cache:', err);
      throw err;
    }
  }, []);

  // Get generation number (useful for detecting external changes)
  const getCacheGeneration = useCallback(async (): Promise<number> => {
    try {
      return await invoke<number>('get_cache_generation');
    } catch (err) {
      console.error('Failed to get cache generation:', err);
      return 0;
    }
  }, []);

  // Poll cache freshness every 5 seconds
  useEffect(() => {
    const interval = setInterval(refreshCacheStats, 5000);
    return () => clearInterval(interval);
  }, [refreshCacheStats]);

  // Initial load
  useEffect(() => {
    refreshCacheStats();
  }, [refreshCacheStats]);

  // ============ PRODUCTION-GRADE METHODS ============

  // Get cache performance metrics
  const getCacheMetrics = useCallback(async () => {
    try {
      return await invoke<any>('get_cache_metrics');
    } catch (err) {
      console.error('Failed to get cache metrics:', err);
      return null;
    }
  }, []);

  // Recover settings from fallback
  const recoverFromFallback = useCallback(async (): Promise<any> => {
    try {
      setIsLoading(true);
      const recovered = await invoke<any>('recover_settings_from_fallback');
      await refreshCacheStats();
      return recovered;
    } catch (err) {
      console.error('Fallback recovery failed:', err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [refreshCacheStats]);

  // Set degraded mode
  const setCacheDegradedMode = useCallback(async (degraded: boolean) => {
    try {
      await invoke('set_cache_degraded_mode', { degraded });
    } catch (err) {
      console.error('Failed to set degraded mode:', err);
      throw err;
    }
  }, []);

  // Force cache refresh
  const forceCacheRefresh = useCallback(async () => {
    try {
      setIsLoading(true);
      const refreshed = await invoke<any>('force_cache_refresh');
      await refreshCacheStats();
      return refreshed;
    } catch (err) {
      console.error('Force refresh failed:', err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [refreshCacheStats]);

  // Check cache health
  const checkCacheHealth = useCallback(async () => {
    try {
      const health = await invoke<any>('check_cache_health');
      return health;
    } catch (err) {
      console.error('Health check failed:', err);
      return null;
    }
  }, []);

  return {
    // State
    cacheStats,
    isLoading,
    lastValidation,
    isCacheFresh,
    cacheAgeMs,
    
    // Methods
    refreshCacheStats,
    validateDraft,
    loadSettingsWithStats,
    saveDraft,
    reloadFromDisk,
    validateField,
    invalidateCache,
    getCacheGeneration,
    
    // New production methods
    getCacheMetrics,
    recoverFromFallback,
    setCacheDegradedMode,
    forceCacheRefresh,
    checkCacheHealth,
    
    // Derived state
    cacheAgeSeconds: cacheAgeMs / 1000,
    needsReload: !isCacheFresh,
  };
}

/**
 * Hook for showing cache status in UI
 * 
 * Usage:
 * ```tsx
 * const cacheStatus = useSettingsCacheStatus();
 * return <div>{cacheStatus.label}</div>;
 * ```
 */
export function useSettingsCacheStatus() {
  const { isCacheFresh, cacheAgeSeconds, cacheStats } = useSettingsCache();

  const statusColor = isCacheFresh ? 'text-green-500' : 'text-yellow-500';
  const statusLabel = isCacheFresh ? 'Fresh' : 'Stale (need reload)';
  
  return {
    statusLabel,
    statusColor,
    isFresh: isCacheFresh,
    ageSeconds: cacheAgeSeconds,
    generation: cacheStats?.generation,
  };
}

/**
 * Hook for inline field validation
 * 
 * Usage:
 * ```tsx
 * const fieldErrors = useFieldValidation(settings, 'segments');
 * ```
 */
export function useFieldValidation(settings: any, fieldName: string) {
  const { validateField } = useSettingsCache();
  const [errors, setErrors] = useState<string[]>([]);
  const [isValidating, setIsValidating] = useState(false);

  useEffect(() => {
    if (!settings) return;
    
    const validateAsync = async () => {
      setIsValidating(true);
      const fieldErrors = await validateField(settings, fieldName);
      setErrors(fieldErrors);
      setIsValidating(false);
    };

    // Debounce validation
    const timer = setTimeout(validateAsync, 300);
    return () => clearTimeout(timer);
  }, [settings, fieldName, validateField]);

  return {
    errors,
    isValidating,
    hasErrors: errors.length > 0,
  };
}
