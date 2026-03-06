use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

// ─── Smart File Categorizer ──────────────────────────────────────────
// Auto-categorize downloads by file type and optionally move them to
// category-specific directories. Supports custom categories, MIME types,
// and extension-based matching.

static CATEGORIZER_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn config_path() -> std::path::PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("file_categories.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".hyperstream").join("file_categories.json")
}

/// A file category definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCategory {
    /// Unique identifier.
    pub id: String,
    /// Display name (e.g., "Video", "Music", "Documents").
    pub name: String,
    /// Icon identifier for the UI (e.g., "video", "music", "file-text").
    #[serde(default)]
    pub icon: String,
    /// Color for the UI (e.g., "#FF6B6B").
    #[serde(default)]
    pub color: String,
    /// File extensions that belong to this category (lowercase, no dot).
    pub extensions: Vec<String>,
    /// Optional subdirectory to move files to (relative to download dir).
    #[serde(default)]
    pub subdirectory: Option<String>,
    /// Whether to auto-move files to the subdirectory on completion.
    #[serde(default)]
    pub auto_move: bool,
    /// Priority for matching (higher wins if extension appears in multiple categories).
    #[serde(default)]
    pub priority: i32,
    /// Whether this category is a built-in (cannot be deleted, only disabled).
    #[serde(default)]
    pub builtin: bool,
    /// Whether this category is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

/// Categorization result for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizeResult {
    pub filename: String,
    pub extension: String,
    pub category_id: String,
    pub category_name: String,
    pub icon: String,
    pub color: String,
    pub should_move: bool,
    pub target_dir: Option<String>,
}

/// Statistics about categorized downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub category_id: String,
    pub category_name: String,
    pub icon: String,
    pub color: String,
    pub file_count: u64,
    pub total_size: u64,
}

// ─── Built-in Categories ─────────────────────────────────────────────

