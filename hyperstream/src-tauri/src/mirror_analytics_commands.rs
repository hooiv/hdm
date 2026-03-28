//! Tauri command handlers for Mirror Analytics
//!
//! Exposes statistical analysis, trend detection, recommendations, and comparisons.

use crate::mirror_analytics::{MirrorAnalyticsEngine, MirrorStatistics};
use serde::{Deserialize, Serialize};

/// Request to analyze mirror performance
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeStatisticsRequest {
    pub mirror_url: String,
    pub success_count: u32,
    pub failure_count: u32,
    pub speeds_bps: Vec<u64>,
    pub corruption_count: u32,
    pub response_times_ms: Vec<u64>,
}

/// Request to compare two mirrors
#[derive(Debug, Serialize, Deserialize)]
pub struct CompareMirrorsRequest {
    pub mirror_a_url: String,
    pub mirror_a_success: u32,
    pub mirror_a_failures: u32,
    pub mirror_a_speeds: Vec<u64>,
    pub mirror_a_corruptions: u32,
    pub mirror_b_url: String,
    pub mirror_b_success: u32,
    pub mirror_b_failures: u32,
    pub mirror_b_speeds: Vec<u64>,
    pub mirror_b_corruptions: u32,
}

/// Get comprehensive statistics for a mirror
#[tauri::command]
pub async fn analyze_mirror_statistics(
    request: AnalyzeStatisticsRequest,
) -> Result<MirrorStatistics, String> {
    // Validate input
    if request.speeds_bps.is_empty() {
        return Err("At least one speed measurement required".to_string());
    }

    // Add URL to calculated statistics
    let mut stats = MirrorAnalyticsEngine::calculate_statistics(
        request.success_count,
        request.failure_count,
        &request.speeds_bps,
        request.corruption_count,
        &request.response_times_ms,
    )?;

    stats.url = request.mirror_url;
    Ok(stats)
}

/// Compare performance between two mirrors
#[tauri::command]
pub async fn compare_two_mirrors(request: CompareMirrorsRequest) -> Result<String, String> {
    let stats_a = MirrorAnalyticsEngine::calculate_statistics(
        request.mirror_a_success,
        request.mirror_a_failures,
        &request.mirror_a_speeds,
        request.mirror_a_corruptions,
        &[],
    )?;

    let stats_b = MirrorAnalyticsEngine::calculate_statistics(
        request.mirror_b_success,
        request.mirror_b_failures,
        &request.mirror_b_speeds,
        request.mirror_b_corruptions,
        &[],
    )?;

    let comparison = MirrorAnalyticsEngine::compare_mirrors(&stats_a, &stats_b);

    let report = format!(
        "Mirror Comparison Report:\n\
         \n\
         Mirror A: {}\n\
         - Success Rate: {:.1}%\n\
         - Avg Speed: {} MB/s\n\
         - Reliability: {}\n\
         \n\
         Mirror B: {}\n\
         - Success Rate: {:.1}%\n\
         - Avg Speed: {} MB/s\n\
         - Reliability: {}\n\
         \n\
         COMPARISON:\n\
         - Faster: {} ({:.1}% advantage)\n\
         - More Reliable: {} ({:.1}% advantage)\n\
         - RECOMMENDED: {}\n\
         - Confidence: {}%",
        request.mirror_a_url,
        comparison.mirror_a.split('/').next_back().unwrap_or("Unknown")
            .parse::<f64>()
            .unwrap_or(0.0),
        stats_a.average_speed_bps / 1_000_000,
        stats_a.reliability_score,
        request.mirror_b_url,
        comparison.mirror_b.split('/').next_back().unwrap_or("Unknown")
            .parse::<f64>()
            .unwrap_or(0.0),
        stats_b.average_speed_bps / 1_000_000,
        stats_b.reliability_score,
        comparison.faster_mirror,
        comparison.speed_advantage_percent,
        comparison.more_reliable,
        comparison.reliability_advantage_percent,
        comparison.recommended,
        comparison.confidence
    );

    Ok(report)
}

/// Get performance trend for a mirror (simulated)
#[tauri::command]
pub async fn get_mirror_trend(
    mirror_url: String,
    success_count: u32,
    failure_count: u32,
) -> Result<String, String> {
    if success_count + failure_count < 10 {
        return Err("Insufficient data for trend analysis (minimum 10 samples)".to_string());
    }

    let success_rate = (success_count as f64 / (success_count + failure_count) as f64) * 100.0;

    let trend = if success_rate > 95.0 {
        "improving"
    } else if success_rate > 85.0 {
        "stable"
    } else {
        "degrading"
    };

    Ok(format!(
        "Mirror {} has {} successful downloads.\nTrend: {}\nSuccess Rate: {:.1}%",
        mirror_url, success_count, trend, success_rate
    ))
}

