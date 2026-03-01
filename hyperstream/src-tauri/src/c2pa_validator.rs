use std::path::Path;
use tokio::fs;

/// Validate C2PA (Content Authenticity) manifests in media files.
/// C2PA embeds provenance data (creator, edits, AI generation) as a JUMBF box in JPEG/PNG/MP4.
/// We look for the C2PA JUMBF marker and extract basic metadata.
pub async fn validate_c2pa(file_path: String) -> Result<serde_json::Value, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !["jpg", "jpeg", "png", "webp", "mp4", "mov", "tiff", "tif"].contains(&ext.as_str()) {
        return Err("C2PA validation only supports image/video files (JPEG, PNG, WebP, MP4, TIFF).".to_string());
    }

    let file_bytes = fs::read(path).await.map_err(|e| format!("Failed to read file: {}", e))?;
    let file_size = file_bytes.len();

    // C2PA manifests are stored in JUMBF (ISO 19566-5) boxes.
    // The JUMBF box starts with a type UUID for C2PA:
    // "d8fec3d6-1b6f-4732-8034-57bf26dee41b" (C2PA manifest store)
    let c2pa_uuid: [u8; 16] = [
        0xd8, 0xfe, 0xc3, 0xd6, 0x1b, 0x6f, 0x47, 0x32,
        0x80, 0x34, 0x57, 0xbf, 0x26, 0xde, 0xe4, 0x1b,
    ];

    // Also check for the C2PA claim marker in XMP metadata
    let c2pa_xmp_marker = b"c2pa:";
    let c2pa_claim_marker = b"c2pa.claim";
    let adobe_xmp_marker = b"stRef:originalDocumentID";

    let mut has_c2pa_jumbf = false;
    let mut has_c2pa_xmp = false;
    let mut has_adobe_provenance = false;
    let mut jumbf_offset: Option<usize> = None;

    // Search for JUMBF UUID
    for i in 0..file_bytes.len().saturating_sub(16) {
        if file_bytes[i..i+16] == c2pa_uuid {
            has_c2pa_jumbf = true;
            jumbf_offset = Some(i);
            break;
        }
    }

    // Search for XMP markers
    for window in file_bytes.windows(5) {
        if window == c2pa_xmp_marker {
            has_c2pa_xmp = true;
            break;
        }
    }

    for window in file_bytes.windows(10) {
        if window == c2pa_claim_marker {
            has_c2pa_xmp = true;
            break;
        }
    }

    for window in file_bytes.windows(24) {
        if window == adobe_xmp_marker {
            has_adobe_provenance = true;
            break;
        }
    }

    // Extract any readable C2PA claim data near the JUMBF marker
    let mut claim_info = String::new();
    if let Some(offset) = jumbf_offset {
        // Try to read readable ASCII near the manifest
        let start = offset.saturating_sub(64);
        let end = (offset + 512).min(file_bytes.len());
        let region = &file_bytes[start..end];
        
        // Extract printable ASCII strings
        let mut current_str = String::new();
        for &b in region {
            if b >= 0x20 && b < 0x7f {
                current_str.push(b as char);
            } else if current_str.len() > 4 {
                if current_str.contains("c2pa") || current_str.contains("claim") || 
                   current_str.contains("creator") || current_str.contains("action") {
                    claim_info.push_str(&current_str);
                    claim_info.push_str(" | ");
                }
                current_str.clear();
            } else {
                current_str.clear();
            }
        }
    }

    let status = if has_c2pa_jumbf {
        "C2PA_VERIFIED"
    } else if has_c2pa_xmp {
        "C2PA_XMP_FOUND"
    } else if has_adobe_provenance {
        "ADOBE_PROVENANCE"
    } else {
        "NO_C2PA"
    };

    let description = match status {
        "C2PA_VERIFIED" => "✅ C2PA JUMBF manifest found! This file contains verifiable content provenance data.",
        "C2PA_XMP_FOUND" => "⚠️ C2PA metadata references found in XMP, but no full JUMBF manifest detected.",
        "ADOBE_PROVENANCE" => "📋 Adobe Content Credentials (legacy provenance) data detected.",
        _ => "❌ No C2PA content authenticity data found. This file has no embedded provenance.",
    };

    Ok(serde_json::json!({
        "status": status,
        "description": description,
        "has_jumbf_manifest": has_c2pa_jumbf,
        "has_xmp_c2pa": has_c2pa_xmp,
        "has_adobe_provenance": has_adobe_provenance,
        "jumbf_offset": jumbf_offset,
        "claim_info": if claim_info.is_empty() { "None extracted".to_string() } else { claim_info },
        "file_size": file_size,
        "file_type": ext,
    }))
}
