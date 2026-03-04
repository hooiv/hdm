use serde::{Serialize, Deserialize};

/// A segment for DASH streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashSegment {
    pub url: String,
    pub start_time: f64,
    pub duration: f64,
    pub byte_range: Option<(u64, u64)>,
}

/// A representation (quality level) in DASH
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashRepresentation {
    pub id: String,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codecs: Option<String>,
    pub mime_type: String,
    pub segments: Vec<DashSegment>,
}

/// A DASH manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashManifest {
    pub duration: f64,
    pub min_buffer_time: f64,
    pub video_representations: Vec<DashRepresentation>,
    pub audio_representations: Vec<DashRepresentation>,
}

/// Parse a simple DASH MPD manifest
pub fn parse_mpd(content: &str, _base_url: &str) -> Result<DashManifest, String> {
    // This is a simplified parser - a full implementation would use an XML parser
    // For now, we extract key information using basic string parsing
    
    let duration = extract_attribute(content, "mediaPresentationDuration")
        .and_then(|d| parse_duration(&d))
        .unwrap_or(0.0);
    
    let min_buffer = extract_attribute(content, "minBufferTime")
        .and_then(|d| parse_duration(&d))
        .unwrap_or(2.0);

    // TODO: Implement full AdaptationSet/Representation XML parsing
    // Currently only extracts top-level manifest attributes.
    eprintln!("[DASH] Warning: MPD parser is incomplete — video/audio representations are not yet extracted");

    Ok(DashManifest {
        duration,
        min_buffer_time: min_buffer,
        video_representations: Vec::new(),
        audio_representations: Vec::new(),
    })
}

/// Extract an XML attribute value
fn extract_attribute(content: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    if let Some(start) = content.find(&pattern) {
        let start = start + pattern.len();
        if let Some(end) = content[start..].find('"') {
            return Some(content[start..start + end].to_string());
        }
    }
    None
}

/// Parse ISO 8601 duration (PT1H2M3.4S)
fn parse_duration(duration: &str) -> Option<f64> {
    if !duration.starts_with("PT") {
        return None;
    }
    
    let duration = &duration[2..]; // Remove "PT"
    let mut total_seconds = 0.0;
    let mut num_str = String::new();
    
    for c in duration.chars() {
        if c.is_ascii_digit() || c == '.' {
            num_str.push(c);
        } else {
            if let Ok(n) = num_str.parse::<f64>() {
                match c {
                    'H' => total_seconds += n * 3600.0,
                    'M' => total_seconds += n * 60.0,
                    'S' => total_seconds += n,
                    _ => {}
                }
            }
            num_str.clear();
        }
    }
    
    Some(total_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("PT1H"), Some(3600.0));
        assert_eq!(parse_duration("PT1M30S"), Some(90.0));
        assert_eq!(parse_duration("PT2H30M15S"), Some(9015.0));
        assert_eq!(parse_duration("PT0.5S"), Some(0.5));
    }
}
