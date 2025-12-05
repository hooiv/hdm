use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use serde::{Serialize, Deserialize};
use zip::ZipArchive;

/// Information about a file in a ZIP archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipEntry {
    pub name: String,
    pub is_directory: bool,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub compression_method: String,
}

/// Preview information for a ZIP file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipPreview {
    pub total_files: usize,
    pub total_directories: usize,
    pub total_compressed_size: u64,
    pub total_uncompressed_size: u64,
    pub entries: Vec<ZipEntry>,
}

/// Preview the contents of a ZIP file
pub fn preview_zip(path: &Path) -> Result<ZipPreview, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP file: {}", e))?;
    
    let mut entries = Vec::new();
    let mut total_compressed = 0u64;
    let mut total_uncompressed = 0u64;
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| format!("Failed to read entry: {}", e))?;
        
        let is_dir = file.is_dir();
        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
        }
        
        total_compressed += file.compressed_size();
        total_uncompressed += file.size();
        
        entries.push(ZipEntry {
            name: file.name().to_string(),
            is_directory: is_dir,
            compressed_size: file.compressed_size(),
            uncompressed_size: file.size(),
            compression_method: format!("{:?}", file.compression()),
        });
    }
    
    Ok(ZipPreview {
        total_files,
        total_directories: total_dirs,
        total_compressed_size: total_compressed,
        total_uncompressed_size: total_uncompressed,
        entries,
    })
}

/// Preview ZIP from a partial download (first ~64KB for central directory)
/// This uses the End of Central Directory record to determine structure
pub fn preview_zip_partial(data: &[u8]) -> Result<ZipPreview, String> {
    // ZIP files have an End of Central Directory (EOCD) at the end
    // We need to find it and parse the central directory
    
    // Look for EOCD signature (0x06054b50) from the end
    let eocd_sig: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    
    let mut eocd_pos = None;
    for i in (0..data.len().saturating_sub(22)).rev() {
        if data[i..].starts_with(&eocd_sig) {
            eocd_pos = Some(i);
            break;
        }
    }
    
    let eocd_pos = eocd_pos.ok_or("Could not find End of Central Directory")?;
    
    if data.len() < eocd_pos + 22 {
        return Err("Incomplete EOCD record".to_string());
    }
    
    // Parse EOCD
    let total_entries = u16::from_le_bytes([data[eocd_pos + 10], data[eocd_pos + 11]]) as usize;
    let _central_dir_size = u32::from_le_bytes([
        data[eocd_pos + 12],
        data[eocd_pos + 13],
        data[eocd_pos + 14],
        data[eocd_pos + 15],
    ]) as usize;
    let _central_dir_offset = u32::from_le_bytes([
        data[eocd_pos + 16],
        data[eocd_pos + 17],
        data[eocd_pos + 18],
        data[eocd_pos + 19],
    ]) as usize;
    
    // For partial preview, we report what we know
    Ok(ZipPreview {
        total_files: total_entries,
        total_directories: 0, // Can't determine without parsing entries
        total_compressed_size: 0,
        total_uncompressed_size: 0,
        entries: Vec::new(), // Would need full central directory
    })
}

/// Extract a single file from a ZIP archive
pub fn extract_file(zip_path: &Path, entry_name: &str, dest_path: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP file: {}", e))?;
    
    let mut zip_file = archive.by_name(entry_name)
        .map_err(|e| format!("Entry not found: {}", e))?;
    
    let mut outfile = File::create(dest_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    
    std::io::copy(&mut zip_file, &mut outfile)
        .map_err(|e| format!("Failed to extract: {}", e))?;
    
    Ok(())
}

/// Extract all files from a ZIP archive
pub fn extract_all(zip_path: &Path, dest_dir: &Path) -> Result<usize, String> {
    let file = File::open(zip_path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP file: {}", e))?;
    
    let mut extracted = 0;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Failed to read entry: {}", e))?;
        let outpath = dest_dir.join(file.name());
        
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = File::create(&outpath)
                .map_err(|e| format!("Failed to create {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to extract {}: {}", file.name(), e))?;
            extracted += 1;
        }
    }
    
    Ok(extracted)
}

/// Read specific bytes from a file at an offset (uses Read, Seek, SeekFrom traits)
/// Useful for reading parts of large ZIP files without loading everything
pub fn read_bytes_at_offset(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    
    // Use SeekFrom to position the file cursor
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Failed to seek: {}", e))?;
    
    // Read the specified number of bytes
    let mut buffer = vec![0u8; length];
    let bytes_read = file.read(&mut buffer)
        .map_err(|e| format!("Failed to read: {}", e))?;
    
    buffer.truncate(bytes_read);
    Ok(buffer)
}

/// Read the last N bytes of a file (useful for reading ZIP EOCD)
pub fn read_last_bytes(path: &Path, length: usize) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    
    // Get file size by seeking to end
    let file_size = file.seek(SeekFrom::End(0))
        .map_err(|e| format!("Failed to get file size: {}", e))?;
    
    // Calculate start position
    let start = file_size.saturating_sub(length as u64);
    
    // Seek to start position
    file.seek(SeekFrom::Start(start))
        .map_err(|e| format!("Failed to seek: {}", e))?;
    
    // Read remaining bytes
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("Failed to read: {}", e))?;
    
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_eocd_parsing() {
        // Minimal valid ZIP file (empty)
        let empty_zip: Vec<u8> = vec![
            0x50, 0x4b, 0x05, 0x06, // EOCD signature
            0x00, 0x00, // Disk number
            0x00, 0x00, // Disk with central directory
            0x00, 0x00, // Total entries on this disk
            0x00, 0x00, // Total entries
            0x00, 0x00, 0x00, 0x00, // Central directory size
            0x00, 0x00, 0x00, 0x00, // Central directory offset
            0x00, 0x00, // Comment length
        ];
        
        let result = preview_zip_partial(&empty_zip);
        assert!(result.is_ok());
    }
}
