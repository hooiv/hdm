use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataInfo {
    pub file_type: String,
    pub has_metadata: bool,
    pub metadata_fields: Vec<String>,
    pub estimated_removable_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubResult {
    pub success: bool,
    pub file_type: String,
    pub bytes_removed: u64,
    pub fields_removed: Vec<String>,
}

/// Detect file type from extension
fn detect_file_type(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("jpeg")
    } else if lower.ends_with(".png") {
        Some("png")
    } else if lower.ends_with(".pdf") {
        Some("pdf")
    } else {
        None
    }
}

/// Scrub metadata from a file (auto-detects type)
pub fn scrub_file(path: &str) -> Result<ScrubResult, String> {
    let file_type = detect_file_type(path)
        .ok_or_else(|| format!("Unsupported file type for metadata scrubbing: {}", path))?;
    
    match file_type {
        "jpeg" => scrub_jpeg(path),
        "png" => scrub_png(path),
        "pdf" => scrub_pdf(path),
        _ => Err("Unsupported file type".to_string()),
    }
}

/// Get metadata info from a file without modifying it
pub fn get_metadata_info(path: &str) -> Result<MetadataInfo, String> {
    let file_type = detect_file_type(path)
        .unwrap_or("unknown");
    
    match file_type {
        "jpeg" => get_jpeg_metadata_info(path),
        "png" => get_png_metadata_info(path),
        "pdf" => get_pdf_metadata_info(path),
        _ => Ok(MetadataInfo {
            file_type: "unknown".to_string(),
            has_metadata: false,
            metadata_fields: vec![],
            estimated_removable_bytes: 0,
        }),
    }
}

// ============ JPEG EXIF Stripping ============

/// JPEG markers
const JPEG_SOI: u8 = 0xD8;
const JPEG_APP1: u8 = 0xE1; // EXIF
const JPEG_APP2: u8 = 0xE2; // ICC Profile (optional strip)
const JPEG_APP12: u8 = 0xEC;
const JPEG_APP13: u8 = 0xED; // IPTC
const JPEG_APP14: u8 = 0xEE;
const JPEG_COM: u8 = 0xFE;   // Comment

fn is_metadata_marker(marker: u8) -> bool {
    // APP1 (EXIF), APP2 (ICC), APP12-APP14, COM
    marker == JPEG_APP1 || marker == JPEG_APP13 || marker == JPEG_COM
        || (marker >= JPEG_APP12 && marker <= JPEG_APP14)
}

fn scrub_jpeg(path: &str) -> Result<ScrubResult, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    
    if data.len() < 2 || data[0] != 0xFF || data[1] != JPEG_SOI {
        return Err("Not a valid JPEG file".to_string());
    }
    
    let mut output = Vec::with_capacity(data.len());
    let mut fields_removed = Vec::new();
    let mut bytes_removed: u64 = 0;
    
    // Copy SOI marker
    output.push(data[0]);
    output.push(data[1]);
    let mut i = 2;
    
    while i < data.len() - 1 {
        if data[i] != 0xFF {
            // Copy remaining data (image data after SOS)
            output.extend_from_slice(&data[i..]);
            break;
        }
        
        let marker = data[i + 1];
        
        // SOS (Start of Scan) - copy everything after this
        if marker == 0xDA {
            output.extend_from_slice(&data[i..]);
            break;
        }
        
        // Markers without length (RST, SOI, EOI, TEM)
        if marker == 0x00 || marker == 0x01 || (marker >= 0xD0 && marker <= 0xD9) {
            output.push(data[i]);
            output.push(data[i + 1]);
            i += 2;
            continue;
        }
        
        // Read segment length
        if i + 3 >= data.len() {
            output.extend_from_slice(&data[i..]);
            break;
        }
        
        let seg_len = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
        let total_seg_len = seg_len + 2; // +2 for the FF XX marker bytes
        
        if is_metadata_marker(marker) {
            // Strip this segment
            let name = match marker {
                JPEG_APP1 => "EXIF/XMP",
                JPEG_APP13 => "IPTC",
                JPEG_COM => "Comment",
                _ => "AppData",
            };
            fields_removed.push(format!("{} ({} bytes)", name, seg_len));
            bytes_removed += total_seg_len as u64;
        } else {
            // Keep this segment
            let end = (i + total_seg_len).min(data.len());
            output.extend_from_slice(&data[i..end]);
        }
        
        i += total_seg_len;
    }
    
    if bytes_removed > 0 {
        std::fs::write(path, &output).map_err(|e| format!("Failed to write file: {}", e))?;
    }
    
    Ok(ScrubResult {
        success: true,
        file_type: "jpeg".to_string(),
        bytes_removed,
        fields_removed,
    })
}

