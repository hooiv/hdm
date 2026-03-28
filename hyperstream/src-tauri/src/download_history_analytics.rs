/// Download History Analytics Engine
/// 
/// Analyzes historical download data to provide:
/// - Success rate trends
/// - Performance patterns  
/// - Time-based insights (when downloads succeed best)
/// - File-type analysis
/// - Mirror performance tracking
/// - Smart recommendations for future downloads
/// - Failure pattern recognition

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadHistoryStat {
    pub url: String,
    pub filename: String,
    pub file_size_bytes: u64,
    pub success: bool,
    pub duration_seconds: u64,
    pub average_speed_mbps: f64,
    pub timestamp: u64,
    pub mirror_used: String,
    pub failure_reason: Option<String>,
    pub retries_needed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileTypeInsights {
    pub file_type: String,     // .iso, .zip, .exe, etc.
    pub total_downloads: u32,
    pub successful: u32,
    pub success_rate: f64,     // 0.0-1.0
    pub avg_speed_mbps: f64,
    pub avg_duration_seconds: u64,
    pub common_failure_reasons: Vec<(String, u32)>, // (reason, count)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeWindowInsights {
    pub hour_of_day: u8,       // 0-23
    pub downloads_in_window: u32,
    pub success_rate: f64,
    pub avg_speed_mbps: f64,
    pub peak_hours: Vec<u8>,   // Hours when success is highest
    pub low_hours: Vec<u8>,    // Hours when success is lowest
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MirrorAnalytics {
    pub mirror_host: String,
    pub total_downloads: u32,
    pub successful: u32,
    pub success_rate: f64,
    pub avg_speed_mbps: f64,
    pub is_cdn: bool,
    pub failure_count: u32,
    pub reliability_trend: String, // "Improving", "Stable", "Degrading"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadRecommendation {
    pub recommendation_id: String,
    pub title: String,
    pub description: String,
    pub category: String,      // "timing", "mirror", "file-type", "strategy"
    pub expected_improvement: f64, // 0.0-1.0, potential improvement percentage
    pub confidence: f64,       // 0.0-1.0, how confident we are
    pub action: String,        // "Download at peak hours", "Use mirror X", etc.
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadAnalyticsSnapshot {
    pub total_downloads: u64,
    pub successful_downloads: u64,
    pub overall_success_rate: f64,
    pub total_bytes_downloaded: u64,
    pub total_time_seconds: u64,
    pub avg_speed_mbps: f64,
    pub avg_duration_seconds: u64,
    pub total_retries: u64,
    
    pub file_type_insights: Vec<FileTypeInsights>,
    pub time_window_insights: Vec<TimeWindowInsights>,
    pub mirror_analytics: Vec<MirrorAnalytics>,
    pub recommendations: Vec<DownloadRecommendation>,
    pub failure_patterns: Vec<(String, u32)>, // (pattern, occurrences)
    pub best_time_to_download: String,
    pub worst_mirror: Option<String>,
    pub best_mirror: Option<String>,
}

/// Analytics engine
pub struct DownloadHistoryAnalytics {
    stats: Arc<Mutex<Vec<DownloadHistoryStat>>>,
}

impl DownloadHistoryAnalytics {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record a download attempt
    pub fn record_download(&self, stat: DownloadHistoryStat) -> Result<(), String> {
        let mut stats = self.stats.lock().map_err(|e| e.to_string())?;
        stats.push(stat);
        Ok(())
    }

    /// Get comprehensive analytics snapshot
    pub fn get_analytics(&self) -> Result<DownloadAnalyticsSnapshot, String> {
        let stats = self.stats.lock().map_err(|e| e.to_string())?;

        if stats.is_empty() {
            return Ok(DownloadAnalyticsSnapshot {
                total_downloads: 0,
                successful_downloads: 0,
                overall_success_rate: 0.0,
                total_bytes_downloaded: 0,
                total_time_seconds: 0,
                avg_speed_mbps: 0.0,
                avg_duration_seconds: 0,
                total_retries: 0,
                file_type_insights: Vec::new(),
                time_window_insights: Vec::new(),
                mirror_analytics: Vec::new(),
                recommendations: Vec::new(),
                failure_patterns: Vec::new(),
                best_time_to_download: "Unknown".to_string(),
                worst_mirror: None,
                best_mirror: None,
            });
        }

        let total = stats.len() as u64;
        let successful = stats.iter().filter(|s| s.success).count() as u64;
        let overall_success_rate = if total > 0 { successful as f64 / total as f64 } else { 0.0 };

        // Calculate aggregates
        let total_bytes: u64 = stats.iter().map(|s| s.file_size_bytes).sum();
        let total_time: u64 = stats.iter().map(|s| s.duration_seconds).sum();
        let total_speed: f64 = stats.iter().map(|s| s.average_speed_mbps).sum();
        let avg_speed_mbps = if !stats.is_empty() { total_speed / stats.len() as f64 } else { 0.0 };
        let avg_duration = if total > 0 { total_time as f64 / total as f64 } else { 0.0 };
        let total_retries: u64 = stats.iter().map(|s| s.retries_needed as u64).sum();

        // Analyze by file type
        let mut file_types: HashMap<String, Vec<&DownloadHistoryStat>> = HashMap::new();
        for stat in stats.iter() {
            let ext = stat.filename
                .split('.')
                .last()
                .unwrap_or("unknown")
                .to_lowercase();
            file_types.entry(ext).or_insert_with(Vec::new).push(stat);
        }

        let file_type_insights: Vec<FileTypeInsights> = file_types
            .into_iter()
            .map(|(file_type, downloads)| {
                let successful = downloads.iter().filter(|d| d.success).count() as u32;
                let success_rate = if !downloads.is_empty() {
                    successful as f64 / downloads.len() as f64
                } else {
                    0.0
                };
                let avg_speed = downloads.iter().map(|d| d.average_speed_mbps).sum::<f64>()
                    / downloads.len() as f64;
                let avg_duration = downloads.iter().map(|d| d.duration_seconds).sum::<u64>()
                    / downloads.len() as u64;

                // Find common failure reasons
                let mut failures: HashMap<String, u32> = HashMap::new();
                for d in downloads.iter() {
                    if !d.success {
                        if let Some(reason) = &d.failure_reason {
                            *failures.entry(reason.clone()).or_insert(0) += 1;
                        }
                    }
                }
                let mut failure_reasons: Vec<_> = failures.into_iter().collect();
                failure_reasons.sort_by_key(|r| std::cmp::Reverse(r.1));

                FileTypeInsights {
                    file_type,
                    total_downloads: downloads.len() as u32,
                    successful,
                    success_rate,
                    avg_speed_mbps: avg_speed,
                    avg_duration_seconds: avg_duration,
                    common_failure_reasons: failure_reasons,
                }
            })
            .collect();

        // Analyze by time windows (hours of day)
        let mut time_windows: HashMap<u8, Vec<&DownloadHistoryStat>> = HashMap::new();
        for stat in stats.iter() {
            let hour = (stat.timestamp / 3600) % 24;
            time_windows.entry(hour as u8).or_insert_with(Vec::new).push(stat);
        }

        let mut time_window_insights: Vec<TimeWindowInsights> = time_windows
            .into_iter()
            .map(|(hour, downloads)| {
                let successful = downloads.iter().filter(|d| d.success).count() as u32;
                let success_rate = if !downloads.is_empty() {
                    successful as f64 / downloads.len() as f64
                } else {
                    0.0
                };
                let avg_speed = downloads.iter().map(|d| d.average_speed_mbps).sum::<f64>()
                    / downloads.len() as f64;

                TimeWindowInsights {
                    hour_of_day: hour,
                    downloads_in_window: downloads.len() as u32,
                    success_rate,
                    avg_speed_mbps: avg_speed,
                    peak_hours: Vec::new(),
                    low_hours: Vec::new(),
                }
            })
            .collect();

        // Find peak and low hours
        time_window_insights.sort_by(|a, b| b.success_rate.partial_cmp(&a.success_rate).unwrap_or(std::cmp::Ordering::Equal));
        let peak_hours: Vec<u8> = time_window_insights.iter().take(3).map(|t| t.hour_of_day).collect();
        let low_hours: Vec<u8> = time_window_insights.iter().rev().take(3).map(|t| t.hour_of_day).collect();

        for insight in time_window_insights.iter_mut() {
            insight.peak_hours = peak_hours.clone();
            insight.low_hours = low_hours.clone();
        }

        // Analyze by mirror
        let mut mirrors: HashMap<String, Vec<&DownloadHistoryStat>> = HashMap::new();
        for stat in stats.iter() {
            mirrors.entry(stat.mirror_used.clone()).or_insert_with(Vec::new).push(stat);
        }

        let mut mirror_analytics: Vec<MirrorAnalytics> = mirrors
            .into_iter()
            .map(|(mirror, downloads)| {
                let successful = downloads.iter().filter(|d| d.success).count() as u32;
                let success_rate = if !downloads.is_empty() {
                    successful as f64 / downloads.len() as f64
                } else {
                    0.0
                };
                let avg_speed = downloads.iter().map(|d| d.average_speed_mbps).sum::<f64>()
                    / downloads.len() as f64;
                let failure_count = downloads.iter().filter(|d| !d.success).count() as u32;

                MirrorAnalytics {
                    mirror_host: mirror,
                    total_downloads: downloads.len() as u32,
                    successful,
                    success_rate,
                    avg_speed_mbps: avg_speed,
                    is_cdn: downloads.first().map(|d| d.mirror_used.contains("cdn")).unwrap_or(false),
                    failure_count,
                    reliability_trend: "Stable".to_string(),
                }
            })
            .collect();

        mirror_analytics.sort_by(|a, b| b.success_rate.partial_cmp(&a.success_rate).unwrap_or(std::cmp::Ordering::Equal));

        let best_mirror = mirror_analytics.first().map(|m| m.mirror_host.clone());
        let worst_mirror = mirror_analytics.last().map(|m| m.mirror_host.clone());

        // Generate recommendations
        let recommendations = self.generate_recommendations(&file_type_insights, &time_window_insights, &mirror_analytics);

        // Analyze failure patterns
        let mut failure_patterns: HashMap<String, u32> = HashMap::new();
        for stat in stats.iter() {
            if !stat.success {
                if let Some(reason) = &stat.failure_reason {
                    *failure_patterns.entry(reason.clone()).or_insert(0) += 1;
                }
            }
        }
        let mut failure_patterns_vec: Vec<_> = failure_patterns.into_iter().collect();
        failure_patterns_vec.sort_by_key(|p| std::cmp::Reverse(p.1));

        let best_time_to_download = if !time_window_insights.is_empty() {
            time_window_insights[0].hour_of_day.to_string()
        } else {
            "Unknown".to_string()
        };

        Ok(DownloadAnalyticsSnapshot {
            total_downloads: total,
            successful_downloads: successful,
            overall_success_rate,
            total_bytes_downloaded: total_bytes,
            total_time_seconds: total_time,
            avg_speed_mbps,
            avg_duration_seconds: avg_duration as u64,
            total_retries,
            file_type_insights,
            time_window_insights,
            mirror_analytics,
            recommendations,
            failure_patterns: failure_patterns_vec,
            best_time_to_download,
            worst_mirror,
            best_mirror,
        })
    }

    fn generate_recommendations(&self, file_types: &[FileTypeInsights], time_windows: &[TimeWindowInsights], mirrors: &[MirrorAnalytics]) -> Vec<DownloadRecommendation> {
        let mut recommendations = Vec::new();

        // Timing recommendations
        if !time_windows.is_empty() {
            let best_hour = time_windows.first().map(|t| t.hour_of_day);
            if let Some(hour) = best_hour {
                recommendations.push(DownloadRecommendation {
                    recommendation_id: "timing_best_hour".to_string(),
                    title: "Download at optimal time".to_string(),
                    description: format!("Historical data shows {} is the best hour to download", hour),
                    category: "timing".to_string(),
                    expected_improvement: 0.15,
                    confidence: 0.85,
                    action: format!("Schedule downloads for {}:00", hour),
                });
            }
        }

        // Mirror recommendations
        if !mirrors.is_empty() && mirrors[0].success_rate > 0.85 {
            recommendations.push(DownloadRecommendation {
                recommendation_id: "mirror_best".to_string(),
                title: "Prefer best-performing mirror".to_string(),
                description: format!("{} has highest success rate ({:.0}%)", mirrors[0].mirror_host, mirrors[0].success_rate * 100.0),
                category: "mirror".to_string(),
                expected_improvement: 0.20,
                confidence: 0.90,
                action: format!("Prioritize {}", mirrors[0].mirror_host),
            });
        }

        // File-type specific recommendations
        for ft in file_types.iter().take(3) {
            if ft.success_rate < 0.8 && !ft.common_failure_reasons.is_empty() {
                if let Some((reason, _count)) = ft.common_failure_reasons.first() {
                    recommendations.push(DownloadRecommendation {
                        recommendation_id: format!("file_type_{}", ft.file_type),
                        title: format!("{} files need special handling", ft.file_type),
                        description: format!("Success rate: {:.0}%. Most common issue: {}", ft.success_rate * 100.0, reason),
                        category: "file-type".to_string(),
                        expected_improvement: 0.25,
                        confidence: 0.75,
                        action: format!("Enable extra retry for .{} files", ft.file_type),
                    });
                }
            }
        }

        recommendations
    }
}

// Global analyzer instance
static HISTORY_ANALYTICS: OnceLock<DownloadHistoryAnalytics> = OnceLock::new();

pub fn get_history_analytics() -> &'static DownloadHistoryAnalytics {
    HISTORY_ANALYTICS.get_or_init(|| DownloadHistoryAnalytics::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_creation() {
        let analytics = DownloadHistoryAnalytics::new();
        let snapshot = analytics.get_analytics().unwrap();
        assert_eq!(snapshot.total_downloads, 0);
    }

    #[test]
    fn test_record_download() {
        let analytics = DownloadHistoryAnalytics::new();
        let stat = DownloadHistoryStat {
            url: "https://example.com/file.iso".to_string(),
            filename: "file.iso".to_string(),
            file_size_bytes: 4_000_000_000,
            success: true,
            duration_seconds: 1600,
            average_speed_mbps: 2.5,
            timestamp: 1000000000,
            mirror_used: "mirror.example.com".to_string(),
            failure_reason: None,
            retries_needed: 0,
        };

        analytics.record_download(stat).unwrap();
        let snapshot = analytics.get_analytics().unwrap();
        assert_eq!(snapshot.total_downloads, 1);
        assert_eq!(snapshot.successful_downloads, 1);
        assert_eq!(snapshot.overall_success_rate, 1.0);
    }
}
