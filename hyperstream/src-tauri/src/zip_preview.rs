use rquest::Client;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::fs::File;
use std::path::Path;
use flate2::read::DeflateDecoder;
use serde::{Serialize, Deserialize};
use zip::ZipArchive;

// Constants
#[allow(dead_code)]
const EOCD_SIGNATURE: u32 = 0x06054b50;
#[allow(dead_code)]
const CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x02014b50;
#[allow(dead_code)]
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;

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

// ================= REMOTE FUNCTIONS =================

/// Preview ZIP from a remote URL
pub async fn preview_zip_remote(url: String, client: Client) -> Result<ZipPreview, String> {
    // SSRF protection: block requests to private/loopback addresses
    crate::api_replay::validate_url_not_private(&url)?;

    // 1. Get Content-Length via HEAD
    let head = client.head(&url).send().await.map_err(|e| e.to_string())?;
    let content_length = head.content_length().ok_or("No Content-Length header")?;

    // 2. Fetch last 65KB (End of Central Directory Area)
    let fetch_size = std::cmp::min(content_length, 65536 + 22);
    let start_byte = content_length - fetch_size;
    
    let range_header = format!("bytes={}-{}", start_byte, content_length - 1);
    let bytes = client.get(&url)
        .header("Range", range_header)
        .send().await.map_err(|e| e.to_string())?
        .bytes().await.map_err(|e| e.to_string())?;

    // 3. Find EOCD Signature (backwards)
    let eocd_pos = find_eocd_signature(&bytes).ok_or("End of Central Directory not found")?;
    
    // 4. Parse EOCD
    let data = &bytes[eocd_pos..];
    let total_entries = u16::from_le_bytes([data[10], data[11]]) as usize;
    let cd_size = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as u64;
    let cd_offset = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as u64;

    // 5. Fetch Central Directory
    let cd_range_header = format!("bytes={}-{}", cd_offset, cd_offset + cd_size - 1);
    let cd_bytes = client.get(&url)
        .header("Range", cd_range_header)
        .send().await.map_err(|e| e.to_string())?
        .bytes().await.map_err(|e| e.to_string())?;

    // 6. Parse Central Directory Entries
    let entries = parse_central_directory(&cd_bytes, total_entries)?;

    Ok(ZipPreview {
        total_files: entries.iter().filter(|e| !e.is_directory).count(),
        total_directories: entries.iter().filter(|e| e.is_directory).count(),
        total_compressed_size: entries.iter().map(|e| e.compressed_size).sum(),
        total_uncompressed_size: entries.iter().map(|e| e.uncompressed_size).sum(),
        entries,
    })
}