fn get_jpeg_metadata_info(path: &str) -> Result<MetadataInfo, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    
    if data.len() < 2 || data[0] != 0xFF || data[1] != JPEG_SOI {
        return Err("Not a valid JPEG file".to_string());
    }
    
    let mut metadata_fields = Vec::new();
    let mut total_metadata_bytes: u64 = 0;
    let mut i = 2;
    
    while i < data.len() - 1 {
        if data[i] != 0xFF { break; }
        let marker = data[i + 1];
        if marker == 0xDA { break; }
        if marker == 0x00 || marker == 0x01 || (marker >= 0xD0 && marker <= 0xD9) {
            i += 2;
            continue;
        }
        if i + 3 >= data.len() { break; }
        
        let seg_len = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
        let total_seg_len = seg_len + 2;
        
        if is_metadata_marker(marker) {
            let name = match marker {
                JPEG_APP1 => "EXIF/XMP",
                JPEG_APP13 => "IPTC",
                JPEG_COM => "Comment",
                _ => "AppData",
            };
            metadata_fields.push(format!("{} ({} bytes)", name, seg_len));
            total_metadata_bytes += total_seg_len as u64;
        }
        
        i += total_seg_len;
    }
    
    Ok(MetadataInfo {
        file_type: "jpeg".to_string(),
        has_metadata: !metadata_fields.is_empty(),
        metadata_fields,
        estimated_removable_bytes: total_metadata_bytes,
    })
}

// ============ PNG Metadata Stripping ============

/// PNG critical chunk types that must NOT be removed
fn is_critical_png_chunk(chunk_type: &[u8; 4]) -> bool {
    // IHDR, PLTE, IDAT, IEND are critical
    matches!(chunk_type, b"IHDR" | b"PLTE" | b"IDAT" | b"IEND"
        | b"tRNS" | b"cHRM" | b"gAMA" | b"iCCP" | b"sBIT"
        | b"sRGB" | b"bKGD" | b"hIST" | b"pHYs" | b"sPLT"
        | b"acTL" | b"fcTL" | b"fdAT") // Animation chunks
}

fn scrub_png(path: &str) -> Result<ScrubResult, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    
    // Check PNG signature
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("Not a valid PNG file".to_string());
    }
    
    let mut output = Vec::with_capacity(data.len());
    let mut fields_removed = Vec::new();
    let mut bytes_removed: u64 = 0;
    
    // Copy PNG signature
    output.extend_from_slice(&data[0..8]);
    let mut i = 8;
    
    while i + 12 <= data.len() {
        // Read chunk length (4 bytes big-endian)
        let length = u32::from_be_bytes([data[i], data[i+1], data[i+2], data[i+3]]) as usize;
        // Read chunk type (4 bytes)
        let mut chunk_type = [0u8; 4];
        chunk_type.copy_from_slice(&data[i+4..i+8]);
        
        let total_chunk_size = 4 + 4 + length + 4; // length + type + data + CRC
        
        if i + total_chunk_size > data.len() {
            // Malformed chunk, copy rest
            output.extend_from_slice(&data[i..]);
            break;
        }
        
        if is_critical_png_chunk(&chunk_type) {
            // Keep critical chunks
            output.extend_from_slice(&data[i..i + total_chunk_size]);
        } else {
            // Strip non-critical chunks (tEXt, iTXt, zTXt, eXIf, etc.)
            let type_str = String::from_utf8_lossy(&chunk_type).to_string();
            fields_removed.push(format!("{} ({} bytes)", type_str, length));
            bytes_removed += total_chunk_size as u64;
        }
        
        i += total_chunk_size;
    }
    
    if bytes_removed > 0 {
        std::fs::write(path, &output).map_err(|e| format!("Failed to write file: {}", e))?;
    }
    
    Ok(ScrubResult {
        success: true,
        file_type: "png".to_string(),
        bytes_removed,
        fields_removed,
    })
}

