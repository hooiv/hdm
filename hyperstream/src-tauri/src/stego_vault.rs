use std::path::Path;
use tokio::fs;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

/// LSB Steganography: Hide secret data inside a PNG image by modifying
/// the least significant bit of **decompressed** pixel data in IDAT chunks.
/// The first 32 bits store the message length, followed by the message bytes.
///
/// Correct approach: decompress IDAT → modify pixel LSBs → recompress → update CRC.
pub async fn stego_hide(image_path: String, secret_data: String) -> Result<serde_json::Value, String> {
    // Validate path is within download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon_path = dunce::canonicalize(&image_path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    if !canon_path.starts_with(&download_dir) {
        return Err("Image must be within the download directory".to_string());
    }

    let path = Path::new(&image_path);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_path));
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "png" {
        return Err("Steganography only supports PNG files (lossless format required).".to_string());
    }

    // Cap file size to prevent OOM when loading image into memory
    let file_size = fs::metadata(path).await.map_err(|e| format!("Metadata error: {}", e))?.len();
    if file_size > 100 * 1024 * 1024 {
        return Err(format!("Image too large: {} bytes (max 100 MB)", file_size));
    }

    let img_bytes = fs::read(path).await.map_err(|e| format!("Read error: {}", e))?;

    // Find the IDAT chunk(s) - PNG pixel data
    let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if img_bytes.len() < 8 || img_bytes[..8] != png_sig {
        return Err("Not a valid PNG file.".to_string());
    }

    let secret_bytes = secret_data.as_bytes();
    let msg_len = secret_bytes.len();
    let bits_needed = 32 + (msg_len * 8);

    // Collect all IDAT chunk data and their positions
    let (idat_chunks, other_chunks) = parse_png_chunks(&img_bytes)?;
    if idat_chunks.is_empty() {
        return Err("No IDAT chunk found in PNG".to_string());
    }

    // Concatenate all IDAT data and decompress
    let compressed: Vec<u8> = idat_chunks.iter().flat_map(|c| c.data.clone()).collect();
    let mut decompressed = Vec::new();
    {
        let mut decoder = ZlibDecoder::new(&compressed[..]);
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| format!("Failed to decompress IDAT: {}", e))?;
    }

    // Check capacity
    let available_bits = decompressed.len() * 8;
    if bits_needed > available_bits {
        return Err(format!(
            "Message too long! Need {} bits, image has {} bits ({} bytes max message).",
            bits_needed, available_bits, (available_bits - 32) / 8
        ));
    }

    // Encode message length (32 bits) into decompressed pixel data LSBs
    let len_bytes = (msg_len as u32).to_be_bytes();
    let mut bit_idx = 0;

    for &byte in &len_bytes {
        for bit_pos in (0..8).rev() {
            let bit = (byte >> bit_pos) & 1;
            decompressed[bit_idx] = (decompressed[bit_idx] & 0xFE) | bit;
            bit_idx += 1;
        }
    }

    // Encode secret message
    for &byte in secret_bytes {
        for bit_pos in (0..8).rev() {
            let bit = (byte >> bit_pos) & 1;
            decompressed[bit_idx] = (decompressed[bit_idx] & 0xFE) | bit;
            bit_idx += 1;
        }
    }

    // Recompress the modified decompressed data
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&decompressed)
        .map_err(|e| format!("Failed to compress IDAT: {}", e))?;
    let new_compressed = encoder.finish()
        .map_err(|e| format!("Failed to finish compression: {}", e))?;

    // Rebuild the PNG file
    let mut output = Vec::new();
    output.extend_from_slice(&png_sig);

    // Write chunks before IDAT
    for chunk in &other_chunks {
        if chunk.position == ChunkPosition::BeforeIdat {
            write_chunk(&mut output, &chunk.chunk_type, &chunk.data);
        }
    }

    // Write new single IDAT chunk with recompressed data
    write_chunk(&mut output, b"IDAT", &new_compressed);

    // Write chunks after IDAT (including IEND)
    for chunk in &other_chunks {
        if chunk.position == ChunkPosition::AfterIdat {
            write_chunk(&mut output, &chunk.chunk_type, &chunk.data);
        }
    }

    let output_path = format!("{}.stego.png", image_path.trim_end_matches(".png"));
    fs::write(&output_path, &output).await.map_err(|e| format!("Write error: {}", e))?;

    Ok(serde_json::json!({
        "status": "hidden",
        "output_path": output_path,
        "message_length": msg_len,
        "bits_used": bits_needed,
        "capacity_bits": available_bits,
    }))
}