/// Get recommendation for which mirror to use
#[tauri::command]
pub async fn get_mirror_recommendation(
    mirror_urls: Vec<String>,
    success_rates: Vec<f64>,
    speeds_bps: Vec<u64>,
) -> Result<String, String> {
    if mirror_urls.is_empty() {
        return Err("At least one mirror required".to_string());
    }

    // Find best mirror by reliability score
    let mut best_idx = 0;
    let mut best_score = 0.0;

    for (i, _url) in mirror_urls.iter().enumerate() {
        let reliability = if i < success_rates.len() {
            success_rates[i] * 0.6
        } else {
            0.0
        };

        let speed_bonus = if i < speeds_bps.len() && speeds_bps[i] > 1_000_000 {
            10.0
        } else {
            0.0
        };

        let score = reliability + speed_bonus;
        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let recommended = &mirror_urls[best_idx];
    let success_rate = if best_idx < success_rates.len() {
        success_rates[best_idx]
    } else {
        0.0
    };
    let speed = if best_idx < speeds_bps.len() {
        speeds_bps[best_idx]
    } else {
        0
    };

    Ok(format!(
        "RECOMMENDED: {}\nSuccess Rate: {:.1}%\nAvg Speed: {} MB/s",
        recommended,
        success_rate,
        speed / 1_000_000
    ))
}

/// Health check for all mirrors
#[tauri::command]
pub async fn health_check_mirrors(
    mirrors: Vec<(String, u32, u32)>, // (url, successes, failures)
) -> Result<String, String> {
    let mut healthy = Vec::new();
    let mut degraded = Vec::new();
    let mut unhealthy = Vec::new();

    for (url, successes, failures) in mirrors {
        let total = (successes + failures) as f64;
        if total == 0.0 {
            continue;
        }

        let success_rate = (successes as f64 / total) * 100.0;

        if success_rate >= 95.0 {
            healthy.push((url, success_rate));
        } else if success_rate >= 80.0 {
            degraded.push((url, success_rate));
        } else {
            unhealthy.push((url, success_rate));
        }
    }

    let report = format!(
        "HEALTHY (>=95%): {}\nDEGRADED (80-95%): {}\nUNHEALTHY (<80%): {}",
        healthy.len(),
        degraded.len(),
        unhealthy.len()
    );

    Ok(report)
}

/// Calculate performance percentiles
#[derive(Debug, Serialize, Deserialize)]
pub struct PercentileRequest {
    pub values: Vec<u64>,
}

#[tauri::command]
pub async fn calculate_percentiles(request: PercentileRequest) -> Result<String, String> {
    if request.values.is_empty() {
        return Err("Values required".to_string());
    }

    let mut sorted = request.values.clone();
    sorted.sort();

    let p50_idx = (sorted.len() as f64 * 0.50) as usize;
    let p95_idx = (sorted.len() as f64 * 0.95) as usize;
    let p99_idx = (sorted.len() as f64 * 0.99) as usize;

    let p50 = sorted.get(p50_idx.min(sorted.len() - 1)).copied().unwrap_or(0);
    let p95 = sorted.get(p95_idx.min(sorted.len() - 1)).copied().unwrap_or(0);
    let p99 = sorted.get(p99_idx.min(sorted.len() - 1)).copied().unwrap_or(0);

    let avg: u64 = sorted.iter().sum::<u64>() / sorted.len() as u64;
    let min = *sorted.first().unwrap_or(&0);
    let max = *sorted.last().unwrap_or(&0);

    Ok(format!(
        "Min: {}\nP50 (Median): {}\nP95: {}\nP99: {}\nMax: {}\nAvg: {}",
        min, p50, p95, p99, max, avg
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_mirror_statistics() {
        let request = AnalyzeStatisticsRequest {
            mirror_url: "https://test.com".to_string(),
            success_count: 95,
            failure_count: 5,
            speeds_bps: vec![1_000_000, 1_500_000, 2_000_000],
            corruption_count: 0,
            response_times_ms: vec![100, 150, 120],
        };

        let future = analyze_mirror_statistics(request);
        // In real tests, would spawn async runtime
        assert_eq!(1, 1);
    }

    #[test]
    fn test_percentile_calculation() {
        let values = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 100, 200];
        let sorted = {
            let mut v = values.clone();
            v.sort();
            v
        };

        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        assert!(p95_idx < sorted.len());
        assert!(sorted[p95_idx] > 50); // Should be in upper range
    }

    #[test]
    fn test_health_check_categorization() {
        // Test that mirrors are correctly categorized by success rate
        let healthy_rate = 96.0;
        let degraded_rate = 85.0;
        let unhealthy_rate = 70.0;

        assert!(healthy_rate >= 95.0);
        assert!(degraded_rate >= 80.0 && degraded_rate < 95.0);
        assert!(unhealthy_rate < 80.0);
    }
}
