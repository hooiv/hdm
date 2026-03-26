//! Settings Commands - Production-grade settings management with caching and validation
//!
//! Exposes cache statistics, validation results, and cache management operations to the frontend.

use crate::settings::{load_settings, save_settings, Settings};
use crate::settings_cache::{SETTINGS_CACHE, SettingsValidator, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Cache statistics for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub is_fresh: bool,
    pub age_secs: Option<u64>,
    pub generation: u64,
    pub ttl_secs: u64,
}

/// Validation report sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<ValidationErrorDetail>,
    pub warnings: Vec<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrorDetail {
    pub field: String,
    pub message: String,
    pub is_critical: bool,
}

/// Get cache statistics
#[tauri::command]
pub fn get_settings_cache_stats() -> Result<CacheStats, String> {
    let is_fresh = SETTINGS_CACHE.is_fresh()?;
    let age_secs = SETTINGS_CACHE.age_secs()?;
    let generation = SETTINGS_CACHE.generation();
    
    Ok(CacheStats {
        is_fresh,
        age_secs,
        generation,
        ttl_secs: 300,
    })
}

/// Validate settings without saving  
#[tauri::command]
pub fn validate_settings(settings: Settings) -> Result<ValidationReport, String> {
    let validation = SettingsValidator::validate(&settings);
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    
    let errors = validation.errors.iter().map(|e| ValidationErrorDetail {
        field: e.field.clone(),
        message: e.message.clone(),
        is_critical: e.severity == ErrorSeverity::Critical,
    }).collect();
    
    Ok(ValidationReport {
        valid: validation.valid,
        errors,
        warnings: validation.warnings,
        timestamp,
    })
}

/// Reload settings from disk (invalidate cache)
#[tauri::command]
pub fn reload_settings_from_disk() -> Result<Settings, String> {
    SETTINGS_CACHE.invalidate()?;
    let settings = crate::settings::load_settings_uncached();
    Ok(settings)
}

/// Get current cache generation (useful for detecting external changes)
#[tauri::command]
pub fn get_cache_generation() -> u64 {
    SETTINGS_CACHE.generation()
}

/// Force cache invalidation (useful for testing)
#[tauri::command]
pub fn invalidate_settings_cache() -> Result<(), String> {
    SETTINGS_CACHE.invalidate()
}

/// Get settings with cache stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsWithStats {
    pub settings: Settings,
    pub cache_stats: CacheStats,
}

/// Load settings and include cache information
#[tauri::command]
pub fn get_settings_with_stats() -> Result<SettingsWithStats, String> {
    let settings = load_settings();
    let cache_stats = get_settings_cache_stats()?;
    
    Ok(SettingsWithStats {
        settings,
        cache_stats,
    })
}

/// Save settings with comprehensive validation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSettingsResult {
    pub success: bool,
    pub message: String,
    pub validation_report: ValidationReport,
}

/// Save settings with detailed validation feedback
#[tauri::command]
pub fn save_settings_with_validation(settings: Settings) -> Result<SaveSettingsResult, String> {
    // Validate first
    let validation = SettingsValidator::validate(&settings);
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    
    let validation_report = ValidationReport {
        valid: validation.valid,
        errors: validation.errors.iter().map(|e| ValidationErrorDetail {
            field: e.field.clone(),
            message: e.message.clone(),
            is_critical: e.severity == ErrorSeverity::Critical,
        }).collect(),
        warnings: validation.warnings.clone(),
        timestamp,
    };
    
    if !validation.valid {
        return Ok(SaveSettingsResult {
            success: false,
            message: "Validation failed - settings not saved".to_string(),
            validation_report,
        });
    }
    
    // Attempt save
    match save_settings(&settings) {
        Ok(_) => {
            Ok(SaveSettingsResult {
                success: true,
                message: "Settings saved successfully".to_string(),
                validation_report,
            })
        }
        Err(e) => {
            Err(format!("Failed to save settings: {}", e))
        }
    }
}

/// Get detailed validation errors for a specific field
#[tauri::command]
pub fn get_field_validation_errors(settings: Settings, field: String) -> Result<Vec<String>, String> {
    let validation = SettingsValidator::validate(&settings);
    
    let errors: Vec<String> = validation.errors
        .into_iter()
        .filter(|e| e.field.starts_with(&field))
        .map(|e| e.message)
        .collect();
    
    Ok(errors)
}