/// Extract hidden data from a steganographic PNG image.
pub async fn stego_extract(image_path: String) -> Result<serde_json::Value, String> {
    // Validate path is within download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon_path = dunce::canonicalize(&image_path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    if !canon_path.starts_with(&download_dir) {
        return Err("Image must be within the download directory".to_string());
    }

    let path = Path::new(&image_path);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_path));
    }

    let img_bytes = fs::read(path).await.map_err(|e| format!("Read error: {}", e))?;

    let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if img_bytes.len() < 8 || img_bytes[..8] != png_sig {
        return Err("Not a valid PNG file.".to_string());
    }

    // Collect and decompress IDAT data
    let (idat_chunks, _) = parse_png_chunks(&img_bytes)?;
    if idat_chunks.is_empty() {
        return Err("No IDAT chunk found".to_string());
    }

    let compressed: Vec<u8> = idat_chunks.iter().flat_map(|c| c.data.clone()).collect();
    let mut decompressed = Vec::new();
    {
        let mut decoder = ZlibDecoder::new(&compressed[..]);
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| format!("Failed to decompress IDAT: {}", e))?;
    }

    if decompressed.len() < 4 {
        return Err("Image too small for steganographic data".to_string());
    }

    // Read message length (first 32 bits from decompressed pixel data)
    let mut len_bits = [0u8; 4];
    let mut bit_idx = 0;
    for byte in &mut len_bits {
        let mut val = 0u8;
        for bit_pos in (0..8).rev() {
            let bit = decompressed[bit_idx] & 1;
            val |= bit << bit_pos;
            bit_idx += 1;
        }
        *byte = val;
    }

    let msg_len = u32::from_be_bytes(len_bits) as usize;

    if msg_len == 0 || msg_len > 1_000_000 {
        return Err("No valid steganographic data found (invalid length marker).".to_string());
    }

    let bits_needed = 32 + (msg_len * 8);
    if bits_needed > decompressed.len() * 8 {
        return Err("Message length exceeds image capacity".to_string());
    }

    // Read message bytes
    let mut message_bytes = vec![0u8; msg_len];
    for byte in &mut message_bytes {
        let mut val = 0u8;
        for bit_pos in (0..8).rev() {
            let bit = decompressed[bit_idx] & 1;
            val |= bit << bit_pos;
            bit_idx += 1;
        }
        *byte = val;
    }

    let message = String::from_utf8(message_bytes)
        .map_err(|_| "Extracted data is not valid UTF-8. The image may not contain hidden text.".to_string())?;

    Ok(serde_json::json!({
        "status": "extracted",
        "message": message,
        "message_length": msg_len,
        "bits_read": bit_idx,
    }))
}

// --- PNG chunk helpers ---

#[derive(Clone, PartialEq)]
enum ChunkPosition {
    BeforeIdat,
    AfterIdat,
}

struct PngChunk {
    chunk_type: [u8; 4],
    data: Vec<u8>,
    position: ChunkPosition,
}

fn parse_png_chunks(img_bytes: &[u8]) -> Result<(Vec<PngChunk>, Vec<PngChunk>), String> {
    let mut idat_chunks = Vec::new();
    let mut other_chunks = Vec::new();
    let mut pos = 8usize; // skip PNG signature
    let mut found_idat = false;

    while pos + 8 <= img_bytes.len() {
        if pos + 4 > img_bytes.len() { break; }
        let chunk_len = u32::from_be_bytes([
            img_bytes[pos], img_bytes[pos+1], img_bytes[pos+2], img_bytes[pos+3]
        ]) as usize;

        if pos + 8 + chunk_len + 4 > img_bytes.len() {
            return Err("Truncated PNG chunk".to_string());
        }

        let mut chunk_type = [0u8; 4];
        chunk_type.copy_from_slice(&img_bytes[pos+4..pos+8]);
        let data = img_bytes[pos+8..pos+8+chunk_len].to_vec();

        if &chunk_type == b"IDAT" {
            found_idat = true;
            idat_chunks.push(PngChunk {
                chunk_type,
                data,
                position: ChunkPosition::BeforeIdat,
            });
        } else {
            other_chunks.push(PngChunk {
                chunk_type,
                data,
                position: if found_idat { ChunkPosition::AfterIdat } else { ChunkPosition::BeforeIdat },
            });
        }

        pos += 12 + chunk_len;
    }

    Ok((idat_chunks, other_chunks))
}

fn write_chunk(output: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(chunk_type);
    output.extend_from_slice(data);

    // Calculate CRC over chunk_type + data
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    let crc = png_crc32(&crc_data);
    output.extend_from_slice(&crc.to_be_bytes());
}

fn png_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}
