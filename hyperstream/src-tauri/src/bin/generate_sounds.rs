use std::fs::File;
use std::io::Write;

/// Generate a simple WAV file with a tone
/// This creates basic placeholder sounds for the audio events
fn generate_simple_wav(frequency: f32, duration_ms: u32) -> Vec<u8> {
    let sample_rate = 44100u32;
    let num_samples = (sample_rate * duration_ms) / 1000;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample / 8) as u32;
    let block_align = num_channels * (bits_per_sample / 8);
    
    let mut wav_data = Vec::new();
    
    // RIFF header
    wav_data.extend_from_slice(b"RIFF");
    let file_size = 36 + num_samples * (bits_per_sample / 8) as u32;
    wav_data.extend_from_slice(&file_size.to_le_bytes());
    wav_data.extend_from_slice(b"WAVE");
    
    // fmt chunk
    wav_data.extend_from_slice(b"fmt ");
    wav_data.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    wav_data.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
    wav_data.extend_from_slice(&num_channels.to_le_bytes());
    wav_data.extend_from_slice(&sample_rate.to_le_bytes());
    wav_data.extend_from_slice(&byte_rate.to_le_bytes());
    wav_data.extend_from_slice(&block_align.to_le_bytes());
    wav_data.extend_from_slice(&bits_per_sample.to_le_bytes());
    
    // data chunk
    wav_data.extend_from_slice(b"data");
    let data_size = num_samples * (bits_per_sample / 8) as u32;
    wav_data.extend_from_slice(&data_size.to_le_bytes());
    
    // Generate audio samples (simple sine wave)
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * frequency * 2.0 * std::f32::consts::PI).sin();
        let amplitude = (i16::MAX as f32 * 0.3 * sample) as i16; // 30% volume
        
        // Fade out at the end
        let fade_samples = sample_rate / 10; // 100ms fade
        let fade_factor = if i > num_samples - fade_samples {
            (num_samples - i) as f32 / fade_samples as f32
        } else {
            1.0
        };
        
        let final_amplitude = (amplitude as f32 * fade_factor) as i16;
        wav_data.extend_from_slice(&final_amplitude.to_le_bytes());
    }
    
    wav_data
}

fn main() {
    println!("Generating placeholder sound files...");
    
    // Success sound: ascending tone (440Hz to 880Hz)
    let success_wav = generate_simple_wav(660.0, 200);
    let mut file = File::create("assets/sounds/success.wav").expect("Failed to create success.wav");
    file.write_all(&success_wav).expect("Failed to write success.wav");
    println!("✓ Created success.wav");
    
    // Error sound: descending tone (440Hz to 220Hz)
    let error_wav = generate_simple_wav(330.0, 300);
    let mut file = File::create("assets/sounds/error.wav").expect("Failed to create error.wav");
    file.write_all(&error_wav).expect("Failed to write error.wav");
    println!("✓ Created error.wav");
    
    // Start sound: short beep (523Hz)
    let start_wav = generate_simple_wav(523.0, 100);
    let mut file = File::create("assets/sounds/start.wav").expect("Failed to create start.wav");
    file.write_all(&start_wav).expect("Failed to write start.wav");
    println!("✓ Created start.wav");
    
    println!("Sound files generated successfully!");
}