/// Download a specific entry from remote ZIP
pub async fn download_entry_remote(url: String, entry_name: String, client: Client) -> Result<Vec<u8>, String> {
    // SSRF protection: block requests to private/loopback addresses
    crate::api_replay::validate_url_not_private(&url)?;

    let head = client.head(&url).send().await.map_err(|e| e.to_string())?;
    let content_length = head.content_length().ok_or("No Content-Length")?;
    
    let fetch_size = std::cmp::min(content_length, 65536 + 22);
    let start_byte = content_length - fetch_size;
    let range_header = format!("bytes={}-{}", start_byte, content_length - 1);
    let bytes = client.get(&url).header("Range", range_header).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    let eocd_pos = find_eocd_signature(&bytes).ok_or("EOCD not found")?;
    let data = &bytes[eocd_pos..];
    let cd_offset = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as u64;
    let cd_size = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as u64;

    let cd_range = format!("bytes={}-{}", cd_offset, cd_offset + cd_size - 1);
    let cd_bytes = client.get(&url).header("Range", cd_range).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;

    let mut cursor = 0;
    let mut target_offset = 0;
    let mut target_compressed_size = 0;
    let mut target_method = 0;
    let mut found = false;

    while cursor < cd_bytes.len() {
        if cd_bytes.len() - cursor < 46 { break; }
        if cd_bytes[cursor] != 0x50 || cd_bytes[cursor+1] != 0x4b { break; } 

        let method = u16::from_le_bytes([cd_bytes[cursor+10], cd_bytes[cursor+11]]);
        let comp_size = u32::from_le_bytes([cd_bytes[cursor+20], cd_bytes[cursor+21], cd_bytes[cursor+22], cd_bytes[cursor+23]]) as u64;
        let filename_len = u16::from_le_bytes([cd_bytes[cursor+28], cd_bytes[cursor+29]]) as usize;
        let extra_len = u16::from_le_bytes([cd_bytes[cursor+30], cd_bytes[cursor+31]]) as usize;
        let comment_len = u16::from_le_bytes([cd_bytes[cursor+32], cd_bytes[cursor+33]]) as usize;
        let local_offset = u32::from_le_bytes([cd_bytes[cursor+42], cd_bytes[cursor+43], cd_bytes[cursor+44], cd_bytes[cursor+45]]) as u64;

        let name_start = cursor + 46;
        let name = String::from_utf8_lossy(&cd_bytes[name_start..name_start+filename_len]).to_string();

        if name == entry_name {
            target_offset = local_offset;
            target_compressed_size = comp_size;
            target_method = method;
            found = true;
            break;
        }

        cursor += 46 + filename_len + extra_len + comment_len;
    }

    if !found { return Err("Entry not found in ZIP".to_string()); }

    let lh_range = format!("bytes={}-{}", target_offset, target_offset + 511);
    let lh_bytes = client.get(&url).header("Range", lh_range).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;

    if lh_bytes[0] != 0x50 || lh_bytes[1] != 0x4b || lh_bytes[2] != 0x03 || lh_bytes[3] != 0x04 {
        return Err("Invalid Local File Header signature".to_string());
    }
    
    let lh_filename_len = u16::from_le_bytes([lh_bytes[26], lh_bytes[27]]) as u64;
    let lh_extra_len = u16::from_le_bytes([lh_bytes[28], lh_bytes[29]]) as u64;
    
    let data_start = target_offset + 30 + lh_filename_len + lh_extra_len;
    let data_end = data_start + target_compressed_size - 1;

    let data_range = format!("bytes={}-{}", data_start, data_end);
    let compressed_data = client.get(&url).header("Range", data_range).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;

    match target_method {
        0 => Ok(compressed_data.to_vec()),
        8 => {
            let mut decoder = DeflateDecoder::new(Cursor::new(compressed_data));
            let mut decompressed = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut decompressed).map_err(|e| format!("Decompression failed: {}", e))?;
            Ok(decompressed)
        },
        _ => Err(format!("Unsupported compression method: {}", target_method))
    }
}

// ================= HELPER FUNCTIONS =================

fn find_eocd_signature(data: &[u8]) -> Option<usize> {
    if data.len() < 22 { return None; }
    for i in (0..data.len() - 3).rev() {
        if data[i] == 0x50 && data[i+1] == 0x4b && data[i+2] == 0x05 && data[i+3] == 0x06 {
            return Some(i);
        }
    }
    None
}

fn parse_central_directory(data: &[u8], expected_count: usize) -> Result<Vec<ZipEntry>, String> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    
    while cursor < data.len() && entries.len() < expected_count {
        if data.len() - cursor < 46 { break; } 
        
        if data[cursor] != 0x50 || data[cursor+1] != 0x4b || data[cursor+2] != 0x01 || data[cursor+3] != 0x02 {
            break;
        }

        let compressed_size = u32::from_le_bytes([data[cursor+20], data[cursor+21], data[cursor+22], data[cursor+23]]) as u64;
        let uncompressed_size = u32::from_le_bytes([data[cursor+24], data[cursor+25], data[cursor+26], data[cursor+27]]) as u64;
        let filename_len = u16::from_le_bytes([data[cursor+28], data[cursor+29]]) as usize;
        let extra_len = u16::from_le_bytes([data[cursor+30], data[cursor+31]]) as usize;
        let comment_len = u16::from_le_bytes([data[cursor+32], data[cursor+33]]) as usize;
        
        let filename_start = cursor + 46;
        let filename_end = filename_start + filename_len;

        if filename_end > data.len() { break; }

        let filename = String::from_utf8_lossy(&data[filename_start..filename_end]).to_string();
        
        entries.push(ZipEntry {
            name: filename.clone(),
            is_directory: filename.ends_with('/'),
            compressed_size,
            uncompressed_size,
            compression_method: format!("{}", u16::from_le_bytes([data[cursor+10], data[cursor+11]])),
        });

        cursor += 46 + filename_len + extra_len + comment_len;
    }

    Ok(entries)
}

// ================= LOCAL FUNCTIONS (RESTORED) =================

