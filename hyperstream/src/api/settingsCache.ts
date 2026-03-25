//! Typed API wrapper for settings cache commands
//! 
//! Provides typed, promise-based access to all settings cache commands

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
  settings: any;
  cache_stats: CacheStats;
}

export interface SaveSettingsResult {
  success: boolean;
  message: string;
  validation_report: ValidationReport;
}

/**
 * Get current cache statistics
 */
export async function getCacheStats(): Promise<CacheStats> {
  return invoke('get_settings_cache_stats');
}

/**
 * Validate settings without saving
 */
export async function validateSettings(settings: any): Promise<ValidationReport> {
  return invoke('validate_settings', { settings });
}

/**
 * Load settings with cache information
 */
export async function getSettingsWithStats(): Promise<SettingsWithStats> {
  return invoke('get_settings_with_stats');
}

/**
 * Save settings with comprehensive validation
 */
export async function saveSettingsWithValidation(settings: any): Promise<SaveSettingsResult> {
  return invoke('save_settings_with_validation', { settings });
}

/**
 * Reload settings from disk (invalidates cache)
 */
export async function reloadSettingsFromDisk(): Promise<any> {
  return invoke('reload_settings_from_disk');
}

/**
 * Get current cache generation number
 */
export async function getCacheGeneration(): Promise<number> {
  return invoke('get_cache_generation');
}

/**
 * Invalidate cache (force reload next access)
 */
export async function invalidateCache(): Promise<void> {
  return invoke('invalidate_settings_cache');
}

/**
 * Get validation errors for a specific field
 */
export async function getFieldValidationErrors(
  settings: any,
  field: string
): Promise<string[]> {
  return invoke('get_field_validation_errors', { settings, field });
}

/**
 * Check if cache is stale
 */
export async function isCacheStale(): Promise<boolean> {
  const stats = await getCacheStats();
  return !stats.is_fresh;
}

/**
 * Get cache age in milliseconds
 */
export async function getCacheAgeMs(): Promise<number> {
  const stats = await getCacheStats();
  return (stats.age_secs ?? 0) * 1000;
}

/**
 * Check if a field is valid
 */
export async function isFieldValid(settings: any, field: string): Promise<boolean> {
  const errors = await getFieldValidationErrors(settings, field);
  return errors.length === 0;
}

/**
 * Validate all fields and get report
 */
export async function getValidationReport(settings: any): Promise<ValidationReport> {
  return validateSettings(settings);
}

/**
 * Get count of validation errors
 */
export async function getValidationErrorCount(settings: any): Promise<number> {
  const report = await getValidationReport(settings);
  return report.errors.filter(e => e.is_critical).length;
}

/**
 * Get count of validation warnings
 */
export async function getValidationWarningCount(settings: any): Promise<number> {
  const report = await getValidationReport(settings);
  return report.warnings.length;
}
