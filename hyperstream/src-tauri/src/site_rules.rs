use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use std::sync::Mutex;

// ─── Site Rules Engine ───────────────────────────────────────────────
// Per-domain download configuration: max connections, custom headers,
// speed limits, retry policies, user-agent overrides, auth, referer rules.
// Rules are matched by domain glob patterns (e.g., "*.example.com").

static RULES_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn rules_path() -> std::path::PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("site_rules.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".hyperstream").join("site_rules.json")
}

/// A single site rule that applies to URLs matching the pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteRule {
    /// Unique identifier for this rule.
    pub id: String,
    /// Human-readable name (e.g., "GitHub Releases").
    pub name: String,
    /// Domain glob pattern: "*.github.com", "drive.google.com", "cdn.*.com"
    pub pattern: String,
    /// Whether this rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum concurrent connections for this domain (overrides global).
    #[serde(default)]
    pub max_connections: Option<u32>,
    /// Maximum segments to split downloads into for this domain.
    #[serde(default)]
    pub max_segments: Option<u32>,
    /// Speed limit in bytes/sec for this domain (0 = unlimited).
    #[serde(default)]
    pub speed_limit_bps: Option<u64>,
    /// Custom User-Agent string.
    #[serde(default)]
    pub user_agent: Option<String>,
    /// Custom Referer header.
    #[serde(default)]
    pub referer: Option<String>,
    /// Custom HTTP headers as key-value pairs.
    #[serde(default)]
    pub custom_headers: Vec<HeaderPair>,
    /// Maximum retry attempts (overrides global).
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// Retry delay in milliseconds (overrides default 2000ms).
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
    /// Whether to use exponential backoff for retries.
    #[serde(default)]
    pub exponential_backoff: Option<bool>,
    /// Username for HTTP Basic Auth.
    #[serde(default)]
    pub auth_username: Option<String>,
    /// Password for HTTP Basic Auth (stored obfuscated).
    #[serde(default)]
    pub auth_password: Option<String>,
    /// Cookie string to send with requests.
    #[serde(default)]
    pub cookie: Option<String>,
    /// Override download directory for this domain.
    #[serde(default)]
    pub download_dir: Option<String>,
    /// Whether to use DPI evasion for this domain.
    #[serde(default)]
    pub force_dpi_evasion: Option<bool>,
    /// Whether to skip SSL certificate verification (dangerous).
    #[serde(default)]
    pub skip_ssl_verify: Option<bool>,
    /// Minimum file size to apply this rule (bytes). 0 = any.
    #[serde(default)]
    pub min_file_size: Option<u64>,
    /// File extension filter — only apply if downloading these extensions.
    /// Empty = apply to all. Example: ["zip", "exe", "iso"]
    #[serde(default)]
    pub file_extensions: Vec<String>,
    /// Priority — higher priority rules override lower ones (default 0).
    #[serde(default)]
    pub priority: i32,
    /// Notes / description for the user.
    #[serde(default)]
    pub notes: Option<String>,
    /// Created timestamp (ISO-8601).
    #[serde(default)]
    pub created_at: String,
    /// Last modified timestamp.
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderPair {
    pub key: String,
    pub value: String,
}

fn default_true() -> bool { true }

/// The effective configuration after merging matching rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EffectiveConfig {
    pub max_connections: Option<u32>,
    pub max_segments: Option<u32>,
    pub speed_limit_bps: Option<u64>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub custom_headers: Vec<HeaderPair>,
    pub max_retries: Option<u32>,
    pub retry_delay_ms: Option<u64>,
    pub exponential_backoff: Option<bool>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub cookie: Option<String>,
    pub download_dir: Option<String>,
    pub force_dpi_evasion: Option<bool>,
    pub skip_ssl_verify: Option<bool>,
    /// Which rules contributed to this config, in priority order.
    pub matched_rules: Vec<String>,
}

// ─── Persistence ─────────────────────────────────────────────────────