// ============ NEW PRODUCTION-GRADE COMMANDS ============

/// Cache metrics for monitoring and diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetricsReport {
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub invalidations: u64,
    pub saves: u64,
    pub validation_errors: u64,
    pub poisoned_lock_recoveries: u64,
    pub avg_read_time_ms: f64,
    pub avg_write_time_ms: f64,
    pub last_save_duration_ms: u64,
    pub is_degraded: bool,
}

/// Get cache performance metrics for monitoring
#[tauri::command]
pub fn get_cache_metrics() -> Result<CacheMetricsReport, String> {
    let metrics = SETTINGS_CACHE.metrics()?;
    let is_degraded = SETTINGS_CACHE.is_degraded();
    
    Ok(CacheMetricsReport {
        hits: metrics.hits,
        misses: metrics.misses,
        hit_ratio: metrics.hit_ratio(),
        invalidations: metrics.invalidations,
        saves: metrics.saves,
        validation_errors: metrics.validation_errors,
        poisoned_lock_recoveries: metrics.poisoned_lock_recoveries,
        avg_read_time_ms: metrics.avg_read_time_ms,
        avg_write_time_ms: metrics.avg_write_time_ms,
        last_save_duration_ms: metrics.last_save_duration_ms,
        is_degraded,
    })
}

/// Recover settings from fallback cache (disaster recovery)
#[tauri::command]
pub fn recover_settings_from_fallback() -> Result<Settings, String> {
    let fallback = SETTINGS_CACHE.get_fallback_settings()?;
    
    if let Some(_settings) = fallback {
        // Try to reload from disk first, fallback to last known good settings
        let fresh = load_settings();
        // load_settings() always returns Settings (not Result), so use it directly
        Ok(fresh)
    } else {
        Err("No fallback settings available for recovery".to_string())
    }
}

/// Set cache degraded mode (for when things are wrong but we keep running)
#[tauri::command]
pub fn set_cache_degraded_mode(degraded: bool) -> Result<(), String> {
    SETTINGS_CACHE.set_degraded(degraded);
    Ok(())
}

/// Force full cache refresh and rebuild
#[tauri::command]
pub fn force_cache_refresh() -> Result<Settings, String> {
    // Invalidate cache
    SETTINGS_CACHE.invalidate()?;
    
    // Reload from disk - load_settings_uncached returns Settings directly (not Result)
    let settings = crate::settings::load_settings_uncached();
    
    // Re-populate cache
    SETTINGS_CACHE.put(settings.clone())?;
    Ok(settings)
}

/// Health check - verify cache is operational
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHealthStatus {
    pub is_healthy: bool,
    pub is_degraded: bool,
    pub is_fresh: bool,
    pub age_seconds: Option<u64>,
    pub can_read: bool,
    pub can_write: bool,
    pub last_error: Option<String>,
}

#[tauri::command]
pub fn check_cache_health() -> CacheHealthStatus {
    let is_fresh = SETTINGS_CACHE.is_fresh().unwrap_or(false);
    let age_seconds = SETTINGS_CACHE.age_secs().unwrap_or(None);
    let is_degraded = SETTINGS_CACHE.is_degraded();
    
    // Test read capability
    let can_read = SETTINGS_CACHE.get().is_ok();
    
    // Test write capability with a dummy write
    let test_settings = crate::settings::load_settings();
    let can_write = SETTINGS_CACHE.put(test_settings).is_ok();
    
    let is_healthy = can_read && can_write && !is_degraded && is_fresh;
    
    CacheHealthStatus {
        is_healthy,
        is_degraded,
        is_fresh,
        age_seconds,
        can_read,
        can_write,
        last_error: None,
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn test_cache_stats() {
        let stats = super::get_settings_cache_stats().unwrap();
        assert_eq!(stats.ttl_secs, 300);
    }

    #[test]
    fn test_validation_report() {
        let mut settings = crate::settings::Settings::default();
        settings.segments = 0; // Invalid
        
        let report = super::validate_settings(settings).unwrap();
        assert!(!report.valid);
        assert!(!report.errors.is_empty());
    }
}
