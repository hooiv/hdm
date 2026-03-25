/// Group Persistence — Save and load download groups to/from disk
///
/// Implements atomic file operations with backup rotation to prevent data loss.
/// Groups are stored in `~/.config/hyperstream/download-groups.json`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use crate::download_groups::DownloadGroup;

/// Serialize all persistence read-modify-write operations to prevent data races
static GROUP_PERSISTENCE_LOCK: std::sync::LazyLock<Mutex<()>> = 
    std::sync::LazyLock::new(|| Mutex::new(()));

/// Persistent representation of download groups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedGroups {
    /// All groups indexed by ID
    pub groups: HashMap<String, DownloadGroup>,
    /// Metadata about the persistence file
    #[serde(default)]
    pub meta: PersistenceMeta,
}

/// Metadata about the persistence file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceMeta {
    /// Schema version for forward compatibility
    pub version: u32,
    /// Last save timestamp (ISO 8601)
    #[serde(default)]
    pub last_saved: Option<String>,
    /// Number of groups persisted
    pub group_count: usize,
}

impl Default for PersistenceMeta {
    fn default() -> Self {
        Self {
            version: 1,
            last_saved: None,
            group_count: 0,
        }
    }
}

/// Get the path to the download-groups.json file
fn get_storage_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("download-groups.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".hyperstream")
        .join("download-groups.json")
}

/// Rotate backups: .bak3 ← .bak2 ← .bak1 ← current
fn rotate_backups(path: &PathBuf) {
    let bak3 = path.with_extension("json.bak3");
    let bak2 = path.with_extension("json.bak2");
    let bak1 = path.with_extension("json.bak1");

    let _ = fs::remove_file(&bak3);
    let _ = fs::rename(&bak2, &bak3);
    let _ = fs::rename(&bak1, &bak2);
    let _ = fs::copy(path, &bak1);
}

/// Write JSON atomically with backup rotation
fn write_json_atomically<T: Serialize + ?Sized>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Failed to write temp file: {}", e))?;

    // Rotate backups before overwriting
    if path.exists() {
        rotate_backups(path);
    }

    if let Err(_rename_err) = fs::rename(&tmp_path, path) {
        fs::write(path, &json).map_err(|e| format!("Failed to write file: {}", e))?;
        let _ = fs::remove_file(&tmp_path);
    }

    Ok(())
}

/// Load all groups from disk
pub fn load_groups() -> Result<PersistedGroups, String> {
    let _lock = GROUP_PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_storage_path();

    if !path.exists() {
        return Ok(PersistedGroups::default());
    }

    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read groups file: {}", e))?;

    let groups: PersistedGroups = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse groups JSON: {}", e))?;

    Ok(groups)
}

/// Save all groups to disk atomically
pub fn save_groups(groups: &HashMap<String, DownloadGroup>) -> Result<(), String> {
    let _lock = GROUP_PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_storage_path();

    let now = chrono::Utc::now().to_rfc3339();
    let persisted = PersistedGroups {
        groups: groups.clone(),
        meta: PersistenceMeta {
            version: 1,
            last_saved: Some(now),
            group_count: groups.len(),
        },
    };

    write_json_atomically(&path, &persisted)?;
    Ok(())
}

/// Add or update a single group (upsert)
pub fn upsert_group(group: &DownloadGroup) -> Result<(), String> {
    let mut persisted = load_groups()?;
    persisted.groups.insert(group.id.clone(), group.clone());
    persisted.meta.group_count = persisted.groups.len();
    
    let _lock = GROUP_PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_storage_path();
    write_json_atomically(&path, &persisted)?;
    Ok(())
}

/// Remove a group by ID
pub fn remove_group(id: &str) -> Result<(), String> {
    let mut persisted = load_groups()?;
    persisted.groups.remove(id);
    persisted.meta.group_count = persisted.groups.len();
    
    let _lock = GROUP_PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_storage_path();
    write_json_atomically(&path, &persisted)?;
    Ok(())
}

/// Load a specific group by ID
pub fn load_group(id: &str) -> Result<Option<DownloadGroup>, String> {
    let persisted = load_groups()?;
    Ok(persisted.groups.get(id).cloned())
}

/// Clear all groups (useful for testing or reset)
pub fn clear_all_groups() -> Result<(), String> {
    let _lock = GROUP_PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_storage_path();
    
    let empty = PersistedGroups::default();
    write_json_atomically(&path, &empty)?;
    Ok(())
}

/// Get count of persisted groups
pub fn get_group_count() -> Result<usize, String> {
    let persisted = load_groups()?;
    Ok(persisted.groups.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download_groups::DownloadGroup;

    #[test]
    fn test_empty_persistence() {
        // Clear first
        let _ = clear_all_groups();
        
        let result = load_groups();
        assert!(result.is_ok());
        let groups = result.unwrap();
        assert_eq!(groups.groups.len(), 0);
    }

    #[test]
    fn test_save_and_load_single_group() {
        let _ = clear_all_groups();
        
        let mut group = DownloadGroup::new("Test Group");
        let group_id = group.id.clone();
        group.add_member("https://example.com/file.zip", None);
        
        let result = upsert_group(&group);
        assert!(result.is_ok());

        let loaded = load_group(&group_id);
        assert!(loaded.is_ok());
        
        let loaded_group = loaded.unwrap();
        assert!(loaded_group.is_some());
        
        let loaded_group = loaded_group.unwrap();
        assert_eq!(loaded_group.id, group_id);
        assert_eq!(loaded_group.name, "Test Group");
        assert_eq!(loaded_group.members.len(), 1);
    }

    #[test]
    fn test_remove_group() {
        let _ = clear_all_groups();
        
        let group = DownloadGroup::new("To Remove");
        let group_id = group.id.clone();
        
        upsert_group(&group).unwrap();
        assert!(load_group(&group_id).unwrap().is_some());
        
        remove_group(&group_id).unwrap();
        assert!(load_group(&group_id).unwrap().is_none());
    }

    #[test]
    fn test_multiple_groups() {
        let _ = clear_all_groups();
        
        let group1 = DownloadGroup::new("Group 1");
        let group2 = DownloadGroup::new("Group 2");
        let group3 = DownloadGroup::new("Group 3");
        
        upsert_group(&group1).unwrap();
        upsert_group(&group2).unwrap();
        upsert_group(&group3).unwrap();
        
        let count = get_group_count().unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_upsert_updates_existing() {
        let _ = clear_all_groups();
        
        let mut group = DownloadGroup::new("Original Name");
        let group_id = group.id.clone();
        upsert_group(&group).unwrap();
        
        // Update the group
        group.name = "Updated Name".to_string();
        group.add_member("https://example.com/file.zip", None);
        upsert_group(&group).unwrap();
        
        let loaded = load_group(&group_id).unwrap().unwrap();
        assert_eq!(loaded.name, "Updated Name");
        assert_eq!(loaded.members.len(), 1);
    }
}
