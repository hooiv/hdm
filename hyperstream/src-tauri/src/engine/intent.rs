use std::path::PathBuf;

/// An Intent is strictly a metadata desire. It has no temporal state (e.g., "Downloading", "Paused").
/// It maps an expected cryptographic hash of data to its spatial manifestation on disk.
/// It completely isolates the user's desire from the mechanical realities of acquiring it.
#[derive(Debug, Clone)]
pub struct AcquisitionIntent {
    pub intent_id: String,
    pub target_hash: String,           // The ultimate truth of the data (e.g., SHA-256)
    pub dynamic_sources: Vec<String>,  // Swarm hunting grounds (URLs, P2P magnets)
    pub manifest_path: PathBuf,        // C:\User\Downloads\file.zip
    pub known_size: Option<u64>,       
    pub priority: u8,
}

impl AcquisitionIntent {
    pub fn new(id: &str, hash: &str, path: PathBuf) -> Self {
        Self {
            intent_id: id.to_string(),
            target_hash: hash.to_string(),
            dynamic_sources: vec![],
            manifest_path: path,
            known_size: None,
            priority: 1,
        }
    }
}
