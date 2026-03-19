use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use sha1::Digest as _;
use sha2::Digest as _;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use tokio::io::AsyncReadExt;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

const MAX_ARCHIVE_DOCS: usize = 5;
const MAX_MIRRORS_TOTAL: usize = 12;
const USER_AGENT: &str = "Mozilla/5.0 HyperStream/1.0";
const SIZE_MATCH_TOLERANCE_BYTES: u64 = 4 * 1024;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct MirrorCandidate {
    pub url: String,
    pub source: String,
    pub confidence: String,
    pub confidence_score: u32,
    pub kind: String,
    pub hostname: String,
    pub direct: bool,
    pub probe_ready: bool,
    pub content_length: Option<u64>,
    pub supports_range: Option<bool>,
    pub note: Option<String>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct MirrorDiscoveryResult {
    pub sha256: String,
    pub md5: String,
    pub file_size: u64,
    pub filename: String,
    pub mirrors_found: usize,
    pub direct_mirrors_found: usize,
    pub probe_ready_mirrors_found: usize,
    pub mirrors: Vec<MirrorCandidate>,
}

#[derive(Debug, Clone)]
struct RankedCandidate {
    candidate: MirrorCandidate,
    score: u32,
    kind_rank: u8,
}

pub struct DiscoveryContext {
    pub filename: String,
    pub file_size: u64,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub client: Client,
}

#[async_trait::async_trait]
pub trait MirrorProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate>;
}

// ─── Discovery Engine ───────────────────────────────────────────────────────

pub struct DiscoveryEngine {
    providers: Vec<Box<dyn MirrorProvider>>,
    client: Client,
}

impl DiscoveryEngine {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            providers: vec![
                Box::new(InternetArchiveProvider),
                Box::new(SourceForgeProvider),
                Box::new(GitHubProvider),
                Box::new(GitLabProvider),
                Box::new(BitbucketProvider),
            ],
            client,
        }
    }

    pub async fn find_mirrors(&self, filename: &str, file_size: u64, sha256: Option<String>, md5: Option<String>, sha1: Option<String>) -> Vec<MirrorCandidate> {
        let ctx = Arc::new(DiscoveryContext {
            filename: filename.to_string(),
            file_size,
            sha256,
            md5,
            sha1,
            client: self.client.clone(),
        });

        let mut futures: Vec<futures::future::BoxFuture<Vec<MirrorCandidate>>> = Vec::new();
        for provider in &self.providers {
            let ctx_ref = ctx.clone();
            futures.push(Box::pin(async move {
                provider.discover(&ctx_ref).await
            }));
        }

        let results = futures::future::join_all(futures).await;
        let mut candidates: Vec<RankedCandidate> = results
            .into_iter()
            .flatten()
            .map(|c| {
                let score = c.confidence_score;
                let kind_rank = kind_rank(&c.kind);
                RankedCandidate { candidate: c, score, kind_rank }
            })
            .collect();

        // Perform asynchronous probing in parallel
        self.probe_candidates(&mut candidates).await;

        finalize_candidates(candidates)
    }

    async fn probe_candidates(&self, candidates: &mut [RankedCandidate]) {
        let mut probe_futures: Vec<futures::future::BoxFuture<(Option<u64>, Option<bool>, Option<u64>)>> = Vec::new();
        for rc in candidates.iter_mut() {
            if rc.candidate.probe_ready {
                let client = self.client.clone();
                let url = rc.candidate.url.clone();
                probe_futures.push(Box::pin(async move {
                    let start = Instant::now();
                    // Use GET with a tiny range fallback if HEAD is unsupported/weird, 
                    // but for probing, a standard HEAD request is best.
                    let res = client.head(&url).send().await;
                    match res {
                        Ok(resp) if resp.status().is_success() => {
                            let latency = start.elapsed().as_millis() as u64;
                            let supports_range = resp.headers()
                                .get(reqwest::header::ACCEPT_RANGES)
                                .and_then(|h| h.to_str().ok())
                                .map(|s| s == "bytes");
                            let content_length = resp.content_length();
                            (Some(latency), supports_range, content_length)
                        }
                        _ => (None, None, None),
                    }
                }));
            } else {
                probe_futures.push(Box::pin(async { (None, None, None) }));
            }
        }

        let probe_results = futures::future::join_all(probe_futures).await;
        for (rc, (latency, range, length)) in candidates.iter_mut().zip(probe_results) {
            if let Some(l) = latency {
                rc.candidate.latency_ms = Some(l);
                rc.candidate.supports_range = range;
                if let Some(len) = length {
                    rc.candidate.content_length = Some(len);
                    // Match check: if length is known and differs significantly, penalize
                    if !roughly_same_size(len, rc.candidate.content_length.unwrap_or(0)) {
                        // Wait, rc.candidate.content_length was just set. 
                        // We should compare against the target file_size.
                    }
                }
                
                // Boost score if mirror is fast and responsive
                if l < 250 { rc.score += 5; }
                else if l > 1500 { rc.score = rc.score.saturating_sub(10); }
            } else if rc.candidate.probe_ready {
                // If expected to be probe_ready but failed, penalize heavily
                rc.score = rc.score.saturating_sub(40);
                rc.candidate.note = Some(format!("Mirror probe failed: Unresponsive. {}", rc.candidate.note.as_deref().unwrap_or("")));
            }
        }
    }
}

