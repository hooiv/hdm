use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use tokio::io::AsyncReadExt;

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

/// Compute hashes and search for alternative download mirrors.
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

    let file_meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let file_size = file_meta.len();
    let (sha256_hash, md5_hash) = compute_hashes(path).await?;
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let ranked = discover_candidates(&client, &filename, file_size).await;
    let mirrors = finalize_candidates(ranked);
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

async fn compute_hashes(path: &Path) -> Result<(String, String), String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let mut sha256_hasher = Sha256::new();
    let mut md5_ctx = md5::Context::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read error: {}", e))?;
        if n == 0 {
            break;
        }
        sha256_hasher.update(&buf[..n]);
        md5_ctx.consume(&buf[..n]);
    }

    Ok((
        hex::encode(sha256_hasher.finalize()),
        format!("{:x}", md5_ctx.compute()),
    ))
}

async fn discover_candidates(client: &Client, filename: &str, file_size: u64) -> Vec<RankedCandidate> {
    let mut candidates = discover_archive_org_candidates(client, filename, file_size).await;

    candidates.push(build_candidate(
        format!(
            "https://archive.org/search?query={}",
            urlencoding::encode(&archive_search_term(filename))
        ),
        "Internet Archive Search",
        "search_page",
        24,
        None,
        Some("Manual search page for related archive items".to_string()),
    ));

    candidates.push(build_candidate(
        format!(
            "https://sourceforge.net/directory/?q={}",
            urlencoding::encode(filename)
        ),
        "SourceForge Search",
        "search_page",
        18,
        None,
        Some("Manual search page for related project mirrors".to_string()),
    ));

    candidates
}

async fn discover_archive_org_candidates(
    client: &Client,
    filename: &str,
    file_size: u64,
) -> Vec<RankedCandidate> {
    let query = archive_search_term(filename);
    let search_url = format!(
        "https://archive.org/advancedsearch.php?q={}&fl[]=identifier&fl[]=title&output=json&rows={}",
        urlencoding::encode(&query),
        MAX_ARCHIVE_DOCS
    );

    let mut results = Vec::new();
    let Ok(response) = client.get(&search_url).header("User-Agent", USER_AGENT).send().await else {
        return results;
    };
    let Ok(json) = response.json::<Value>().await else {
        return results;
    };
    let Some(docs) = json
        .get("response")
        .and_then(|r| r.get("docs"))
        .and_then(|d| d.as_array())
    else {
        return results;
    };

    for doc in docs.iter().take(MAX_ARCHIVE_DOCS) {
        let Some(identifier) = doc.get("identifier").and_then(|v| v.as_str()) else {
            continue;
        };
        let title = doc
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(identifier);

        let direct_matches = discover_archive_item_matches(client, identifier, title, filename, file_size).await;
        if direct_matches.is_empty() {
            results.push(build_candidate(
                format!("https://archive.org/details/{}", urlencoding::encode(identifier)),
                "Internet Archive",
                "details_page",
                40,
                None,
                Some(format!("Related Internet Archive item: {}", title)),
            ));
        } else {
            results.extend(direct_matches);
        }
    }

    results
}

async fn discover_archive_item_matches(
    client: &Client,
    identifier: &str,
    title: &str,
    filename: &str,
    file_size: u64,
) -> Vec<RankedCandidate> {
    let metadata_url = format!("https://archive.org/metadata/{}", urlencoding::encode(identifier));
    let Ok(response) = client
        .get(&metadata_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(json) = response.json::<Value>().await else {
        return Vec::new();
    };
    let Some(files) = json.get("files").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    for file in files {
        let Some(name) = file.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !name.eq_ignore_ascii_case(filename) {
            continue;
        }

        let content_length = parse_u64_field(file.get("size")).or_else(|| parse_u64_field(file.get("length")));
        let exact_size = content_length
            .map(|size| roughly_same_size(size, file_size))
            .unwrap_or(false);
        let score = if exact_size { 96 } else { 84 };
        let note = if exact_size {
            format!("Exact filename and size match in Internet Archive item: {}", title)
        } else {
            format!("Exact filename match in Internet Archive item: {}", title)
        };

        matches.push(build_candidate(
            format!(
                "https://archive.org/download/{}/{}",
                urlencoding::encode(identifier),
                urlencoding::encode(name)
            ),
            "Internet Archive",
            "direct_download",
            score,
            content_length,
            Some(note),
        ));
    }

    matches
}

fn build_candidate(
    url: String,
    source: &str,
    kind: &str,
    score: u32,
    content_length: Option<u64>,
    note: Option<String>,
) -> RankedCandidate {
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
        if finalized.len() >= MAX_MIRRORS_TOTAL {
            break;
        }
    }
    finalized
}

fn compare_candidates(a: &RankedCandidate, b: &RankedCandidate) -> Ordering {
    b.score
        .cmp(&a.score)
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
        if is_default_port {
            let _ = parsed.set_port(None);
        }
        let path_str = parsed.path().trim_end_matches('/').to_string();
        parsed.set_path(if path_str.is_empty() { "/" } else { &path_str });
        return parsed.to_string();
    }

    url.trim().trim_end_matches('/').to_lowercase()
}

fn is_public_http_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https")
        && crate::api_replay::validate_url_not_private(url).is_ok()
}

fn hostname_for_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default()
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
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename)
        .replace(['_', '.'], " ");

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
    fn finalize_candidates_prefers_stronger_duplicate() {
        let weak = build_candidate(
            "https://archive.org/download/example/file.iso".to_string(),
            "Internet Archive Search",
            "search_page",
            20,
            None,
            None,
        );
        let strong = build_candidate(
            "https://archive.org/download/example/file.iso".to_string(),
            "Internet Archive",
            "direct_download",
            96,
            Some(1024),
            Some("Exact filename and size match".to_string()),
        );

        let finalized = finalize_candidates(vec![weak, strong]);
        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].direct);
        assert!(finalized[0].probe_ready);
        assert_eq!(finalized[0].confidence, "high");
    }

    #[test]
    fn normalized_url_key_ignores_fragment_and_trailing_slash() {
        let a = normalized_url_key("https://archive.org/download/example/file.iso/");
        let b = normalized_url_key("https://archive.org/download/example/file.iso#fragment");
        assert_eq!(a, b);
    }

    #[test]
    fn only_public_direct_urls_are_probe_ready() {
        assert!(is_public_http_url("https://archive.org/download/example/file.iso"));
        assert!(!is_public_http_url("file:///tmp/file.iso"));
        assert!(!is_public_http_url("http://127.0.0.1/file.iso"));
    }
}
