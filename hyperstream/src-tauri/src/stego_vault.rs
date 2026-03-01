use std::path::Path;
use tokio::fs;

/// LSB Steganography: Hide secret data inside a PNG image by modifying
/// the least significant bit of each color channel.
/// The first 32 bits store the message length, followed by the message bytes.
pub async fn stego_hide(image_path: String, secret_data: String) -> Result<serde_json::Value, String> {
    let path = Path::new(&image_path);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_path));
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "png" {
        return Err("Steganography only supports PNG files (lossless format required).".to_string());
    }

    let img_bytes = fs::read(path).await.map_err(|e| format!("Read error: {}", e))?;

    // Find the IDAT chunk(s) - PNG pixel data
    // PNG structure: 8-byte signature, then chunks: [4-byte length][4-byte type][data][4-byte CRC]
    let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if img_bytes.len() < 8 || img_bytes[..8] != png_sig {
        return Err("Not a valid PNG file.".to_string());
    }

    let secret_bytes = secret_data.as_bytes();
    let msg_len = secret_bytes.len();

    // We need: 32 bits for length + 8 bits per byte of message
    let bits_needed = 32 + (msg_len * 8);

    // Encode in a simple way: modify raw file bytes after IDAT header
    // Find first IDAT chunk
    let mut pos = 8usize;
    let mut idat_data_start: Option<usize> = None;
    let mut idat_data_len: usize = 0;

    while pos + 8 < img_bytes.len() {
        let chunk_len = u32::from_be_bytes([
            img_bytes[pos], img_bytes[pos+1], img_bytes[pos+2], img_bytes[pos+3]
        ]) as usize;
        let chunk_type = &img_bytes[pos+4..pos+8];

        if chunk_type == b"IDAT" {
            idat_data_start = Some(pos + 8); // Start of IDAT data
            idat_data_len = chunk_len;
            break;
        }

        pos += 12 + chunk_len; // 4 len + 4 type + data + 4 CRC
    }

    let idat_start = idat_data_start.ok_or("No IDAT chunk found in PNG")?;

    // Check capacity
    let available_bits = idat_data_len * 8;
    if bits_needed > available_bits {
        return Err(format!(
            "Message too long! Need {} bits, image IDAT has {} bits ({} bytes max message).",
            bits_needed, available_bits, (available_bits - 32) / 8
        ));
    }

    let mut modified = img_bytes.clone();

    // Encode message length (32 bits)
    let len_bytes = (msg_len as u32).to_be_bytes();
    let mut bit_idx = 0;

    for &byte in &len_bytes {
        for bit_pos in (0..8).rev() {
            let bit = (byte >> bit_pos) & 1;
            let file_pos = idat_start + bit_idx;
            modified[file_pos] = (modified[file_pos] & 0xFE) | bit;
            bit_idx += 1;
        }
    }

    // Encode secret message
    for &byte in secret_bytes {
        for bit_pos in (0..8).rev() {
            let bit = (byte >> bit_pos) & 1;
            let file_pos = idat_start + bit_idx;
            modified[file_pos] = (modified[file_pos] & 0xFE) | bit;
            bit_idx += 1;
        }
    }

    // Save as new file
    let output_path = format!("{}.stego.png", image_path.trim_end_matches(".png"));
    fs::write(&output_path, &modified).await.map_err(|e| format!("Write error: {}", e))?;

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
    let path = Path::new(&image_path);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_path));
    }

    let img_bytes = fs::read(path).await.map_err(|e| format!("Read error: {}", e))?;

    let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if img_bytes.len() < 8 || img_bytes[..8] != png_sig {
        return Err("Not a valid PNG file.".to_string());
    }

    // Find IDAT
    let mut pos = 8usize;
    let mut idat_data_start: Option<usize> = None;

    while pos + 8 < img_bytes.len() {
        let chunk_len = u32::from_be_bytes([
            img_bytes[pos], img_bytes[pos+1], img_bytes[pos+2], img_bytes[pos+3]
        ]) as usize;
        let chunk_type = &img_bytes[pos+4..pos+8];

        if chunk_type == b"IDAT" {
            idat_data_start = Some(pos + 8);
            break;
        }
        pos += 12 + chunk_len;
    }

    let idat_start = idat_data_start.ok_or("No IDAT chunk found")?;

    // Read message length (first 32 bits)
    let mut len_bits = [0u8; 4];
    let mut bit_idx = 0;
    for byte in &mut len_bits {
        let mut val = 0u8;
        for bit_pos in (0..8).rev() {
            let bit = img_bytes[idat_start + bit_idx] & 1;
            val |= bit << bit_pos;
            bit_idx += 1;
        }
        *byte = val;
    }

    let msg_len = u32::from_be_bytes(len_bits) as usize;

    if msg_len == 0 || msg_len > 1_000_000 {
        return Err("No valid steganographic data found (invalid length marker).".to_string());
    }

    // Read message bytes
    let mut message_bytes = vec![0u8; msg_len];
    for byte in &mut message_bytes {
        let mut val = 0u8;
        for bit_pos in (0..8).rev() {
            if idat_start + bit_idx >= img_bytes.len() {
                return Err("Message extends beyond image data".to_string());
            }
            let bit = img_bytes[idat_start + bit_idx] & 1;
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