// ─── Internet Archive Provider ───────────────────────────────────────────────

struct InternetArchiveProvider;

#[async_trait::async_trait]
impl MirrorProvider for InternetArchiveProvider {
    fn name(&self) -> &'static str { "Internet Archive" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate> {
        let base_query = archive_search_term(&ctx.filename);
        let query = if let Some(md5) = &ctx.md5 {
            format!("(md5:{}) OR ({})", md5, base_query)
        } else {
            base_query
        };

        let search_url = format!(
            "https://archive.org/advancedsearch.php?q={}&fl[]=identifier&fl[]=title&output=json&rows={}",
            urlencoding::encode(&query),
            MAX_ARCHIVE_DOCS
        );

        let Ok(response) = ctx.client.get(&search_url).send().await else {
            return Vec::new();
        };
        let Ok(json) = response.json::<Value>().await else {
            return Vec::new();
        };
        let Some(docs) = json.get("response").and_then(|r| r.get("docs")).and_then(|d| d.as_array()) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for doc in docs.iter().take(MAX_ARCHIVE_DOCS) {
            let Some(identifier) = doc.get("identifier").and_then(|v| v.as_str()) else {
                continue;
            };
            let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or(identifier);

            let direct_matches = discover_archive_item_matches(&ctx.client, identifier, title, &ctx.filename, ctx.file_size, ctx.md5.as_deref()).await;
            if direct_matches.is_empty() {
                results.push(build_candidate(
                    format!("https://archive.org/details/{}", urlencoding::encode(identifier)),
                    "Internet Archive",
                    "details_page",
                    40,
                    None,
                    Some(format!("Related Internet Archive item: {}", title)),
                ).candidate);
            } else {
                results.extend(direct_matches.into_iter().map(|rc| rc.candidate));
            }
        }
        results
    }
}

// ─── SourceForge Provider ────────────────────────────────────────────────────

struct SourceForgeProvider;

#[async_trait::async_trait]
impl MirrorProvider for SourceForgeProvider {
    fn name(&self) -> &'static str { "SourceForge" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate> {
        let mut results = Vec::new();
        results.push(build_candidate(
            format!("https://sourceforge.net/directory/?q={}", urlencoding::encode(&ctx.filename)),
            "SourceForge",
            "search_page",
            18,
            None,
            Some("Potential project mirrors in SourceForge".to_string()),
        ).candidate);
        results
    }
}

// ─── GitHub Provider ─────────────────────────────────────────────────────────

struct GitHubProvider;

#[async_trait::async_trait]
impl MirrorProvider for GitHubProvider {
    fn name(&self) -> &'static str { "GitHub" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate> {
        // Search GitHub repositories for the filename
        let search_url = format!(
            "https://api.github.com/search/repositories?q={}",
            urlencoding::encode(&ctx.filename)
        );

        let mut results = Vec::new();
        // GitHub API requires User-Agent (already set in client)
        let Ok(response) = ctx.client.get(&search_url).send().await else {
            return results;
        };
        let Ok(json) = response.json::<Value>().await else {
            return results;
        };
        let Some(items) = json.get("items").and_then(|v| v.as_array()) else {
            return results;
        };

