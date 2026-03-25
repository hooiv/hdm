// network_diagnostics.rs — Advanced network diagnostics and failure analysis
//
// Provides proactive network health monitoring, failure detection, and diagnostics

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

/// Network connectivity test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityTest {
    pub test_id: String,
    pub target: String,
    pub success: bool,
    pub latency_ms: u64,
    pub timestamp: u64,
    pub error_message: Option<String>,
    pub test_type: ConnectivityTestType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectivityTestType {
    DNS,
    Ping,
    HTTP,
    HTTPS,
}

/// Network health snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealth {
    pub timestamp: u64,
    pub is_online: bool,
    pub latency_ms: u64,
    pub packet_loss_percent: f32,
    pub jitter_ms: f32,
    pub dns_working: bool,
    pub ipv4_available: bool,
    pub ipv6_available: bool,
    pub active_connections: u32,
    pub recent_errors: Vec<String>,
}

/// Network behavior pattern
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkPattern {
    Stable,
    Flaky,      // Intermittent connectivity issues
    Degraded,   // Slow but connected
    Offline,
    UnknownPattern,
}

/// Diagnostic report for a network issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub issue_id: String,
    pub timestamp: u64,
    pub error_type: String,
    pub affected_urls: Vec<String>,
    pub diagnostic_tests: Vec<ConnectivityTest>,
    pub network_pattern: NetworkPattern,
    pub recommendations: Vec<String>,
    pub root_cause_hypothesis: String,
}

