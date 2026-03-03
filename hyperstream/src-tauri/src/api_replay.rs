use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone)]
pub struct ReplayResult {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body_preview: String,
    pub response_time_ms: u64,
    pub body_size: usize,
}

/// Replay an HTTP request with the given parameters.
pub async fn replay_request(
    url: String,
    method: String,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
) -> Result<ReplayResult, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let mut req = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "HEAD" => client.head(&url),
        "PATCH" => client.patch(&url),
        _ => return Err(format!("Unsupported method: {}", method)),
    };

    if let Some(hdrs) = headers {
        for (k, v) in &hdrs {
            req = req.header(k, v);
        }
    }

    if let Some(b) = body {
        req = req.body(b);
    }

    let start = Instant::now();
    let response = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    let elapsed = start.elapsed().as_millis() as u64;

    let status_code = response.status().as_u16();
    let mut resp_headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            resp_headers.insert(name.to_string(), v.to_string());
        }
    }

    let body_bytes = response.bytes().await.map_err(|e| format!("Body read error: {}", e))?;
    let body_size = body_bytes.len();
    let body_preview = String::from_utf8_lossy(&body_bytes[..body_size.min(2000)]).to_string();

    Ok(ReplayResult {
        status_code,
        headers: resp_headers,
        body_preview,
        response_time_ms: elapsed,
        body_size,
    })
}

#[derive(Serialize)]
pub struct FuzzResult {
    pub original_url: String,
    pub mutations: Vec<FuzzMutation>,
}

#[derive(Serialize)]
pub struct FuzzMutation {
    pub mutated_url: String,
    pub mutation_type: String,
    pub status_code: u16,
    pub response_time_ms: u64,
    pub body_size: usize,
    pub interesting: bool,
}

/// Fuzz a URL by mutating query parameters and path segments.
pub async fn fuzz_url(url: String) -> Result<FuzzResult, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let mut mutations: Vec<FuzzMutation> = Vec::new();

    // Get baseline
    let baseline_start = Instant::now();
    let baseline = client.get(&url).send().await.map_err(|e| format!("Baseline failed: {}", e))?;
    let _baseline_elapsed = baseline_start.elapsed().as_millis() as u64;
    let baseline_status = baseline.status().as_u16();
    let baseline_size = baseline.bytes().await.map_err(|e| e.to_string())?.len();

    // Mutation 1: Remove query parameters one by one
    let pairs: Vec<(String, String)> = parsed.query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    for i in 0..pairs.len() {
        let mut new_url = parsed.clone();
        {
            let mut query = new_url.query_pairs_mut();
            query.clear();
            for (j, (k, v)) in pairs.iter().enumerate() {
                if j != i {
                    query.append_pair(k, v);
                }
            }
        }
        let mutated = new_url.to_string();
        if let Ok(result) = probe_url(&client, &mutated).await {
            mutations.push(FuzzMutation {
                mutated_url: mutated,
                mutation_type: format!("Removed param: {}", pairs[i].0),
                status_code: result.0,
                response_time_ms: result.1,
                body_size: result.2,
                interesting: result.0 != baseline_status || (result.2 as i64 - baseline_size as i64).unsigned_abs() > 100,
            });
        }
    }

    // Mutation 2: Add common probe parameters
    let probe_params = vec![
        ("debug", "true"), ("admin", "1"), ("format", "json"),
        ("callback", "test"), ("_", "1"), ("verbose", "1"),
    ];
    for (key, value) in probe_params {
        let mut new_url = parsed.clone();
        new_url.query_pairs_mut().append_pair(key, value);
        let mutated = new_url.to_string();
        if let Ok(result) = probe_url(&client, &mutated).await {
            mutations.push(FuzzMutation {
                mutated_url: mutated,
                mutation_type: format!("Added param: {}={}", key, value),
                status_code: result.0,
                response_time_ms: result.1,
                body_size: result.2,
                interesting: result.0 != baseline_status || (result.2 as i64 - baseline_size as i64).unsigned_abs() > 100,
            });
        }
    }

    // Mutation 3: Path traversal probes
    let path = parsed.path().to_string();
    let traversal_suffixes = vec!["/../", "/./", "/%2e%2e/", "/..;/"];
    for suffix in traversal_suffixes {
        let mutated = format!("{}{}{}", parsed.scheme(), "://", 
            format!("{}{}{}", parsed.host_str().unwrap_or(""), &path, suffix));
        if let Ok(result) = probe_url(&client, &mutated).await {
            mutations.push(FuzzMutation {
                mutated_url: mutated,
                mutation_type: format!("Path traversal: {}", suffix),
                status_code: result.0,
                response_time_ms: result.1,
                body_size: result.2,
                interesting: result.0 == 200 || result.0 == 403,
            });
        }
    }

    Ok(FuzzResult {
        original_url: url,
        mutations,
    })
}

async fn probe_url(client: &Client, url: &str) -> Result<(u16, u64, usize), String> {
    let start = Instant::now();
    let res = client.get(url).send().await.map_err(|e| e.to_string())?;
    let elapsed = start.elapsed().as_millis() as u64;
    let status = res.status().as_u16();
    let size = res.bytes().await.map_err(|e| e.to_string())?.len();
    Ok((status, elapsed, size))
}