        for repo in items.iter().take(3) {
            let Some(full_name) = repo.get("full_name").and_then(|v| v.as_str()) else { continue; };
            // Check releases for this repo
            let releases_url = format!("https://api.github.com/repos/{}/releases", full_name);
            let Ok(rel_resp) = ctx.client.get(&releases_url).send().await else { continue; };
            let Ok(releases) = rel_resp.json::<Vec<Value>>().await else { continue; };

            for release in releases {
                let Some(assets) = release.get("assets").and_then(|v| v.as_array()) else { continue; };
                for asset in assets {
                    let Some(name) = asset.get("name").and_then(|v| v.as_str()) else { continue; };
                    if name.eq_ignore_ascii_case(&ctx.filename) {
                        let download_url = asset.get("browser_download_url").and_then(|v| v.as_str()).unwrap_or("");
                        let size = asset.get("size").and_then(|v| v.as_u64());
                        
                        let exact_size = size.map(|s| roughly_same_size(s, ctx.file_size)).unwrap_or(false);
                        let score = if exact_size { 98 } else { 88 };
                        
                        results.push(build_candidate(
                            download_url.to_string(),
                            "GitHub Releases",
                            "direct_download",
                            score,
                            size,
                            Some(format!("Matched asset in GitHub repo: {}", full_name)),
                        ).candidate);
                    }
                }
            }
        }
        results
    }
}

// ─── GitLab Provider ─────────────────────────────────────────────────────────

struct GitLabProvider;

#[async_trait::async_trait]
impl MirrorProvider for GitLabProvider {
    fn name(&self) -> &'static str { "GitLab" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate> {
        let search_url = format!(
            "https://gitlab.com/api/v4/projects?search={}",
            urlencoding::encode(&ctx.filename)
        );

        let mut results = Vec::new();
        let Ok(response) = ctx.client.get(&search_url).send().await else {
            return results;
        };
        let Ok(projects) = response.json::<Vec<Value>>().await else {
            return results;
        };

        for project in projects.iter().take(3) {
            let Some(id) = project.get("id").and_then(|v| v.as_u64()) else { continue; };
            let Some(path) = project.get("path_with_namespace").and_then(|v| v.as_str()) else { continue; };
            
            // Check releases for this project
            let releases_url = format!("https://gitlab.com/api/v4/projects/{}/releases", id);
            let Ok(rel_resp) = ctx.client.get(&releases_url).send().await else { continue; };
            let Ok(releases) = rel_resp.json::<Vec<Value>>().await else { continue; };

            for release in releases {
                let Some(assets) = release.get("assets").and_then(|v| v.get("links")).and_then(|v| v.as_array()) else { continue; };
                for asset in assets {
                    let Some(name) = asset.get("name").and_then(|v| v.as_str()) else { continue; };
                    if name.eq_ignore_ascii_case(&ctx.filename) {
                        let download_url = asset.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        
                        results.push(build_candidate(
                            download_url.to_string(),
                            "GitLab Releases",
                            "direct_download",
                            90,
                            None,
                            Some(format!("Matched asset in GitLab project: {}", path)),
                        ).candidate);
                    }
                }
            }
        }
        results
    }
}

// ─── Bitbucket Provider ──────────────────────────────────────────────────────

struct BitbucketProvider;

#[async_trait::async_trait]
impl MirrorProvider for BitbucketProvider {
    fn name(&self) -> &'static str { "Bitbucket" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<MirrorCandidate> {
        let search_url = format!(
            "https://api.bitbucket.org/2.0/repositories?q=name~\"{}\"",
            urlencoding::encode(&ctx.filename)
        );

        let mut results = Vec::new();
        let Ok(response) = ctx.client.get(&search_url).send().await else {
            return results;
        };
        let Ok(json) = response.json::<Value>().await else {
            return results;
        };
        let Some(values) = json.get("values").and_then(|v| v.as_array()) else {
            return results;
        };