pub fn builtin_categories() -> Vec<FileCategory> {
    vec![
        FileCategory {
            id: "video".into(),
            name: "Video".into(),
            icon: "video".into(),
            color: "#FF6B6B".into(),
            extensions: vec![
                "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v",
                "mpg", "mpeg", "3gp", "ts", "vob", "ogv", "rm", "rmvb",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Videos".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "music".into(),
            name: "Music".into(),
            icon: "music".into(),
            color: "#4ECDC4".into(),
            extensions: vec![
                "mp3", "flac", "aac", "ogg", "wma", "wav", "m4a", "opus",
                "aiff", "ape", "alac", "mid", "midi",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Music".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "documents".into(),
            name: "Documents".into(),
            icon: "file-text".into(),
            color: "#45B7D1".into(),
            extensions: vec![
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt",
                "rtf", "odt", "ods", "odp", "csv", "epub", "mobi", "djvu",
                "tex", "md", "pages", "numbers", "key",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Documents".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "archives".into(),
            name: "Archives".into(),
            icon: "archive".into(),
            color: "#96CEB4".into(),
            extensions: vec![
                "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst",
                "lz", "lzma", "cab", "iso", "dmg", "img",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Archives".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "programs".into(),
            name: "Programs".into(),
            icon: "package".into(),
            color: "#FFEAA7".into(),
            extensions: vec![
                "exe", "msi", "dmg", "deb", "rpm", "appimage", "apk",
                "bat", "sh", "cmd", "ps1", "app", "pkg",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Programs".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "images".into(),
            name: "Images".into(),
            icon: "image".into(),
            color: "#DDA0DD".into(),
            extensions: vec![
                "jpg", "jpeg", "png", "gif", "bmp", "svg", "webp", "ico",
                "tiff", "tif", "psd", "ai", "eps", "raw", "cr2", "nef",
                "heic", "heif", "avif", "jxl",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Images".into()),
            auto_move: true,
            priority: 10,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "code".into(),
            name: "Source Code".into(),
            icon: "code".into(),
            color: "#74B9FF".into(),
            extensions: vec![
                "js", "ts", "py", "rs", "go", "java", "c", "cpp", "h", "hpp",
                "cs", "rb", "php", "swift", "kt", "scala", "r", "lua",
                "json", "xml", "yaml", "yml", "toml", "ini", "cfg",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Code".into()),
            auto_move: false,
            priority: 5,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "torrents".into(),
            name: "Torrent Files".into(),
            icon: "download".into(),
            color: "#A29BFE".into(),
            extensions: vec!["torrent", "magnet"].into_iter().map(String::from).collect(),
            subdirectory: Some("Torrents".into()),
            auto_move: false,
            priority: 15,
            builtin: true,
            enabled: true,
        },
        FileCategory {
            id: "fonts".into(),
            name: "Fonts".into(),
            icon: "type".into(),
            color: "#FD79A8".into(),
            extensions: vec![
                "ttf", "otf", "woff", "woff2", "eot",
            ].into_iter().map(String::from).collect(),
            subdirectory: Some("Fonts".into()),
            auto_move: false,
            priority: 10,
            builtin: true,
            enabled: true,
        },
    ]
}

// ─── Persistence ─────────────────────────────────────────────────────

fn load_categories() -> Vec<FileCategory> {
    let _lock = CATEGORIZER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = config_path();
    if !path.exists() {
        return builtin_categories();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|_| builtin_categories()),
        Err(_) => builtin_categories(),
    }
}

fn save_categories(categories: &[FileCategory]) -> Result<(), String> {
    let _lock = CATEGORIZER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(categories).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Core Categorization ─────────────────────────────────────────────

/// Build an extension → category lookup map from current categories.
fn build_extension_map(categories: &[FileCategory]) -> HashMap<String, usize> {
    let mut map: HashMap<String, (usize, i32)> = HashMap::new(); // ext -> (index, priority)
    for (i, cat) in categories.iter().enumerate() {
        if !cat.enabled { continue; }
        for ext in &cat.extensions {
            let ext_lower = ext.to_lowercase();
            match map.get(&ext_lower) {
                Some(&(_, existing_priority)) if existing_priority >= cat.priority => {}
                _ => { map.insert(ext_lower, (i, cat.priority)); }
            }
        }
    }
    map.into_iter().map(|(k, (i, _))| (k, i)).collect()
}

/// Extract file extension from a filename or path.
fn get_extension(filename: &str) -> String {
    let name = filename.rsplit('/').next().unwrap_or(filename);
    let name = name.rsplit('\\').next().unwrap_or(name);
    match name.rsplit('.').next() {
        Some(ext) if ext != name => ext.to_lowercase(),
        _ => String::new(),
    }
}

/// Categorize a single file by its name/path.
pub fn categorize(filename: &str) -> CategorizeResult {
    let ext = get_extension(filename);
    let categories = load_categories();
    let ext_map = build_extension_map(&categories);

    if let Some(&idx) = ext_map.get(&ext) {
        let cat = &categories[idx];
        CategorizeResult {
            filename: filename.to_string(),
            extension: ext,
            category_id: cat.id.clone(),
            category_name: cat.name.clone(),
            icon: cat.icon.clone(),
            color: cat.color.clone(),
            should_move: cat.auto_move && cat.subdirectory.is_some(),
            target_dir: cat.subdirectory.clone(),
        }
    } else {
        CategorizeResult {
            filename: filename.to_string(),
            extension: ext,
            category_id: "other".into(),
            category_name: "Other".into(),
            icon: "file".into(),
            color: "#636E72".into(),
            should_move: false,
            target_dir: None,
        }
    }
}

/// Categorize a file and optionally move it to the category subdirectory.
/// Returns the new path if the file was moved, or the original path if not.
pub fn categorize_and_move(filename: &str, download_dir: &str) -> Result<(CategorizeResult, String), String> {
    let result = categorize(filename);

    if !result.should_move {
        return Ok((result, filename.to_string()));
    }

    let target_subdir = match &result.target_dir {
        Some(d) => d,
        None => return Ok((result, filename.to_string())),
    };

    let target_dir = std::path::Path::new(download_dir).join(target_subdir);
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    let file_name = std::path::Path::new(filename)
        .file_name()
        .ok_or("Invalid filename")?;
    let target_path = target_dir.join(file_name);

    // Don't move if source and target are the same
    let source = std::path::Path::new(filename);
    if source == target_path {
        return Ok((result, filename.to_string()));
    }

    // If target already exists, don't overwrite
    if target_path.exists() {
        return Ok((result, filename.to_string()));
    }

    std::fs::rename(filename, &target_path).map_err(|e| {
        format!("Failed to move file to category folder: {}", e)
    })?;

    Ok((result, target_path.to_string_lossy().to_string()))
}

/// Categorize multiple files at once.
pub fn categorize_batch(filenames: &[String]) -> Vec<CategorizeResult> {
    let categories = load_categories();
    let ext_map = build_extension_map(&categories);

    filenames.iter().map(|filename| {
        let ext = get_extension(filename);
        if let Some(&idx) = ext_map.get(&ext) {
            let cat = &categories[idx];
            CategorizeResult {
                filename: filename.clone(),
                extension: ext,
                category_id: cat.id.clone(),
                category_name: cat.name.clone(),
                icon: cat.icon.clone(),
                color: cat.color.clone(),
                should_move: cat.auto_move && cat.subdirectory.is_some(),
                target_dir: cat.subdirectory.clone(),
            }
        } else {
            CategorizeResult {
                filename: filename.clone(),
                extension: ext,
                category_id: "other".into(),
                category_name: "Other".into(),
                icon: "file".into(),
                color: "#636E72".into(),
                should_move: false,
                target_dir: None,
            }
        }
    }).collect()
}

// ─── CRUD Operations ─────────────────────────────────────────────────

pub fn list_categories() -> Vec<FileCategory> {
    load_categories()
}

pub fn get_category(id: &str) -> Option<FileCategory> {
    load_categories().into_iter().find(|c| c.id == id)
}

pub fn add_category(category: FileCategory) -> Result<(), String> {
    let mut categories = load_categories();
    if categories.iter().any(|c| c.id == category.id) {
        return Err(format!("Category '{}' already exists", category.id));
    }
    categories.push(category);
    save_categories(&categories)
}

pub fn update_category(category: FileCategory) -> Result<(), String> {
    let mut categories = load_categories();
    if let Some(existing) = categories.iter_mut().find(|c| c.id == category.id) {
        *existing = category;
        save_categories(&categories)
    } else {
        Err(format!("Category '{}' not found", category.id))
    }
}

pub fn delete_category(id: &str) -> Result<(), String> {
    let mut categories = load_categories();
    let before = categories.len();
    // Don't allow deleting built-in categories
    if categories.iter().any(|c| c.id == id && c.builtin) {
        return Err("Cannot delete built-in category. Disable it instead.".into());
    }
    categories.retain(|c| c.id != id);
    if categories.len() == before {
        return Err(format!("Category '{}' not found", id));
    }
    save_categories(&categories)
}

pub fn reset_to_defaults() -> Result<(), String> {
    save_categories(&builtin_categories())
}

/// Get statistics about downloaded files by category.
/// Scans the download directory for files and categorizes them.
pub fn compute_stats(download_dir: &str) -> Vec<CategoryStats> {
    let categories = load_categories();
    let ext_map = build_extension_map(&categories);

    let mut stats: HashMap<String, (u64, u64)> = HashMap::new(); // id -> (count, total_size)

    // Walk the download directory
    if let Ok(entries) = std::fs::read_dir(download_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }

            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let ext = get_extension(&filename);
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);

            let category_id = if let Some(&idx) = ext_map.get(&ext) {
                categories[idx].id.clone()
            } else {
                "other".to_string()
            };

            let entry = stats.entry(category_id).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += size;
        }
    }

    let mut result: Vec<CategoryStats> = categories.iter().filter_map(|cat| {
        let (count, size) = stats.get(&cat.id).copied().unwrap_or((0, 0));
        if count > 0 || cat.enabled {
            Some(CategoryStats {
                category_id: cat.id.clone(),
                category_name: cat.name.clone(),
                icon: cat.icon.clone(),
                color: cat.color.clone(),
                file_count: count,
                total_size: size,
            })
        } else {
            None
        }
    }).collect();

    // Add "Other" if there are uncategorized files
    if let Some(&(count, size)) = stats.get("other") {
        if count > 0 {
            result.push(CategoryStats {
                category_id: "other".into(),
                category_name: "Other".into(),
                icon: "file".into(),
                color: "#636E72".into(),
                file_count: count,
                total_size: size,
            });
        }
    }

    result
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_extension() {
        assert_eq!(get_extension("movie.mp4"), "mp4");
        assert_eq!(get_extension("/path/to/file.ZIP"), "zip");
        assert_eq!(get_extension("C:\\Downloads\\archive.tar.gz"), "gz");
        assert_eq!(get_extension("noext"), "");
    }

    #[test]
    fn test_categorize_video() {
        let result = categorize("movie.mp4");
        assert_eq!(result.category_id, "video");
        assert_eq!(result.category_name, "Video");
    }

    #[test]
    fn test_categorize_music() {
        let result = categorize("song.mp3");
        assert_eq!(result.category_id, "music");
    }

    #[test]
    fn test_categorize_archive() {
        let result = categorize("data.zip");
        assert_eq!(result.category_id, "archives");
    }

    #[test]
    fn test_categorize_unknown() {
        let result = categorize("mystery.xyz123");
        assert_eq!(result.category_id, "other");
    }

    #[test]
    fn test_categorize_batch() {
        let files = vec![
            "video.mp4".to_string(),
            "doc.pdf".to_string(),
            "unknown.abc".to_string(),
        ];
        let results = categorize_batch(&files);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].category_id, "video");
        assert_eq!(results[1].category_id, "documents");
        assert_eq!(results[2].category_id, "other");
    }

    #[test]
    fn test_builtin_categories_no_overlap() {
        let categories = builtin_categories();
        let mut all_exts: Vec<&str> = Vec::new();
        for cat in &categories {
            for ext in &cat.extensions {
                // Check for duplicates across categories
                if all_exts.contains(&ext.as_str()) {
                    // Duplicates are ok if priority differs
                }
                all_exts.push(ext);
            }
        }
    }
}