/// Network anomaly detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetection {
    pub anomaly_id: String,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub timestamp: u64,
    pub affected_download_ids: Vec<String>,
    pub metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnomalyType {
    SuddenLatencySpike,
    PacketLossIncrease,
    DnsFailurePattern,
    ConnectionRefusalPattern,
    RateLimitingDetected,
    GeoBlockDetected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Network diagnostics engine
pub struct NetworkDiagnostics {
    test_history: Arc<RwLock<VecDeque<ConnectivityTest>>>,
    network_health_history: Arc<RwLock<VecDeque<NetworkHealth>>>,
    diagnostic_reports: Arc<RwLock<Vec<DiagnosticReport>>>,
    anomalies: Arc<RwLock<Vec<AnomalyDetection>>>,
    max_samples: usize,
}

impl NetworkDiagnostics {
    pub fn new() -> Self {
        Self {
            test_history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            network_health_history: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            diagnostic_reports: Arc::new(RwLock::new(Vec::new())),
            anomalies: Arc::new(RwLock::new(Vec::new())),
            max_samples: 1000,
        }
    }

    /// Record connectivity test result
    pub fn record_test(&self, test: ConnectivityTest) {
        let mut history = self.test_history.write().unwrap();
        if history.len() >= self.max_samples {
            history.pop_front();
        }
        history.push_back(test);
    }

    /// Record network health snapshot
    pub fn record_health(&self, health: NetworkHealth) {
        let mut history = self.network_health_history.write().unwrap();
        if history.len() >= 100 {
            history.pop_front();
        }
        history.push_back(health);
    }

    /// Create diagnostic report
    pub fn create_diagnostic_report(
        &self,
        error_type: &str,
        affected_urls: Vec<String>,
        tests: Vec<ConnectivityTest>,
    ) -> DiagnosticReport {
        let pattern = self.detect_network_pattern();
        let recommendations = self.generate_recommendations(&pattern, error_type);

        let report = DiagnosticReport {
            issue_id: format!("diag-{}", current_timestamp_ms()),
            timestamp: current_timestamp_ms(),
            error_type: error_type.to_string(),
            affected_urls,
            diagnostic_tests: tests,
            network_pattern: pattern,
            recommendations,
            root_cause_hypothesis: self.hypothesize_root_cause(&pattern, error_type),
        };

        self.diagnostic_reports.write().unwrap().push(report.clone());
        report
    }

    /// Detect current network pattern
    pub fn detect_network_pattern(&self) -> NetworkPattern {
        let history = self.test_history.read().unwrap();
        if history.is_empty() {
            return NetworkPattern::UnknownPattern;
        }

        let recent: Vec<_> = history.iter().rev().take(20).collect();

        let success_rate = recent.iter().filter(|t| t.success).count() as f32 / recent.len() as f32;

        if success_rate < 0.5 {
            return NetworkPattern::Offline;
        }

        if success_rate < 0.9 {
            return NetworkPattern::Flaky;
        }

        let avg_latency: u64 = recent.iter().map(|t| t.latency_ms).sum::<u64>() / recent.len() as u64;

        if avg_latency > 5000 {
            NetworkPattern::Degraded
        } else if success_rate > 0.95 && avg_latency < 1000 {
            NetworkPattern::Stable
        } else {
            NetworkPattern::UnknownPattern
        }
    }

    /// Detect anomalies in network behavior
    pub fn detect_anomalies(&self, download_ids: Vec<String>) -> Vec<AnomalyDetection> {
        let mut detected = Vec::new();
        let history = self.test_history.read().unwrap();

        if history.len() < 10 {
            return detected;
        }

        // Check for latency spikes
        let recent: Vec<u64> = history.iter().rev().take(20).map(|t| t.latency_ms).collect();
        let avg_latency = recent.iter().sum::<u64>() / recent.len() as u64;
        let max_latency = *recent.iter().max().unwrap_or(&0);

        if max_latency > avg_latency * 3 {
            detected.push(AnomalyDetection {
                anomaly_id: format!("anomaly-spike-{}", current_timestamp_ms()),
                anomaly_type: AnomalyType::SuddenLatencySpike,
                severity: AnomalySeverity::High,
                timestamp: current_timestamp_ms(),
                affected_download_ids: download_ids.clone(),
                metrics: {
                    let mut m = HashMap::new();
                    m.insert("avg_latency".to_string(), avg_latency as f64);
                    m.insert("max_latency".to_string(), max_latency as f64);
                    m.insert("spike_ratio".to_string(), (max_latency as f64) / (avg_latency as f64));
                    m
                },
            });
        }

        // Check for packet loss
        let loss_rate = history.iter().rev().take(20).filter(|t| !t.success).count() as f32 / 20.0;
        if loss_rate > 0.1 {
            detected.push(AnomalyDetection {
                anomaly_id: format!("anomaly-loss-{}", current_timestamp_ms()),
                anomaly_type: AnomalyType::PacketLossIncrease,
                severity: if loss_rate > 0.3 { AnomalySeverity::Critical } else { AnomalySeverity::Medium },
                timestamp: current_timestamp_ms(),
                affected_download_ids: download_ids.clone(),
                metrics: {
                    let mut m = HashMap::new();
                    m.insert("loss_rate".to_string(), loss_rate as f64);
                    m
                },
            });
        }

        self.anomalies.write().unwrap().extend(detected.clone());
        detected
    }

    /// Generate recovery recommendations
    fn generate_recommendations(&self, pattern: &NetworkPattern, error_type: &str) -> Vec<String> {
        let mut recs = Vec::new();

        match pattern {
            NetworkPattern::Offline => {
                recs.push("No internet connection detected. Check your network connection.".to_string());
                recs.push("Try switching to a different network or waiting a few seconds.".to_string());
            }
            NetworkPattern::Flaky => {
                recs.push("Your connection is unstable. Enable smaller chunk sizes for more reliable downloads.".to_string());
                recs.push("Consider enabling Tor or a VPN for more stable connections.".to_string());
                recs.push("Try adjusting the number of concurrent segments down.".to_string());
            }
            NetworkPattern::Degraded => {
                recs.push("Your connection is slow. Downloads will take longer but should complete.".to_string());
                recs.push("Reduce concurrent segments to improve reliability on slow connections.".to_string());
            }
            _ => {}
        }

        // Add error-specific recommendations
        if error_type.contains("DNS") {
            recs.push("Try changing your DNS provider (e.g., 8.8.8.8 or 1.1.1.1).".to_string());
        }
        if error_type.contains("429") || error_type.contains("Rate") {
            recs.push("Server is rate-limiting you. Enable automatic retry with exponential backoff.".to_string());
            recs.push("Try reducing concurrent connections or adding delays between requests.".to_string());
        }
        if error_type.contains("SSL") || error_type.contains("TLS") {
            recs.push("Update your system's root certificates or disable TLS verification (not recommended).".to_string());
        }

        recs
    }

    fn hypothesize_root_cause(&self, pattern: &NetworkPattern, error_type: &str) -> String {
        match pattern {
            NetworkPattern::Offline => "No internet connectivity".to_string(),
            NetworkPattern::Flaky => "Intermittent network issues detected".to_string(),
            NetworkPattern::Degraded => "Slow network connection".to_string(),
            NetworkPattern::Stable => {
                if error_type.contains("DNS") {
                    "DNS resolution issue despite network stability".to_string()
                } else if error_type.contains("Rate") {
                    "Rate limiting from server".to_string()
                } else if error_type.contains("SSL") {
                    "TLS/SSL certificate verification issue".to_string()
                } else {
                    "Likely server-side issue".to_string()
                }
            }
            NetworkPattern::UnknownPattern => "Unable to determine root cause".to_string(),
        }
    }

    /// Get recent diagnostic reports
    pub fn get_recent_reports(&self, limit: usize) -> Vec<DiagnosticReport> {
        self.diagnostic_reports
            .read()
            .unwrap()
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get recent anomalies
    pub fn get_recent_anomalies(&self, limit: usize) -> Vec<AnomalyDetection> {
        self.anomalies
            .read()
            .unwrap()
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get network health summary
    pub fn get_health_summary(&self) -> Option<NetworkHealth> {
        self.network_health_history
            .read()
            .unwrap()
            .back()
            .cloned()
    }

    /// Export diagnostic data for analysis
    pub fn export_diagnostics_summary(&self) -> DiagnosticsSummary {
        DiagnosticsSummary {
            total_tests: self.test_history.read().unwrap().len(),
            total_reports: self.diagnostic_reports.read().unwrap().len(),
            total_anomalies: self.anomalies.read().unwrap().len(),
            current_pattern: self.detect_network_pattern(),
            current_health: self.get_health_summary(),
            recent_anomalies: self.get_recent_anomalies(5),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSummary {
    pub total_tests: usize,
    pub total_reports: usize,
    pub total_anomalies: usize,
    pub current_pattern: NetworkPattern,
    pub current_health: Option<NetworkHealth>,
    pub recent_anomalies: Vec<AnomalyDetection>,
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_detection() {
        let diagnostics = NetworkDiagnostics::new();

        // Record stable tests
        for i in 0..20 {
            diagnostics.record_test(ConnectivityTest {
                test_id: format!("test-{}", i),
                target: "example.com".to_string(),
                success: true,
                latency_ms: 50,
                timestamp: current_timestamp_ms(),
                error_message: None,
                test_type: ConnectivityTestType::HTTP,
            });
        }

        let pattern = diagnostics.detect_network_pattern();
        assert_eq!(pattern, NetworkPattern::Stable);
    }

    #[test]
    fn test_anomaly_detection() {
        let diagnostics = NetworkDiagnostics::new();

        // Record mixed results
        for i in 0..15 {
            let success = i % 3 != 0; // 2 out of 3 succeed
            diagnostics.record_test(ConnectivityTest {
                test_id: format!("test-{}", i),
                target: "example.com".to_string(),
                success,
                latency_ms: if success { 100 } else { 0 },
                timestamp: current_timestamp_ms(),
                error_message: if !success { Some("Timeout".to_string()) } else { None },
                test_type: ConnectivityTestType::HTTP,
            });
        }

        let anomalies = diagnostics.detect_anomalies(vec!["dl1".to_string()]);
        assert!(!anomalies.is_empty());
    }
}
