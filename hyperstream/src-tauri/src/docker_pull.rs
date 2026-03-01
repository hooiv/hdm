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

#[tauri::command]
pub async fn fetch_docker_manifest(image: String) -> Result<DockerImageInfo, String> {
    // Parse image and tag
    let parts: Vec<&str> = image.split(':').collect();
    let mut name = parts[0].to_string();
    let tag = if parts.len() > 1 { parts[1].to_string() } else { "latest".to_string() };

    // Standardize implicit library prefixes (e.g. "ubuntu" -> "library/ubuntu")
    if !name.contains('/') {
        name = format!("library/{}", name);
    }

    let client = reqwest::Client::new();

    // 1. Get Auth Token
    // We request anonymous pull access for the repository
    let auth_url = format!("https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull", name);
    let auth_resp = client.get(&auth_url)
        .send()
        .await
        .map_err(|e| format!("Auth request failed: {}", e))?;

    if !auth_resp.status().is_success() {
        return Err(format!("Auth failed with status: {}", auth_resp.status()));
    }

    let auth_data: DockerTokenResponse = auth_resp.json().await
        .map_err(|e| format!("Failed to parse token: {}", e))?;
        
    let token = auth_data.token;

    // 2. Get Manifest
    let manifest_url = format!("https://registry-1.docker.io/v2/{}/manifests/{}", name, tag);
    let manifest_resp = client.get(&manifest_url)
        .bearer_auth(&token)
        // Accept both OCI and Docker V2 manifest formats
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json")
        .send()
        .await
        .map_err(|e| format!("Manifest request failed: {}", e))?;

    if !manifest_resp.status().is_success() {
        return Err(format!("Manifest pull failed: {}", manifest_resp.status()));
    }

    let manifest_str = manifest_resp.text().await.map_err(|e| format!("Failed to read manifest text: {}", e))?;
    
    let manifest: DockerManifest = serde_json::from_str(&manifest_str)
        .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

    let layers = manifest.layers.ok_or_else(|| "No layers found in manifest. Older V2 Schema 1 is currently unsupported.".to_string())?;

    // 3. Build response
    let mut layer_infos = Vec::new();
    
    let mut auth_headers = HashMap::new();
    auth_headers.insert("Authorization".to_string(), format!("Bearer {}", token));

    for l in layers {
        let digest_clean = l.digest.replace("sha256:", "");
        let layer_url = format!("https://registry-1.docker.io/v2/{}/blobs/{}", name, l.digest);
        
        layer_infos.push(DockerLayer {
            digest: digest_clean,
            size: l.size,
            url: layer_url,
            headers: auth_headers.clone(),
        });
    }

    Ok(DockerImageInfo {
        name,
        tag,
        layers: layer_infos,
    })
}
