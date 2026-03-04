use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;

#[derive(Serialize)]
struct LfsBatchRequest<'a> {
    operation: &'a str,
    transfers: Vec<&'a str>,
    objects: Vec<LfsObjectReq<'a>>,
}

#[derive(Serialize)]
struct LfsObjectReq<'a> {
    oid: &'a str,
    size: u64,
}

#[derive(Deserialize, Debug)]
struct LfsBatchResponse {
    objects: Option<Vec<LfsObjectRes>>,
}

#[derive(Deserialize, Debug)]
struct LfsObjectRes {
    oid: String,
    actions: Option<LfsActions>,
}

#[derive(Deserialize, Debug)]
struct LfsActions {
    download: Option<LfsActionLink>,
}

#[derive(Deserialize, Debug)]
struct LfsActionLink {
    href: String,
    #[serde(default)]
    #[allow(dead_code)]
    header: std::collections::HashMap<String, String>,
}

pub async fn extract_lfs_pointer_info(text: &str) -> Option<(String, u64)> {
    if !text.starts_with("version https://git-lfs.github.com/spec/v1") {
        return None;
    }

    let mut oid = String::new();
    let mut size = 0u64;

    for line in text.lines() {
        if line.starts_with("oid sha256:") {
            oid = line.replace("oid sha256:", "").trim().to_string();
        } else if line.starts_with("size ") {
            if let Ok(s) = line.replace("size ", "").trim().parse::<u64>() {
                size = s;
            }
        }
    }

    if !oid.is_empty() && size > 0 {
        Some((oid, size))
    } else {
        None
    }
}

pub async fn resolve_lfs_pointer(original_url: &str, text: &str) -> Option<String> {
    let (oid, size) = extract_lfs_pointer_info(text).await?;

    // Guess the Git LFS batch API URL based on common Git host formats
    // e.g., https://raw.githubusercontent.com/owner/repo/main/file.bin
    //    -> https://github.com/owner/repo.git/info/lfs/objects/batch
    
    let mut lfs_api_url = original_url.to_string();
    
    if original_url.contains("raw.githubusercontent.com") {
        let parts: Vec<&str> = original_url.split('/').collect();
        // https://raw.githubusercontent.com / owner / repo / branch / path...
        // 0: https:, 1: "", 2: raw.githubusercontent.com, 3: owner, 4: repo
        if parts.len() >= 5 {
            let owner = parts[3];
            let repo = parts[4];
            lfs_api_url = format!("https://github.com/{}/{}.git/info/lfs/objects/batch", owner, repo);
        }
    } else if original_url.contains("gitlab.com") && original_url.contains("/-/raw/") {
        let parts: Vec<&str> = original_url.split("/-/raw/").collect();
        if parts.len() == 2 {
            let base_repo = parts[0];
            lfs_api_url = format!("{}.git/info/lfs/objects/batch", base_repo);
        }
    } else {
        // Can't guess the LFS API endpoint
        return None;
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let req_body = LfsBatchRequest {
        operation: "download",
        transfers: vec!["basic"],
        objects: vec![LfsObjectReq {
            oid: &oid,
            size,
        }],
    };

    let res = client.post(&lfs_api_url)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&req_body)
        .send()
        .await
        .ok()?;

    if res.status().is_success() {
        let batch_res: LfsBatchResponse = res.json().await.ok()?;
        if let Some(objects) = batch_res.objects {
            for obj in objects {
                if obj.oid == oid {
                    if let Some(actions) = obj.actions {
                        if let Some(download) = actions.download {
                            // Validate the href to prevent SSRF via malicious LFS responses
                            if let Ok(parsed) = url::Url::parse(&download.href) {
                                if parsed.scheme() == "http" || parsed.scheme() == "https" {
                                    // Block private/loopback IPs in download URLs
                                    if let Some(host) = parsed.host_str() {
                                        let lower = host.to_lowercase();
                                        if lower == "localhost" || lower == "[::1]" {
                                            return None;
                                        }
                                        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                                            match ip {
                                                std::net::IpAddr::V4(v4) => {
                                                    if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() {
                                                        return None;
                                                    }
                                                }
                                                std::net::IpAddr::V6(v6) => {
                                                    if v6.is_loopback() || v6.is_unspecified() {
                                                        return None;
                                                    }
                                                    if let Some(v4) = v6.to_ipv4_mapped() {
                                                        if v4.is_loopback() || v4.is_private() || v4.is_link_local() {
                                                            return None;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    return Some(download.href);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}