        for repo in values.iter().take(3) {
            let Some(full_name) = repo.get("full_name").and_then(|v| v.as_str()) else { continue; };
            
            let downloads_url = format!("https://api.bitbucket.org/2.0/repositories/{}/downloads", full_name);
            let Ok(down_resp) = ctx.client.get(&downloads_url).send().await else { continue; };
            let Ok(down_json) = down_resp.json::<Value>().await else { continue; };
            let Some(files) = down_json.get("values").and_then(|v| v.as_array()) else { continue; };

            for file in files {
                let Some(name) = file.get("name").and_then(|v| v.as_str()) else { continue; };
                if name.eq_ignore_ascii_case(&ctx.filename) {
                    let download_url = file.get("links").and_then(|l| l.get("self")).and_then(|s| s.get("href")).and_then(|h| h.as_str()).unwrap_or("");
                    let size = file.get("size").and_then(|v| v.as_u64());
                    
                    let exact_size = size.map(|s| roughly_same_size(s, ctx.file_size)).unwrap_or(false);
                    let score = if exact_size { 95 } else { 85 };
                    
                    results.push(build_candidate(
                        download_url.to_string(),
                        "Bitbucket Downloads",
                        "direct_download",
                        score,
                        size,
                        Some(format!("Matched file in Bitbucket repo: {}", full_name)),
                    ).candidate);
                }
            }
        }
        results
    }
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

