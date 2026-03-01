use sha2::{Sha256, Digest};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
pub struct DuplicateGroup {
    pub hash: String,
    pub file_size: u64,
    pub files: Vec<String>,
    pub wasted_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct OptimizeResult {
    pub total_files_scanned: usize,
    pub total_size: u64,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub total_duplicates: usize,
    pub total_wasted_bytes: u64,
    pub potential_savings_mb: f64,
}

/// Scan a list of directories/files for duplicates using SHA-256 hashing.
/// Groups identical files and reports wasted space.
pub async fn optimize_mods(paths: Vec<String>) -> Result<OptimizeResult, String> {
    let mut file_hashes: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    let mut total_files = 0usize;
    let mut total_size = 0u64;

    for path_str in &paths {
        let path = Path::new(path_str);
        if path.is_file() {
            if let Ok((hash, size)) = hash_file(path).await {
                file_hashes.entry(hash).or_default().push((path_str.clone(), size));
                total_files += 1;
                total_size += size;
            }
        } else if path.is_dir() {
            scan_directory_recursive(path, &mut file_hashes, &mut total_files, &mut total_size).await;
        }
    }

    let mut duplicate_groups = Vec::new();
    let mut total_duplicates = 0usize;
    let mut total_wasted = 0u64;

    for (hash, files) in &file_hashes {
        if files.len() > 1 {
            let file_size = files[0].1;
            let wasted = file_size * (files.len() as u64 - 1);
            total_duplicates += files.len() - 1;
            total_wasted += wasted;
            duplicate_groups.push(DuplicateGroup {
                hash: hash.clone(),
                file_size,
                files: files.iter().map(|(p, _)| p.clone()).collect(),
                wasted_bytes: wasted,
            });
        }
    }

    // Sort by wasted bytes descending
    duplicate_groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));

    Ok(OptimizeResult {
        total_files_scanned: total_files,
        total_size,
        duplicate_groups,
        total_duplicates,
        total_wasted_bytes: total_wasted,
        potential_savings_mb: total_wasted as f64 / (1024.0 * 1024.0),
    })
}

async fn hash_file(path: &Path) -> Result<(String, u64), String> {
    let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
    let size = bytes.len() as u64;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hex::encode(hasher.finalize());
    Ok((hash, size))
}

async fn scan_directory_recursive(
    dir: &Path,
    hashes: &mut HashMap<String, Vec<(String, u64)>>,
    total_files: &mut usize,
    total_size: &mut u64,
) {
    let mut dirs_to_scan = vec![dir.to_path_buf()];

    while let Some(current_dir) = dirs_to_scan.pop() {
        if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_file() {
                    if let Ok((hash, size)) = hash_file(&path).await {
                        hashes.entry(hash).or_default().push((path.to_string_lossy().to_string(), size));
                        *total_files += 1;
                        *total_size += size;
                    }
                } else if path.is_dir() {
                    dirs_to_scan.push(path);
                }
            }
        }
    }
}