/// Preview the contents of a LOCAL ZIP file
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
pub fn preview_zip_partial(data: &[u8]) -> Result<ZipPreview, String> {
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
    
    let total_entries = u16::from_le_bytes([data[eocd_pos + 10], data[eocd_pos + 11]]) as usize;
    
    Ok(ZipPreview {
        total_files: total_entries,
        total_directories: 0, 
        total_compressed_size: 0,
        total_uncompressed_size: 0,
        entries: Vec::new(), 
    })
}

/// Extract a single file from a LOCAL ZIP archive
pub fn extract_file(zip_path: &Path, entry_name: &str, dest_path: &Path) -> Result<(), String> {
    // Validate entry_name doesn't contain path traversal sequences
    if entry_name.contains("..") {
        return Err("Entry name contains path traversal sequence".to_string());
    }

    // Validate dest_path stays within its intended parent directory
    if let Some(parent) = dest_path.parent() {
        let canonical_parent = dunce::canonicalize(parent)
            .unwrap_or_else(|_| parent.to_path_buf());
        // Resolve dest_path relative to parent — since file doesn't exist yet, join the file_name
        let resolved_dest = canonical_parent.join(
            dest_path.file_name().ok_or("Invalid destination filename")?
        );
        if !resolved_dest.starts_with(&canonical_parent) {
            return Err("Destination path escapes parent directory".to_string());
        }
    }
    
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

/// Sanitize a zip entry name to prevent path traversal (Zip Slip).
fn sanitize_zip_entry(name: &str) -> Option<std::path::PathBuf> {
    use std::path::Component;
    let path = std::path::Path::new(name);
    let mut clean = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(c) => clean.push(c),
            Component::CurDir => {} // skip "."
            // Reject "..", root, and prefix components
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        return None;
    }
    Some(clean)
}

/// Extract all files from a LOCAL ZIP archive
pub fn extract_all(zip_path: &Path, dest_dir: &Path) -> Result<usize, String> {
    let file = File::open(zip_path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP file: {}", e))?;
    
    let canonical_dest = dest_dir.canonicalize().unwrap_or_else(|_| dest_dir.to_path_buf());
    let mut extracted = 0;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Failed to read entry: {}", e))?;
        
        let sanitized = match sanitize_zip_entry(file.name()) {
            Some(p) => p,
            None => {
                eprintln!("Skipping unsafe zip entry: {}", file.name());
                continue;
            }
        };
        let outpath = dest_dir.join(&sanitized);
        
        // Double-check: resolved path must be inside dest_dir
        let canonical_out = outpath.canonicalize().unwrap_or_else(|_| outpath.clone());
        if !canonical_out.starts_with(&canonical_dest) && !outpath.starts_with(dest_dir) {
            eprintln!("Zip entry escapes destination, skipping: {}", file.name());
            continue;
        }
        
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

/// Read specific bytes from a file
pub fn read_bytes_at_offset(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    // Cap read length to 10 MB to prevent OOM
    const MAX_READ_LENGTH: usize = 10 * 1024 * 1024;
    if length > MAX_READ_LENGTH {
        return Err(format!("Read length {} exceeds maximum {} bytes", length, MAX_READ_LENGTH));
    }
    // Validate path is within download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(path) {
        if !canon.starts_with(&download_dir) {
            return Err("Path must be within the download directory".to_string());
        }
    }

    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| format!("Failed to seek: {}", e))?;
    let mut buffer = vec![0u8; length];
    let bytes_read = file.read(&mut buffer).map_err(|e| format!("Failed to read: {}", e))?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

/// Read the last N bytes of a file
pub fn read_last_bytes(path: &Path, length: usize) -> Result<Vec<u8>, String> {
    // Cap read length to 10 MB
    const MAX_READ_LENGTH: usize = 10 * 1024 * 1024;
    if length > MAX_READ_LENGTH {
        return Err(format!("Read length {} exceeds maximum {} bytes", length, MAX_READ_LENGTH));
    }

    // Validate path is within the download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(path) {
        if !canon.starts_with(&download_dir) {
            return Err("Path must be within the download directory".to_string());
        }
    }

    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let file_size = file.seek(SeekFrom::End(0)).map_err(|e| format!("Failed to get file size: {}", e))?;
    let start = file_size.saturating_sub(length as u64);
    file.seek(SeekFrom::Start(start)).map_err(|e| format!("Failed to seek: {}", e))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| format!("Failed to read: {}", e))?;
    Ok(buffer)
}
