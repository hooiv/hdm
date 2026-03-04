use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize)]
pub struct DockerLayer {
    pub digest: String,
    pub size: u64,
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct DockerImageInfo {
    pub name: String,
    pub tag: String,
    pub layers: Vec<DockerLayer>,
}

#[derive(Deserialize)]
struct DockerTokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct DockerManifestLayer {
    digest: String,
    size: u64,
}

#[derive(Deserialize)]
struct DockerManifest {
    layers: Option<Vec<DockerManifestLayer>>,
    // Older V2 Schema 1 manifests have 'fsLayers' and different format. We focus on modern V2.
}

/// Manifest list entry (for multi-platform images)
#[derive(Deserialize)]
struct DockerManifestListEntry {
    digest: String,
    #[serde(rename = "mediaType")]
    #[allow(dead_code)]
    media_type: Option<String>,
    platform: Option<DockerPlatform>,
}

#[derive(Deserialize)]
struct DockerPlatform {
    architecture: Option<String>,
    os: Option<String>,
}

/// Manifest list (fat manifest) wrapping multiple platform-specific manifests
#[derive(Deserialize)]
struct DockerManifestList {
    manifests: Option<Vec<DockerManifestListEntry>>,
}

#[tauri::command]
pub async fn fetch_docker_manifest(image: String) -> Result<DockerImageInfo, String> {
    // Parse image and tag. Handle registry:port/name:tag format correctly.
    // The tag is always after the LAST colon, but only if that colon isn't part of a port
    // (i.e., not followed by digits then '/').
    let (mut name, tag) = if let Some(at_idx) = image.rfind(':') {
        let after = &image[at_idx + 1..];
        // If everything after the last ':' contains '/', it's likely a port:path, not a tag
        if after.contains('/') {
            (image.clone(), "latest".to_string())
        } else {
            (image[..at_idx].to_string(), after.to_string())
        }
    } else {
        (image.clone(), "latest".to_string())
    };

    // Detect custom registry: if the first segment contains a dot or colon, treat it as a registry host
    let (_registry_host, auth_url_base, registry_api) = if name.contains('.') || name.split('/').next().map_or(false, |first| first.contains(':')) {
        // Custom registry: extract host from name
        let parts: Vec<&str> = name.splitn(2, '/').collect();
        if parts.len() < 2 {
            return Err(format!("Invalid image format for custom registry: {}", name));
        }
        let host = parts[0].to_string();
        name = parts[1].to_string();
        let api = format!("https://{}", host);
        // Custom registries typically don't use Docker Hub's auth; try without auth first
        (host.clone(), String::new(), api)
    } else {
        // Docker Hub
        if !name.contains('/') {
            name = format!("library/{}", name);
        }
        (
            "registry-1.docker.io".to_string(),
            "https://auth.docker.io/token?service=registry.docker.io".to_string(),
            "https://registry-1.docker.io".to_string(),
        )
    };

    // Validate name and tag contain only safe characters (alphanumeric, -, _, ., /)
    let valid_docker_name = |s: &str| -> bool {
        s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    };
    if !valid_docker_name(&name) {
        return Err(format!("Invalid image name: {}", name));
    }
    if !valid_docker_name(&tag) {
        return Err(format!("Invalid image tag: {}", tag));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    // 1. Get Auth Token (Docker Hub uses token auth; custom registries may not)
    let token = if !auth_url_base.is_empty() {
        let auth_url = format!("{}&scope=repository:{}:pull", auth_url_base, name);
        let auth_resp = client.get(&auth_url)
            .send()
            .await
            .map_err(|e| format!("Auth request failed: {}", e))?;

        if !auth_resp.status().is_success() {
            return Err(format!("Auth failed with status: {}", auth_resp.status()));
        }

        let auth_data: DockerTokenResponse = auth_resp.json().await
            .map_err(|e| format!("Failed to parse token: {}", e))?;
        Some(auth_data.token)
    } else {
        None // Custom registry — try without auth
    };

    // 2. Get Manifest
    let manifest_url = format!("{}/v2/{}/manifests/{}", registry_api, name, tag);
    let mut req = client.get(&manifest_url)
        // Accept manifest list, OCI and Docker V2 manifest formats
        .header("Accept", "application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json");
    if let Some(ref t) = token {
        req = req.bearer_auth(t);
    }
    let manifest_resp = req.send()
        .await
        .map_err(|e| format!("Manifest request failed: {}", e))?;

    if !manifest_resp.status().is_success() {
        return Err(format!("Manifest pull failed: {}", manifest_resp.status()));
    }

    let manifest_str = manifest_resp.text().await.map_err(|e| format!("Failed to read manifest text: {}", e))?;

    // Check if this is a manifest list (fat manifest) — if so, pick the best platform
    let manifest_str = if manifest_str.contains("\"manifests\"") {
        let manifest_list: DockerManifestList = serde_json::from_str(&manifest_str)
            .map_err(|e| format!("Failed to parse manifest list: {}", e))?;
        let manifests = manifest_list.manifests
            .ok_or_else(|| "Empty manifest list".to_string())?;

        // Prefer current platform (amd64/linux), fall back to first manifest
        let target_arch = if cfg!(target_arch = "x86_64") { "amd64" }
            else if cfg!(target_arch = "aarch64") { "arm64" }
            else { "amd64" };
        let chosen = manifests.iter()
            .find(|m| {
                m.platform.as_ref().map_or(false, |p| {
                    p.architecture.as_deref() == Some(target_arch) &&
                    p.os.as_deref() == Some("linux")
                })
            })
            .or_else(|| manifests.first())
            .ok_or_else(|| "No suitable platform found in manifest list".to_string())?;

        // Fetch the platform-specific manifest using the digest
        let platform_url = format!("{}/v2/{}/manifests/{}", registry_api, name, chosen.digest);
        let mut req = client.get(&platform_url)
            .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json");
        if let Some(ref t) = token {
            req = req.bearer_auth(t);
        }
        let platform_resp = req.send().await
            .map_err(|e| format!("Platform manifest request failed: {}", e))?;
        if !platform_resp.status().is_success() {
            return Err(format!("Platform manifest pull failed: {}", platform_resp.status()));
        }
        platform_resp.text().await
            .map_err(|e| format!("Failed to read platform manifest: {}", e))?
    } else {
        manifest_str
    };

    let manifest: DockerManifest = serde_json::from_str(&manifest_str)
        .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

    let layers = manifest.layers.ok_or_else(|| "No layers found in manifest. Older V2 Schema 1 is currently unsupported.".to_string())?;

    // 3. Build response
    let mut layer_infos = Vec::new();
    
    let mut auth_headers = HashMap::new();
    if let Some(ref t) = token {
        auth_headers.insert("Authorization".to_string(), format!("Bearer {}", t));
    }

    for l in layers {
        let digest_clean = l.digest.replace("sha256:", "");
        let layer_url = format!("{}/v2/{}/blobs/{}", registry_api, name, l.digest);
        
        // Note: auth headers are applied server-side during download,
        // not exposed to the frontend to prevent token leakage
        layer_infos.push(DockerLayer {
            digest: digest_clean,
            size: l.size,
            url: layer_url,
            headers: HashMap::new(),
        });
    }

    Ok(DockerImageInfo {
        name,
        tag,
        layers: layer_infos,
    })
}
