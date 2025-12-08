use aes::Aes128;
use cbc::Decryptor;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use block_padding::Pkcs7;

type Aes128Cbc = Decryptor<Aes128>;

pub fn decrypt_aes128(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != 16 {
        return Err(format!("Key must be 16 bytes (got {})", key.len()));
    }
    if iv.len() != 16 {
        return Err(format!("IV must be 16 bytes (got {})", iv.len()));
    }

    let decryptor = Aes128Cbc::new(key.into(), iv.into());
    
    // We need to copy data because decrypt works in-place or consumes buffer
    let mut buffer = data.to_vec();
    
    // Decrypt in-place
    let len = decryptor.decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|e| format!("Decryption failed (padding error?): {:?}", e))?
        .len();
        
    // Truncate to actual size (remove padding)
    buffer.truncate(len);
    
    Ok(buffer)
}

pub fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    hex::decode(s).map_err(|e| e.to_string())
}