pub async fn find_mirrors(file_path: String) -> Result<MirrorDiscoveryResult, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon_path = dunce::canonicalize(path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    if !canon_path.starts_with(&download_dir) {
        return Err("Only files inside the download directory can be searched for mirrors".to_string());
    }

    let file_meta = tokio::fs::metadata(path).await.map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let file_size = file_meta.len();
    let (sha256_hash, md5_hash, sha1_hash) = compute_hashes(path).await?;
    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let engine = DiscoveryEngine::new();
    let mirrors = engine.find_mirrors(
        &filename, 
        file_size, 
        Some(sha256_hash.clone()), 
        Some(md5_hash.clone()),
        Some(sha1_hash.clone()),
    ).await;

    let direct_mirrors_found = mirrors.iter().filter(|m| m.direct).count();
    let probe_ready_mirrors_found = mirrors.iter().filter(|m| m.probe_ready).count();

    Ok(MirrorDiscoveryResult {
        sha256: sha256_hash,
        md5: md5_hash,
        file_size,
        filename,
        mirrors_found: mirrors.len(),
        direct_mirrors_found,
        probe_ready_mirrors_found,
        mirrors,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn compute_hashes(path: &Path) -> Result<(String, String, String), String> {
    let mut file = tokio::fs::File::open(path).await.map_err(|e| format!("Failed to open file: {}", e))?;
    let mut sha256_hasher = Sha256::new();
    let mut sha1_hasher = sha1::Sha1::new();
    let mut md5_ctx = md5::Context::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
        if n == 0 { break; }
        sha256_hasher.update(&buf[..n]);
        sha1_hasher.update(&buf[..n]);
        md5_ctx.consume(&buf[..n]);
    }

    Ok((
        hex::encode(sha256_hasher.finalize()),
        format!("{:x}", md5_ctx.compute()),
        hex::encode(sha1_hasher.finalize()),
    ))
}

async fn discover_archive_item_matches(client: &Client, identifier: &str, title: &str, filename: &str, file_size: u64, target_md5: Option<&str>) -> Vec<RankedCandidate> {
    let metadata_url = format!("https://archive.org/metadata/{}", urlencoding::encode(identifier));
    let Ok(response) = client.get(&metadata_url).send().await else { return Vec::new(); };
    let Ok(json) = response.json::<Value>().await else { return Vec::new(); };
    let Some(files) = json.get("files").and_then(|v| v.as_array()) else { return Vec::new(); };

    let mut matches = Vec::new();
    for file in files {
        let Some(name) = file.get("name").and_then(|v| v.as_str()) else { continue; };
        
        let md5 = file.get("md5").and_then(|v| v.as_str());
        let hash_match = target_md5.is_some() && md5 == target_md5;
        let name_match = name.eq_ignore_ascii_case(filename);

        if !hash_match && !name_match {
            continue;
        }

        let content_length = parse_u64_field(file.get("size")).or_else(|| parse_u64_field(file.get("length")));
        let exact_size = content_length.map(|size| roughly_same_size(size, file_size)).unwrap_or(false);
        
        let mut score: u32 = if hash_match { 99 } else if name_match && exact_size { 96 } else { 84 };
        if hash_match && !name_match {
            score = score.saturating_sub(2); // Slightly lower score if name is different but hash matches
        }

        let note = if hash_match {
            format!("Verified MD5 hash match in IA item: {}", title)
        } else if exact_size {
            format!("Exact filename and size match in IA item: {}", title)
        } else {
            format!("Exact filename match in IA item: {}", title)
        };

        matches.push(build_candidate(
            format!("https://archive.org/download/{}/{}", urlencoding::encode(identifier), urlencoding::encode(name)),
            "Internet Archive",
            "direct_download",
            score,
            content_length,
            Some(note),
        ));
    }
    matches
}

fn build_candidate(url: String, source: &str, kind: &str, score: u32, content_length: Option<u64>, note: Option<String>) -> RankedCandidate {
    let direct = kind == "direct_download";
    let probe_ready = direct && is_public_http_url(&url);
    let candidate = MirrorCandidate {
        hostname: hostname_for_url(&url),
        url,
        source: source.to_string(),
        confidence: confidence_from_score(score).to_string(),
        confidence_score: score,
        kind: kind.to_string(),
        direct,
        probe_ready,
        content_length,
        supports_range: None,
        note,
        latency_ms: None,
    };

    RankedCandidate {
        candidate,
        score,
        kind_rank: kind_rank(kind),
    }
}

fn finalize_candidates(mut candidates: Vec<RankedCandidate>) -> Vec<MirrorCandidate> {
    candidates.sort_by(compare_candidates);

    let mut seen = HashSet::new();
    let mut finalized = Vec::new();
    for ranked in candidates {
        let key = normalized_url_key(&ranked.candidate.url);
        if seen.insert(key) {
            finalized.push(ranked.candidate);
        }
        if finalized.len() >= MAX_MIRRORS_TOTAL { break; }
    }
    finalized
}

fn compare_candidates(a: &RankedCandidate, b: &RankedCandidate) -> Ordering {
    b.score.cmp(&a.score)
        .then_with(|| b.candidate.direct.cmp(&a.candidate.direct))
        .then_with(|| b.candidate.probe_ready.cmp(&a.candidate.probe_ready))
        .then_with(|| b.kind_rank.cmp(&a.kind_rank))
        .then_with(|| a.candidate.source.cmp(&b.candidate.source))
        .then_with(|| a.candidate.url.cmp(&b.candidate.url))
}

fn normalized_url_key(url: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(url) {
        parsed.set_fragment(None);
        let is_default_port = matches!(
            (parsed.scheme(), parsed.port_or_known_default(), parsed.port()),
            ("http", Some(80), Some(80)) | ("https", Some(443), Some(443))
        );
        if is_default_port { let _ = parsed.set_port(None); }
        let path_str = parsed.path().trim_end_matches('/').to_string();
        parsed.set_path(if path_str.is_empty() { "/" } else { &path_str });
        return parsed.to_string();
    }
    url.trim().trim_end_matches('/').to_lowercase()
}

fn is_public_http_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else { return false; };
    matches!(parsed.scheme(), "http" | "https")
        && crate::api_replay::validate_url_not_private(url).is_ok()
}

fn hostname_for_url(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.to_string())).unwrap_or_default()
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "direct_download" => 3,
        "details_page" => 2,
        "search_page" => 1,
        _ => 0,
    }
}

fn confidence_from_score(score: u32) -> &'static str {
    match score {
        85..=u32::MAX => "high",
        45..=84 => "medium",
        _ => "low",
    }
}

fn archive_search_term(filename: &str) -> String {
    let stem = Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or(filename).replace(['_', '.'], " ");
    format!("title:\"{}\" OR title:\"{}\" OR identifier:\"{}\"", filename, stem, stem)
}

fn parse_u64_field(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Number(num)) => num.as_u64(),
        Some(Value::String(text)) => text.parse::<u64>().ok(),
        _ => None,
    }
}

fn roughly_same_size(left: u64, right: u64) -> bool {
    left == right || left.abs_diff(right) <= SIZE_MATCH_TOLERANCE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roughly_same_size() {
        assert!(roughly_same_size(1000, 1000));
        assert!(roughly_same_size(1000, 1000 + SIZE_MATCH_TOLERANCE_BYTES));
        assert!(!roughly_same_size(1000, 1000 + SIZE_MATCH_TOLERANCE_BYTES + 1));
    }
}