fn get_png_metadata_info(path: &str) -> Result<MetadataInfo, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("Not a valid PNG file".to_string());
    }
    
    let mut metadata_fields = Vec::new();
    let mut total_metadata_bytes: u64 = 0;
    let mut i = 8;
    
    while i + 12 <= data.len() {
        let length = u32::from_be_bytes([data[i], data[i+1], data[i+2], data[i+3]]) as usize;
        let mut chunk_type = [0u8; 4];
        chunk_type.copy_from_slice(&data[i+4..i+8]);
        let total_chunk_size = 4 + 4 + length + 4;
        
        if i + total_chunk_size > data.len() { break; }
        
        if !is_critical_png_chunk(&chunk_type) {
            let type_str = String::from_utf8_lossy(&chunk_type).to_string();
            metadata_fields.push(format!("{} ({} bytes)", type_str, length));
            total_metadata_bytes += total_chunk_size as u64;
        }
        
        i += total_chunk_size;
    }
    
    Ok(MetadataInfo {
        file_type: "png".to_string(),
        has_metadata: !metadata_fields.is_empty(),
        metadata_fields,
        estimated_removable_bytes: total_metadata_bytes,
    })
}

// ============ PDF Metadata Stripping ============

fn scrub_pdf(path: &str) -> Result<ScrubResult, String> {
    use lopdf::Document;
    
    let mut doc = Document::load(path)
        .map_err(|e| format!("Failed to load PDF: {}", e))?;
    
    let mut fields_removed = Vec::new();
    let original_size = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    
    // Clear the Info dictionary
    if let Ok(info_id) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_id.as_reference() {
            if let Ok(info_dict) = doc.get_dictionary(info_ref) {
                for (key, _) in info_dict.iter() {
                    let key_str = String::from_utf8_lossy(key).to_string();
                    fields_removed.push(format!("PDF::{}", key_str));
                }
            }
            // Remove the Info object
            doc.delete_object(info_ref);
        }
        // Remove trailer reference
        doc.trailer.remove(b"Info");
    }
    
    // Save
    doc.save(path)
        .map_err(|e| format!("Failed to save PDF: {}", e))?;
    
    let new_size = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    
    let bytes_removed = if original_size > new_size { original_size - new_size } else { 0 };
    
    Ok(ScrubResult {
        success: true,
        file_type: "pdf".to_string(),
        bytes_removed,
        fields_removed,
    })
}

fn get_pdf_metadata_info(path: &str) -> Result<MetadataInfo, String> {
    use lopdf::Document;
    
    let doc = Document::load(path)
        .map_err(|e| format!("Failed to load PDF: {}", e))?;
    
    let mut metadata_fields = Vec::new();
    
    if let Ok(info_id) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_id.as_reference() {
            if let Ok(info_dict) = doc.get_dictionary(info_ref) {
                for (key, _) in info_dict.iter() {
                    let key_str = String::from_utf8_lossy(key).to_string();
                    metadata_fields.push(format!("PDF::{}", key_str));
                }
            }
        }
    }
    
    Ok(MetadataInfo {
        file_type: "pdf".to_string(),
        has_metadata: !metadata_fields.is_empty(),
        metadata_fields,
        estimated_removable_bytes: 0, // Hard to estimate without rewriting
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_file_type() {
        assert_eq!(detect_file_type("photo.jpg"), Some("jpeg"));
        assert_eq!(detect_file_type("photo.JPEG"), Some("jpeg"));
        assert_eq!(detect_file_type("image.png"), Some("png"));
        assert_eq!(detect_file_type("doc.pdf"), Some("pdf"));
        assert_eq!(detect_file_type("file.txt"), None);
    }
    
    #[test]
    fn test_is_critical_png_chunk() {
        assert!(is_critical_png_chunk(b"IHDR"));
        assert!(is_critical_png_chunk(b"IDAT"));
        assert!(is_critical_png_chunk(b"IEND"));
        assert!(!is_critical_png_chunk(b"tEXt"));
        assert!(!is_critical_png_chunk(b"eXIf"));
    }
}
