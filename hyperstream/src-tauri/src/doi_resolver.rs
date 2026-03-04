use reqwest::header::{ACCEPT, USER_AGENT};

#[tauri::command]
pub async fn resolve_doi(doi: String) -> Result<String, String> {
    // Basic validation to strip common prefixes if user pasted full URL
    let stripped_doi = doi
        .replace("https://doi.org/", "")
        .replace("http://doi.org/", "")
        .replace("doi.org/", "");

    // URL-encode the DOI to handle special characters (e.g. parentheses, angle brackets)
    let encoded_doi: String = stripped_doi.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
        _ => format!("%{:02X}", b),
    }).collect();

    let url = format!("https://doi.org/{}", encoded_doi);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;
    let response = client
        .get(&url)
        .header(ACCEPT, "application/x-bibtex")
        .header(USER_AGENT, "HyperStream/1.0 (Downloader; aditya)")
        .send()
        .await
        .map_err(|e| format!("Failed to request DOI: {}", e))?;

    if response.status().is_success() {
        // Guard against oversized responses (max 1 MB for BibTeX)
        if let Some(cl) = response.content_length() {
            if cl > 1024 * 1024 {
                return Err(format!("DOI response too large: {} bytes (max 1 MB)", cl));
            }
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read BibTeX response: {}", e))?;
        if bytes.len() > 1024 * 1024 {
            return Err(format!("DOI response too large: {} bytes (max 1 MB)", bytes.len()));
        }
        let bibtex = String::from_utf8_lossy(&bytes).to_string();
        Ok(bibtex)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        Err("DOI not found".to_string())
    } else {
        Err(format!("Failed with status: {}", response.status()))
    }
}
