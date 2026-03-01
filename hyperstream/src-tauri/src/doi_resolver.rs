use reqwest::header::{ACCEPT, USER_AGENT};

#[tauri::command]
pub async fn resolve_doi(doi: String) -> Result<String, String> {
    // Basic validation to strip common prefixes if user pasted full URL
    let stripped_doi = doi
        .replace("https://doi.org/", "")
        .replace("http://doi.org/", "")
        .replace("doi.org/", "");

    let url = format!("https://doi.org/{}", stripped_doi);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(ACCEPT, "application/x-bibtex")
        .header(USER_AGENT, "HyperStream/1.0 (Downloader; aditya)")
        .send()
        .await
        .map_err(|e| format!("Failed to request DOI: {}", e))?;

    if response.status().is_success() {
        let bibtex = response
            .text()
            .await
            .map_err(|e| format!("Failed to read BibTeX response: {}", e))?;
        Ok(bibtex)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        Err("DOI not found".to_string())
    } else {
        Err(format!("Failed with status: {}", response.status()))
    }
}
