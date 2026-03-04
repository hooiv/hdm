use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use lazy_static::lazy_static;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GeofenceRule {
    pub id: String,
    pub url_pattern: String,
    pub region: String,
    pub proxy_type: String,    // "direct", "tor", "socks5", "http"
    pub proxy_address: String, // e.g. "socks5://127.0.0.1:9050"
    pub enabled: bool,
}

/// Internal rule with pre-compiled regex to avoid recompilation on every match
struct CompiledRule {
    rule: GeofenceRule,
    compiled_re: regex::Regex,
}

lazy_static! {
    static ref GEOFENCE_RULES: Mutex<Vec<CompiledRule>> = Mutex::new(Vec::new());
}

/// Add or update a geofence rule — route specific URL patterns through specific proxies/regions.
pub fn set_geofence_rule(
    url_pattern: String,
    region: String,
    proxy_type: String,
    proxy_address: String,
) -> Result<String, String> {
    // Reject excessively long patterns to mitigate ReDoS
    if url_pattern.len() > 512 {
        return Err("URL pattern too long (max 512 characters)".to_string());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let compiled_re = regex::RegexBuilder::new(&url_pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| format!("Invalid URL pattern regex: {}", e))?;
    let rule = GeofenceRule {
        id: id.clone(),
        url_pattern,
        region: region.clone(),
        proxy_type,
        proxy_address,
        enabled: true,
    };

    let mut rules = GEOFENCE_RULES.lock().unwrap_or_else(|e| e.into_inner());
    rules.push(CompiledRule { rule, compiled_re });

    Ok(format!("Geofence rule added for region '{}' (ID: {})", region, id))
}

/// Get all configured geofence rules.
pub fn get_geofence_rules() -> Result<Vec<GeofenceRule>, String> {
    let rules = GEOFENCE_RULES.lock().unwrap_or_else(|e| e.into_inner());
    Ok(rules.iter().map(|cr| cr.rule.clone()).collect())
}

/// Remove a geofence rule by ID.
pub fn remove_geofence_rule(rule_id: String) -> Result<String, String> {
    let mut rules = GEOFENCE_RULES.lock().unwrap_or_else(|e| e.into_inner());
    let before = rules.len();
    rules.retain(|cr| cr.rule.id != rule_id);
    let after = rules.len();

    if before == after {
        Err(format!("Rule not found: {}", rule_id))
    } else {
        Ok(format!("Rule {} removed", rule_id))
    }
}

/// Toggle a geofence rule on/off.
pub fn toggle_geofence_rule(rule_id: String) -> Result<String, String> {
    let mut rules = GEOFENCE_RULES.lock().unwrap_or_else(|e| e.into_inner());
    for cr in rules.iter_mut() {
        if cr.rule.id == rule_id {
            cr.rule.enabled = !cr.rule.enabled;
            return Ok(format!("Rule {} is now {}", rule_id, if cr.rule.enabled { "enabled" } else { "disabled" }));
        }
    }
    Err(format!("Rule not found: {}", rule_id))
}

/// Match a URL against geofence rules and return the matching proxy config.
/// Returns None if no rule matches (use direct connection).
pub fn match_geofence(url: &str) -> Option<GeofenceRule> {
    let rules = GEOFENCE_RULES.lock().unwrap_or_else(|e| e.into_inner());
    for cr in rules.iter() {
        if !cr.rule.enabled { continue; }
        if cr.compiled_re.is_match(url) {
            return Some(cr.rule.clone());
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
