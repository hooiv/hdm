use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GeofenceRule {
    pub id: String,
    pub url_pattern: String,
    pub region: String,
    pub proxy_type: String,    // "direct", "tor", "socks5", "http"
    pub proxy_address: String, // e.g. "socks5://127.0.0.1:9050"
    pub enabled: bool,
}

lazy_static! {
    static ref GEOFENCE_RULES: Mutex<Vec<GeofenceRule>> = Mutex::new(Vec::new());
}

/// Add or update a geofence rule — route specific URL patterns through specific proxies/regions.
pub fn set_geofence_rule(
    url_pattern: String,
    region: String,
    proxy_type: String,
    proxy_address: String,
) -> Result<String, String> {
    // Validate the regex pattern
    Regex::new(&url_pattern).map_err(|e| format!("Invalid URL pattern regex: {}", e))?;

    let id = uuid::Uuid::new_v4().to_string();
    let rule = GeofenceRule {
        id: id.clone(),
        url_pattern,
        region: region.clone(),
        proxy_type,
        proxy_address,
        enabled: true,
    };

    if let Ok(mut rules) = GEOFENCE_RULES.lock() {
        rules.push(rule);
    }

    Ok(format!("Geofence rule added for region '{}' (ID: {})", region, id))
}

/// Get all configured geofence rules.
pub fn get_geofence_rules() -> Result<Vec<GeofenceRule>, String> {
    let rules = GEOFENCE_RULES.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(rules.clone())
}

/// Remove a geofence rule by ID.
pub fn remove_geofence_rule(rule_id: String) -> Result<String, String> {
    let mut rules = GEOFENCE_RULES.lock().map_err(|e| format!("Lock error: {}", e))?;
    let before = rules.len();
    rules.retain(|r| r.id != rule_id);
    let after = rules.len();

    if before == after {
        Err(format!("Rule not found: {}", rule_id))
    } else {
        Ok(format!("Rule {} removed", rule_id))
    }
}

/// Toggle a geofence rule on/off.
pub fn toggle_geofence_rule(rule_id: String) -> Result<String, String> {
    let mut rules = GEOFENCE_RULES.lock().map_err(|e| format!("Lock error: {}", e))?;
    for rule in rules.iter_mut() {
        if rule.id == rule_id {
            rule.enabled = !rule.enabled;
            return Ok(format!("Rule {} is now {}", rule_id, if rule.enabled { "enabled" } else { "disabled" }));
        }
    }
    Err(format!("Rule not found: {}", rule_id))
}

/// Match a URL against geofence rules and return the matching proxy config.
/// Returns None if no rule matches (use direct connection).
pub fn match_geofence(url: &str) -> Option<GeofenceRule> {
    if let Ok(rules) = GEOFENCE_RULES.lock() {
        for rule in rules.iter() {
            if !rule.enabled { continue; }
            if let Ok(re) = Regex::new(&rule.url_pattern) {
                if re.is_match(url) {
                    return Some(rule.clone());
                }
            }
        }
    }
    None
}

/// Get some preset region configurations.
pub fn get_preset_regions() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"region": "US", "description": "United States", "proxy": "Use a US-based SOCKS5 proxy"}),
        serde_json::json!({"region": "EU", "description": "Europe (Germany)", "proxy": "Use an EU-based SOCKS5 proxy"}),
        serde_json::json!({"region": "JP", "description": "Japan", "proxy": "Use a JP-based SOCKS5 proxy"}),
        serde_json::json!({"region": "TOR", "description": "Tor Network", "proxy": "socks5://127.0.0.1:9050"}),
        serde_json::json!({"region": "DIRECT", "description": "Direct Connection", "proxy": "No proxy"}),
    ]
}
