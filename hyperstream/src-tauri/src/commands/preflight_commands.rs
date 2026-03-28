/// Tauri commands for Pre-Flight Analysis
use crate::preflight_analysis::{get_analyzer, PreFlightAnalysis};
use tauri::command;

#[command]
pub async fn analyze_url_preflight(url: String) -> Result<PreFlightAnalysis, String> {
    let analyzer = get_analyzer();
    analyzer.analyze(&url).await
}

#[command]
pub async fn analyze_multiple_urls(urls: Vec<String>) -> Result<Vec<PreFlightAnalysis>, String> {
    let analyzer = get_analyzer();
    let mut results = Vec::new();

    for url in urls {
        match analyzer.analyze(&url).await {
            Ok(analysis) => results.push(analysis),
            Err(e) => {
                // Continue with other URLs even if one fails
                eprintln!("Failed to analyze {}: {}", url, e);
            }
        }
    }

    Ok(results)
}

#[command]
pub fn get_preflight_recommendations(analysis: PreFlightAnalysis) -> Vec<String> {
    analysis
        .recommendations
        .iter()
        .enumerate()
        .map(|(idx, rec)| {
            format!(
                "{}. [{}] {} - Expected: {}",
                idx + 1,
                rec.category.to_uppercase(),
                rec.suggestion,
                rec.expected_benefit
            )
        })
        .collect()
}

#[command]
pub fn get_preflight_analysis_summary(analysis: PreFlightAnalysis) -> String {
    format!(
        r#"📊 PRE-FLIGHT ANALYSIS SUMMARY
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📁 File: {:?}
📦 Size: {}
🔗 URL: {}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
⚡ CONNECTIVITY & SPEED
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Health: {:?}
DNS Latency: {} ms
TCP Latency: {} ms
TLS Latency: {} ms
Pre-test Speed: {:.2} MB/s

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 RISK ASSESSMENT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Success Probability: {:.1}%
Risk Level: {:?}
Reliability Score: {:.0}%
Availability Score: {:.0}%

Risk Factors:
{}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🎯 OPTIMAL STRATEGY
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{}

Estimated Duration: {}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
💡 RECOMMENDATIONS
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🌐 MIRRORS AVAILABLE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{}

Analysis completed in {} ms
"#,
        analysis.file_name,
        analysis
            .file_size_bytes
            .map(|b| format_bytes(b))
            .unwrap_or_else(|| "Unknown".to_string()),
        analysis.url,
        analysis.connection_health,
        analysis.dns_latency_ms.unwrap_or(0),
        analysis.tcp_latency_ms.unwrap_or(0),
        analysis.tls_latency_ms.unwrap_or(0),
        analysis.pre_test_speed_mbps.unwrap_or(0.0),
        analysis.success_probability * 100.0,
        analysis.risk_level,
        analysis.reliability_score,
        analysis.availability_score,
        analysis
            .risk_factors
            .iter()
            .map(|f| format!("  ⚠️ {}", f))
            .collect::<Vec<_>>()
            .join("\n"),
        analysis.optimal_strategy,
        analysis
            .estimated_duration_seconds
            .map(|s| format_duration(s))
            .unwrap_or_else(|| "Calculating...".to_string()),
        analysis
            .recommendations
            .iter()
            .enumerate()
            .map(|(idx, rec)| format!(
                "  {}. [{}] {} → {}",
                idx + 1,
                rec.category,
                rec.suggestion,
                rec.expected_benefit
            ))
            .collect::<Vec<_>>()
            .join("\n"),
        analysis
            .detected_mirrors
            .iter()
            .enumerate()
            .map(|(idx, m)| format!(
                "  {}. {} ({}) - Health: {:.0}%",
                idx + 1,
                m.host,
                m.protocol,
                m.health_score * 100.0
            ))
            .collect::<Vec<_>>()
            .join("\n"),
        analysis.analysis_duration_ms,
    )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{} min {:.0} sec", mins, secs)
    } else {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{} hours {} min", hours, mins)
    }
}