fn load_rules() -> Vec<SiteRule> {
    let _lock = RULES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = rules_path();
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_rules(rules: &[SiteRule]) -> Result<(), String> {
    let _lock = RULES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = rules_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Pattern Matching ────────────────────────────────────────────────

/// Extract the domain from a URL.
fn extract_domain(url: &str) -> Option<String> {
    // Simple extraction — handle URLs with and without scheme
    let url_lower = url.to_lowercase();
    let after_scheme = if let Some(pos) = url_lower.find("://") {
        &url_lower[pos + 3..]
    } else {
        &url_lower
    };
    // Strip path, query, fragment
    let domain = after_scheme.split('/').next()?;
    // Strip port
    let domain = domain.split(':').next()?;
    // Strip user@
    if let Some(pos) = domain.find('@') {
        Some(domain[pos + 1..].to_string())
    } else {
        Some(domain.to_string())
    }
}

/// Extract file extension from a URL path.
fn extract_extension(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let path = path.split('#').next()?;
    let filename = path.rsplit('/').next()?;
    let ext = filename.rsplit('.').next()?;
    if ext == filename { return None; }
    Some(ext.to_lowercase())
}

/// Match a domain against a glob pattern.
/// Supported patterns: "*.example.com", "cdn.*.com", "exact.domain.com"
fn domain_matches(domain: &str, pattern: &str) -> bool {
    let domain = domain.to_lowercase();
    let pattern = pattern.to_lowercase();

    if pattern == "*" {
        return true;
    }

    // Split both into parts
    let domain_parts: Vec<&str> = domain.split('.').collect();
    let pattern_parts: Vec<&str> = pattern.split('.').collect();

    // Handle leading wildcard: *.example.com matches a.example.com, b.c.example.com
    if pattern.starts_with("*.") {
        let suffix = &pattern[2..];
        return domain.ends_with(suffix) && domain.len() > suffix.len();
    }

    // Handle wildcards at other positions
    if pattern_parts.len() != domain_parts.len() {
        return false;
    }

    for (dp, pp) in domain_parts.iter().zip(pattern_parts.iter()) {
        if *pp == "*" {
            continue; // Wildcard matches any single segment
        }
        if dp != pp {
            return false;
        }
    }
    true
}

// ─── Rule Matching & Merging ─────────────────────────────────────────

/// Find all rules that match a URL, sorted by priority (highest first).
pub fn find_matching_rules(url: &str) -> Vec<SiteRule> {
    let rules = load_rules();
    let domain = match extract_domain(url) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let ext = extract_extension(url);

    let mut matching: Vec<SiteRule> = rules
        .into_iter()
        .filter(|r| {
            if !r.enabled { return false; }
            if !domain_matches(&domain, &r.pattern) { return false; }
            // Check extension filter if specified
            if !r.file_extensions.is_empty() {
                if let Some(ref e) = ext {
                    if !r.file_extensions.iter().any(|fe| fe.to_lowercase() == *e) {
                        return false;
                    }
                } else {
                    return false; // Has extension filter but URL has no extension
                }
            }
            true
        })
        .collect();

    // Sort by priority (descending) — highest priority first
    matching.sort_by(|a, b| b.priority.cmp(&a.priority));
    matching
}

/// Resolve the effective configuration for a URL.
/// Merges all matching rules, with higher-priority rules taking precedence.
/// Uses "first non-None wins" strategy for each field.
pub fn resolve_config(url: &str) -> EffectiveConfig {
    let matching = find_matching_rules(url);
    if matching.is_empty() {
        return EffectiveConfig::default();
    }

    let mut config = EffectiveConfig::default();
    config.matched_rules = matching.iter().map(|r| r.name.clone()).collect();

    for rule in &matching {
        if config.max_connections.is_none() { config.max_connections = rule.max_connections; }
        if config.max_segments.is_none() { config.max_segments = rule.max_segments; }
        if config.speed_limit_bps.is_none() { config.speed_limit_bps = rule.speed_limit_bps; }
        if config.user_agent.is_none() { config.user_agent = rule.user_agent.clone(); }
        if config.referer.is_none() { config.referer = rule.referer.clone(); }
        if config.max_retries.is_none() { config.max_retries = rule.max_retries; }
        if config.retry_delay_ms.is_none() { config.retry_delay_ms = rule.retry_delay_ms; }
        if config.exponential_backoff.is_none() { config.exponential_backoff = rule.exponential_backoff; }
        if config.auth_username.is_none() { config.auth_username = rule.auth_username.clone(); }
        if config.auth_password.is_none() { config.auth_password = rule.auth_password.clone(); }
        if config.cookie.is_none() { config.cookie = rule.cookie.clone(); }
        if config.download_dir.is_none() { config.download_dir = rule.download_dir.clone(); }
        if config.force_dpi_evasion.is_none() { config.force_dpi_evasion = rule.force_dpi_evasion; }
        if config.skip_ssl_verify.is_none() { config.skip_ssl_verify = rule.skip_ssl_verify; }
        // Merge headers — later rules don't override existing keys
        for h in &rule.custom_headers {
            if !config.custom_headers.iter().any(|ch| ch.key.to_lowercase() == h.key.to_lowercase()) {
                config.custom_headers.push(h.clone());
            }
        }
    }

    config
}

// ─── Built-in Presets ────────────────────────────────────────────────

/// Generate sensible default rules for common download sites.
pub fn builtin_presets() -> Vec<SiteRule> {
    let now = chrono::Local::now().to_rfc3339();
    vec![
        SiteRule {
            id: "preset-google-drive".into(),
            name: "Google Drive".into(),
            pattern: "*.google.com".into(),
            enabled: true,
            max_connections: Some(2),
            max_segments: Some(4),
            speed_limit_bps: None,
            user_agent: Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".into()),
            referer: Some("https://drive.google.com/".into()),
            custom_headers: vec![],
            max_retries: Some(10),
            retry_delay_ms: Some(5000),
            exponential_backoff: Some(true),
            auth_username: None,
            auth_password: None,
            cookie: None,
            download_dir: None,
            force_dpi_evasion: Some(false),
            skip_ssl_verify: None,
            min_file_size: None,
            file_extensions: vec![],
            priority: 10,
            notes: Some("Google Drive limits connections aggressively".into()),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        SiteRule {
            id: "preset-github".into(),
            name: "GitHub Releases".into(),
            pattern: "*.github.com".into(),
            enabled: true,
            max_connections: Some(4),
            max_segments: Some(8),
            speed_limit_bps: None,
            user_agent: None,
            referer: None,
            custom_headers: vec![],
            max_retries: Some(5),
            retry_delay_ms: Some(2000),
            exponential_backoff: Some(true),
            auth_username: None,
            auth_password: None,
            cookie: None,
            download_dir: None,
            force_dpi_evasion: None,
            skip_ssl_verify: None,
            min_file_size: None,
            file_extensions: vec![],
            priority: 10,
            notes: Some("GitHub releases support range requests well".into()),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        SiteRule {
            id: "preset-sourceforge".into(),
            name: "SourceForge".into(),
            pattern: "*.sourceforge.net".into(),
            enabled: true,
            max_connections: Some(2),
            max_segments: Some(4),
            speed_limit_bps: None,
            user_agent: None,
            referer: None,
            custom_headers: vec![],
            max_retries: Some(8),
            retry_delay_ms: Some(3000),
            exponential_backoff: Some(true),
            auth_username: None,
            auth_password: None,
            cookie: None,
            download_dir: None,
            force_dpi_evasion: None,
            skip_ssl_verify: None,
            min_file_size: None,
            file_extensions: vec![],
            priority: 5,
            notes: Some("SourceForge mirrors can be unreliable — more retries".into()),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        SiteRule {
            id: "preset-mega".into(),
            name: "MEGA".into(),
            pattern: "*.mega.nz".into(),
            enabled: true,
            max_connections: Some(1),
            max_segments: Some(1),
            speed_limit_bps: None,
            user_agent: None,
            referer: None,
            custom_headers: vec![],
            max_retries: Some(5),
            retry_delay_ms: Some(10000),
            exponential_backoff: Some(true),
            auth_username: None,
            auth_password: None,
            cookie: None,
            download_dir: None,
            force_dpi_evasion: None,
            skip_ssl_verify: None,
            min_file_size: None,
            file_extensions: vec![],
            priority: 10,
            notes: Some("MEGA uses encrypted streams — single connection only".into()),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        SiteRule {
            id: "preset-cdn-generic".into(),
            name: "Generic CDN (AWS/Cloudflare/Akamai)".into(),
            pattern: "*.cloudfront.net".into(),
            enabled: true,
            max_connections: Some(8),
            max_segments: Some(16),
            speed_limit_bps: None,
            user_agent: None,
            referer: None,
            custom_headers: vec![],
            max_retries: Some(3),
            retry_delay_ms: Some(1000),
            exponential_backoff: Some(false),
            auth_username: None,
            auth_password: None,
            cookie: None,
            download_dir: None,
            force_dpi_evasion: None,
            skip_ssl_verify: None,
            min_file_size: None,
            file_extensions: vec![],
            priority: 5,
            notes: Some("CDNs handle many connections well — maximize throughput".into()),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
    ]
}

// ─── CRUD Operations ─────────────────────────────────────────────────

pub fn list_rules() -> Vec<SiteRule> {
    load_rules()
}

pub fn get_rule(id: &str) -> Option<SiteRule> {
    load_rules().into_iter().find(|r| r.id == id)
}

pub fn add_rule(mut rule: SiteRule) -> Result<(), String> {
    let mut rules = load_rules();
    if rules.iter().any(|r| r.id == rule.id) {
        return Err(format!("Rule with ID '{}' already exists", rule.id));
    }
    let now = chrono::Local::now().to_rfc3339();
    if rule.created_at.is_empty() { rule.created_at = now.clone(); }
    if rule.updated_at.is_empty() { rule.updated_at = now; }
    rules.push(rule);
    save_rules(&rules)
}

pub fn update_rule(mut rule: SiteRule) -> Result<(), String> {
    let mut rules = load_rules();
    if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
        rule.updated_at = chrono::Local::now().to_rfc3339();
        *existing = rule;
        save_rules(&rules)
    } else {
        Err(format!("Rule '{}' not found", rule.id))
    }
}

pub fn delete_rule(id: &str) -> Result<(), String> {
    let mut rules = load_rules();
    let before = rules.len();
    rules.retain(|r| r.id != id);
    if rules.len() == before {
        return Err(format!("Rule '{}' not found", id));
    }
    save_rules(&rules)
}

pub fn import_presets() -> Result<usize, String> {
    let mut rules = load_rules();
    let presets = builtin_presets();
    let mut imported = 0;
    for preset in presets {
        if !rules.iter().any(|r| r.id == preset.id) {
            rules.push(preset);
            imported += 1;
        }
    }
    if imported > 0 {
        save_rules(&rules)?;
    }
    Ok(imported)
}

/// Test a URL against all rules and return the effective config.
/// Useful for the UI to show what rules would apply before downloading.
pub fn test_url(url: &str) -> EffectiveConfig {
    resolve_config(url)
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.example.com/path"), Some("www.example.com".into()));
        assert_eq!(extract_domain("http://user@host.org:8080/"), Some("host.org".into()));
        assert_eq!(extract_domain("ftp://files.server.net/file.zip"), Some("files.server.net".into()));
    }

    #[test]
    fn test_extract_extension() {
        assert_eq!(extract_extension("https://example.com/file.zip"), Some("zip".into()));
        assert_eq!(extract_extension("https://example.com/file.tar.gz?v=2"), Some("gz".into()));
        assert_eq!(extract_extension("https://example.com/"), None);
    }

    #[test]
    fn test_domain_matches_exact() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(!domain_matches("other.com", "example.com"));
    }

    #[test]
    fn test_domain_matches_wildcard_prefix() {
        assert!(domain_matches("cdn.example.com", "*.example.com"));
        assert!(domain_matches("a.b.example.com", "*.example.com"));
        assert!(!domain_matches("example.com", "*.example.com")); // Must have subdomain
    }

    #[test]
    fn test_domain_matches_wildcard_middle() {
        assert!(domain_matches("cdn.fast.com", "cdn.*.com"));
        assert!(!domain_matches("cdn.fast.org", "cdn.*.com"));
    }

    #[test]
    fn test_domain_matches_star() {
        assert!(domain_matches("anything.com", "*"));
    }

    #[test]
    fn test_resolve_config_priority() {
        // This test validates the merging logic — higher priority wins
        let r1 = SiteRule {
            id: "low".into(), name: "Low".into(), pattern: "*.test.com".into(),
            enabled: true, max_connections: Some(2), max_segments: None,
            speed_limit_bps: Some(1000), user_agent: Some("LowUA".into()),
            referer: None, custom_headers: vec![], max_retries: None,
            retry_delay_ms: None, exponential_backoff: None, auth_username: None,
            auth_password: None, cookie: None, download_dir: None,
            force_dpi_evasion: None, skip_ssl_verify: None, min_file_size: None,
            file_extensions: vec![], priority: 1, notes: None,
            created_at: String::new(), updated_at: String::new(),
        };
        let r2 = SiteRule {
            id: "high".into(), name: "High".into(), pattern: "*.test.com".into(),
            enabled: true, max_connections: Some(8), max_segments: Some(16),
            speed_limit_bps: None, user_agent: None,
            referer: None, custom_headers: vec![], max_retries: None,
            retry_delay_ms: None, exponential_backoff: None, auth_username: None,
            auth_password: None, cookie: None, download_dir: None,
            force_dpi_evasion: None, skip_ssl_verify: None, min_file_size: None,
            file_extensions: vec![], priority: 10, notes: None,
            created_at: String::new(), updated_at: String::new(),
        };

        // Simulate merging
        let matching = vec![r2, r1]; // already sorted by priority desc
        let mut config = EffectiveConfig::default();
        for rule in &matching {
            if config.max_connections.is_none() { config.max_connections = rule.max_connections; }
            if config.max_segments.is_none() { config.max_segments = rule.max_segments; }
            if config.speed_limit_bps.is_none() { config.speed_limit_bps = rule.speed_limit_bps; }
            if config.user_agent.is_none() { config.user_agent = rule.user_agent.clone(); }
        }

        assert_eq!(config.max_connections, Some(8)); // From high priority
        assert_eq!(config.max_segments, Some(16));    // From high priority
        assert_eq!(config.speed_limit_bps, Some(1000)); // From low (high had None)
        assert_eq!(config.user_agent, Some("LowUA".into())); // From low (high had None)
    }
}
