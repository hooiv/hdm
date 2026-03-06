use serde::{Serialize, Deserialize};
use regex::Regex;

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

/// Parse a DASH MPD manifest into structured representations with segment URLs.
///
/// Supports:
///   - `<SegmentTemplate>` with `$Number$` / `$Time$` / `$Bandwidth$` / `$RepresentationID$`
///   - `<SegmentList>` with explicit `<SegmentURL>`
///   - `<BaseURL>` URL resolution
///   - Byte-range segments via `@range` attributes
///   - Period-level and AdaptationSet-level segment templates (inherited)
pub fn parse_mpd(content: &str, base_url: &str) -> Result<DashManifest, String> {
    let duration = extract_attribute(content, "mediaPresentationDuration")
        .and_then(|d| parse_duration(&d))
        .unwrap_or(0.0);

    let min_buffer = extract_attribute(content, "minBufferTime")
        .and_then(|d| parse_duration(&d))
        .unwrap_or(2.0);

    // Resolve base URL: <BaseURL> inside MPD overrides the manifest URL
    let mpd_base = extract_tag_content(content, "BaseURL")
        .map(|b| resolve_url(base_url, &b))
        .unwrap_or_else(|| base_url.to_string());

    let mut video_reps = Vec::new();
    let mut audio_reps = Vec::new();

    // Extract all <Period> blocks (or treat entire content as single period)
    let periods = extract_blocks(content, "Period");
    let period_blocks: Vec<&str> = if periods.is_empty() {
        vec![content]
    } else {
        periods.iter().map(|s| s.as_str()).collect()
    };

    for period in period_blocks {
        let period_base = extract_tag_content(period, "BaseURL")
            .map(|b| resolve_url(&mpd_base, &b))
            .unwrap_or_else(|| mpd_base.clone());

        // Period-level SegmentTemplate (inherited by AdaptationSets)
        let period_template = extract_segment_template(period);

        let adaptation_sets = extract_blocks(period, "AdaptationSet");
        for adapt_set in &adaptation_sets {
            let adapt_mime = extract_attribute(adapt_set, "mimeType")
                .or_else(|| extract_attribute(adapt_set, "contentType"))
                .unwrap_or_default();

            let adapt_base = extract_tag_content(adapt_set, "BaseURL")
                .map(|b| resolve_url(&period_base, &b))
                .unwrap_or_else(|| period_base.clone());

            // AdaptationSet-level template overrides Period-level
            let adapt_template = extract_segment_template(adapt_set)
                .or_else(|| period_template.clone());

            let representations = extract_blocks(adapt_set, "Representation");
            for rep_xml in &representations {
                let rep_id = extract_attribute(rep_xml, "id").unwrap_or_default();
                let bandwidth: u64 = extract_attribute(rep_xml, "bandwidth")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let width: Option<u32> = extract_attribute(rep_xml, "width")
                    .and_then(|v| v.parse().ok());
                let height: Option<u32> = extract_attribute(rep_xml, "height")
                    .and_then(|v| v.parse().ok());
                let codecs = extract_attribute(rep_xml, "codecs");
                let rep_mime = extract_attribute(rep_xml, "mimeType")
                    .unwrap_or_else(|| adapt_mime.clone());

                let rep_base = extract_tag_content(rep_xml, "BaseURL")
                    .map(|b| resolve_url(&adapt_base, &b))
                    .unwrap_or_else(|| adapt_base.clone());

                // Representation-level template overrides AdaptationSet-level
                let template = extract_segment_template(rep_xml)
                    .or_else(|| adapt_template.clone());

                let segments = if let Some(tmpl) = template {
                    build_segments_from_template(&tmpl, &rep_base, &rep_id, bandwidth, duration)
                } else {
                    // Try <SegmentList>
                    extract_segment_list(rep_xml, &rep_base)
                };

                let rep = DashRepresentation {
                    id: rep_id,
                    bandwidth,
                    width,
                    height,
                    codecs,
                    mime_type: rep_mime.clone(),
                    segments,
                };

                let is_video = rep_mime.contains("video")
                    || adapt_mime.contains("video")
                    || width.is_some();
                let is_audio = rep_mime.contains("audio") || adapt_mime.contains("audio");

                if is_video {
                    video_reps.push(rep);
                } else if is_audio {
                    audio_reps.push(rep);
                } else {
                    // Default: treat as video if has width, else audio
                    if width.is_some() {
                        video_reps.push(rep);
                    } else {
                        audio_reps.push(rep);
                    }
                }
            }
        }
    }

    // Sort by bandwidth descending (highest quality first)
    video_reps.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    audio_reps.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));

    Ok(DashManifest {
        duration,
        min_buffer_time: min_buffer,
        video_representations: video_reps,
        audio_representations: audio_reps,
    })
}

// ── SegmentTemplate handling ─────────────────────────────────────────

#[derive(Debug, Clone)]
struct SegmentTemplate {
    media: String,
    initialization: Option<String>,
    start_number: u64,
    timescale: u64,
    duration: Option<u64>,
    /// SegmentTimeline <S> entries: (t, d, r) tuples
    timeline: Vec<(Option<u64>, u64, i64)>,
}

fn extract_segment_template(xml: &str) -> Option<SegmentTemplate> {
    // Only match the SegmentTemplate at this level (not nested ones)
    let template_block = extract_block_first(xml, "SegmentTemplate")?;

    let media = extract_attribute(&template_block, "media")?;
    let initialization = extract_attribute(&template_block, "initialization");
    let start_number: u64 = extract_attribute(&template_block, "startNumber")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let timescale: u64 = extract_attribute(&template_block, "timescale")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let duration: Option<u64> = extract_attribute(&template_block, "duration")
        .and_then(|v| v.parse().ok());

    // Parse <SegmentTimeline>
    let mut timeline = Vec::new();
    if let Some(tl_block) = extract_block_first(&template_block, "SegmentTimeline") {
        let s_re = Regex::new(r#"<S\s[^>]*/?>"#).unwrap();
        for cap in s_re.find_iter(&tl_block) {
            let s_tag = cap.as_str();
            let t: Option<u64> = extract_attribute(s_tag, "t").and_then(|v| v.parse().ok());
            let d: u64 = extract_attribute(s_tag, "d").and_then(|v| v.parse().ok()).unwrap_or(0);
            let r: i64 = extract_attribute(s_tag, "r").and_then(|v| v.parse().ok()).unwrap_or(0);
            timeline.push((t, d, r));
        }
    }

    Some(SegmentTemplate {
        media,
        initialization,
        start_number,
        timescale,
        duration,
        timeline,
    })
}

fn build_segments_from_template(
    tmpl: &SegmentTemplate,
    base_url: &str,
    rep_id: &str,
    bandwidth: u64,
    total_duration: f64,
) -> Vec<DashSegment> {
    let mut segments = Vec::new();
    let timescale = tmpl.timescale.max(1) as f64;

    // Add initialization segment if present
    if let Some(ref init_pattern) = tmpl.initialization {
        let init_url = substitute_template(init_pattern, rep_id, bandwidth, 0, 0);
        segments.push(DashSegment {
            url: resolve_url(base_url, &init_url),
            start_time: 0.0,
            duration: 0.0,
            byte_range: None,
        });
    }

    if !tmpl.timeline.is_empty() {
        // SegmentTimeline mode
        let mut number = tmpl.start_number;
        let mut time: u64 = 0;

        for &(t, d, r) in &tmpl.timeline {
            if let Some(explicit_t) = t {
                time = explicit_t;
            }
            let repeat_count = if r >= 0 { r as u64 } else { 0 };

            for _ in 0..=repeat_count {
                let url_str = substitute_template(&tmpl.media, rep_id, bandwidth, number, time);
                segments.push(DashSegment {
                    url: resolve_url(base_url, &url_str),
                    start_time: time as f64 / timescale,
                    duration: d as f64 / timescale,
                    byte_range: None,
                });
                time += d;
                number += 1;
            }
        }
    } else if let Some(seg_duration) = tmpl.duration {
        // Fixed-duration segments: calculate count from total duration
        let segment_duration_secs = seg_duration as f64 / timescale;
        let segment_count = if total_duration > 0.0 {
            (total_duration / segment_duration_secs).ceil() as u64
        } else {
            // Fallback: assume 2 hours max if duration unknown
            (7200.0 / segment_duration_secs).ceil() as u64
        };
        // Cap to prevent runaway allocation
        let segment_count = segment_count.min(50000);

        for i in 0..segment_count {
            let number = tmpl.start_number + i;
            let time = i * seg_duration;
            let url_str = substitute_template(&tmpl.media, rep_id, bandwidth, number, time);
            segments.push(DashSegment {
                url: resolve_url(base_url, &url_str),
                start_time: time as f64 / timescale,
                duration: segment_duration_secs,
                byte_range: None,
            });
        }
    }

    segments
}

fn substitute_template(pattern: &str, rep_id: &str, bandwidth: u64, number: u64, time: u64) -> String {
    pattern
        .replace("$RepresentationID$", rep_id)
        .replace("$Bandwidth$", &bandwidth.to_string())
        .replace("$Number$", &number.to_string())
        .replace("$Time$", &time.to_string())
}

// ── SegmentList handling ─────────────────────────────────────────────

fn extract_segment_list(xml: &str, base_url: &str) -> Vec<DashSegment> {
    let mut segments = Vec::new();

    if let Some(list_block) = extract_block_first(xml, "SegmentList") {
        // Look for Initialization element
        if let Some(init_url) = extract_attribute(&list_block, "initialization") {
            // initialization can be an attribute of SegmentList
            segments.push(DashSegment {
                url: resolve_url(base_url, &init_url),
                start_time: 0.0,
                duration: 0.0,
                byte_range: None,
            });
        }
        // Or as a child element <Initialization sourceURL="..."/>
        let init_re = Regex::new(r#"<Initialization\s[^>]*/?>"#).unwrap();
        for cap in init_re.find_iter(&list_block) {
            if let Some(src) = extract_attribute(cap.as_str(), "sourceURL") {
                let byte_range = extract_attribute(cap.as_str(), "range")
                    .and_then(|r| parse_byte_range(&r));
                segments.push(DashSegment {
                    url: resolve_url(base_url, &src),
                    start_time: 0.0,
                    duration: 0.0,
                    byte_range,
                });
            }
        }

        // Extract <SegmentURL> entries
        let seg_re = Regex::new(r#"<SegmentURL\s[^>]*/?>"#).unwrap();
        let mut time_offset = 0.0;
        let seg_duration: f64 = extract_attribute(&list_block, "duration")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let timescale: f64 = extract_attribute(&list_block, "timescale")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0)
            .max(1.0);

        for cap in seg_re.find_iter(&list_block) {
            let tag = cap.as_str();
            if let Some(media_url) = extract_attribute(tag, "media") {
                let byte_range = extract_attribute(tag, "mediaRange")
                    .and_then(|r| parse_byte_range(&r));
                let dur = if seg_duration > 0.0 { seg_duration / timescale } else { 0.0 };
                segments.push(DashSegment {
                    url: resolve_url(base_url, &media_url),
                    start_time: time_offset,
                    duration: dur,
                    byte_range,
                });
                time_offset += dur;
            }
        }
    }

    segments
}

fn parse_byte_range(range: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() == 2 {
        let start = parts[0].parse().ok()?;
        let end = parts[1].parse().ok()?;
        Some((start, end))
    } else {
        None
    }
}

// ── XML helpers ──────────────────────────────────────────────────────

/// Extract an XML attribute value (handles both single and double quotes)
fn extract_attribute(content: &str, attr: &str) -> Option<String> {
    // Try double quotes
    let pattern = format!("{}=\"", attr);
    if let Some(start) = content.find(&pattern) {
        let start = start + pattern.len();
        if let Some(end) = content[start..].find('"') {
            return Some(content[start..start + end].to_string());
        }
    }
    // Try single quotes
    let pattern = format!("{}='", attr);
    if let Some(start) = content.find(&pattern) {
        let start = start + pattern.len();
        if let Some(end) = content[start..].find('\'') {
            return Some(content[start..start + end].to_string());
        }
    }
    None
}

/// Extract the text content of a simple XML tag (e.g., <BaseURL>http://...</BaseURL>)
fn extract_tag_content(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    if let Some(start_pos) = content.find(&open) {
        // Find the end of the opening tag
        let after_open = start_pos + open.len();
        if let Some(gt) = content[after_open..].find('>') {
            let content_start = after_open + gt + 1;
            if let Some(end_pos) = content[content_start..].find(&close) {
                let text = content[content_start..content_start + end_pos].trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}

/// Extract all blocks of a specific XML element (e.g., all <Representation>...</Representation>)
fn extract_blocks(content: &str, tag: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);

    let mut search_from = 0;
    while let Some(start) = content[search_from..].find(&open) {
        let abs_start = search_from + start;

        // Check for self-closing tag
        let after_tag = abs_start + open.len();
        if let Some(gt_offset) = content[after_tag..].find('>') {
            let gt_pos = after_tag + gt_offset;
            if content.as_bytes().get(gt_pos.saturating_sub(1)) == Some(&b'/') {
                // Self-closing: <Tag ... />
                blocks.push(content[abs_start..=gt_pos].to_string());
                search_from = gt_pos + 1;
                continue;
            }
        }

        if let Some(end) = content[abs_start..].find(&close) {
            let block_end = abs_start + end + close.len();
            blocks.push(content[abs_start..block_end].to_string());
            search_from = block_end;
        } else {
            search_from = abs_start + open.len();
        }
    }
    blocks
}

/// Extract the first block of a specific XML element
fn extract_block_first(content: &str, tag: &str) -> Option<String> {
    extract_blocks(content, tag).into_iter().next()
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if relative.starts_with('/') {
        // Absolute path — combine with scheme+host of base
        if let Some(pos) = base.find("://") {
            if let Some(slash) = base[pos + 3..].find('/') {
                return format!("{}{}", &base[..pos + 3 + slash], relative);
            }
        }
        return format!("{}{}", base.trim_end_matches('/'), relative);
    }
    // Relative path — combine with base directory
    let base_dir = if let Some(pos) = base.rfind('/') {
        &base[..=pos]
    } else {
        base
    };
    format!("{}{}", base_dir, relative)
}

/// Parse ISO 8601 duration (PT1H2M3.4S, also handles P1DT2H3M4S)
fn parse_duration(duration: &str) -> Option<f64> {
    if !duration.starts_with('P') {
        return None;
    }

    let mut total_seconds = 0.0;
    let mut num_str = String::new();
    let mut in_time = false;

    for c in duration[1..].chars() {
        if c == 'T' {
            in_time = true;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            num_str.push(c);
        } else {
            if let Ok(n) = num_str.parse::<f64>() {
                match (c, in_time) {
                    ('Y', false) => total_seconds += n * 365.25 * 86400.0,
                    ('M', false) => total_seconds += n * 30.0 * 86400.0,
                    ('D', false) => total_seconds += n * 86400.0,
                    ('H', true) => total_seconds += n * 3600.0,
                    ('M', true) => total_seconds += n * 60.0,
                    ('S', true) => total_seconds += n,
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

    #[test]
    fn test_parse_duration_with_days() {
        let d = parse_duration("P1DT2H30M").unwrap();
        assert!((d - (86400.0 + 9000.0)).abs() < 0.01);
    }

    #[test]
    fn test_resolve_url_absolute() {
        assert_eq!(
            resolve_url("https://cdn.example.com/video/", "https://other.com/file.mp4"),
            "https://other.com/file.mp4"
        );
    }

    #[test]
    fn test_resolve_url_relative() {
        assert_eq!(
            resolve_url("https://cdn.example.com/video/manifest.mpd", "segment_001.m4s"),
            "https://cdn.example.com/video/segment_001.m4s"
        );
    }

    #[test]
    fn test_resolve_url_absolute_path() {
        assert_eq!(
            resolve_url("https://cdn.example.com/video/manifest.mpd", "/media/seg1.m4s"),
            "https://cdn.example.com/media/seg1.m4s"
        );
    }

    #[test]
    fn test_extract_attribute() {
        assert_eq!(
            extract_attribute(r#"<Tag bandwidth="128000" id="1">"#, "bandwidth"),
            Some("128000".to_string())
        );
    }

    #[test]
    fn test_substitute_template() {
        let result = substitute_template(
            "video_$RepresentationID$_$Number$.m4s",
            "720p",
            5000000,
            42,
            0,
        );
        assert_eq!(result, "video_720p_42.m4s");
    }

    #[test]
    fn test_parse_mpd_with_segment_template() {
        let mpd = r#"<?xml version="1.0"?>
<MPD mediaPresentationDuration="PT10S" minBufferTime="PT2S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="seg_$Number$.m4s" initialization="init.m4s" startNumber="1" duration="2000" timescale="1000"/>
      <Representation id="720p" bandwidth="5000000" width="1280" height="720" codecs="avc1.64001f"/>
    </AdaptationSet>
    <AdaptationSet mimeType="audio/mp4">
      <SegmentTemplate media="audio_$Number$.m4s" initialization="audio_init.m4s" startNumber="1" duration="2000" timescale="1000"/>
      <Representation id="audio" bandwidth="128000" codecs="mp4a.40.2"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

        let manifest = parse_mpd(mpd, "https://cdn.example.com/video/manifest.mpd").unwrap();
        assert!((manifest.duration - 10.0).abs() < 0.01);
        assert_eq!(manifest.video_representations.len(), 1);
        assert_eq!(manifest.audio_representations.len(), 1);

        let video = &manifest.video_representations[0];
        assert_eq!(video.id, "720p");
        assert_eq!(video.bandwidth, 5000000);
        assert_eq!(video.width, Some(1280));
        assert_eq!(video.height, Some(720));
        // init + 5 segments (10s / 2s each)
        assert_eq!(video.segments.len(), 6);
        assert!(video.segments[0].url.contains("init.m4s"));
        assert!(video.segments[1].url.contains("seg_1.m4s"));
    }

    #[test]
    fn test_parse_mpd_with_segment_timeline() {
        let mpd = r#"<?xml version="1.0"?>
<MPD mediaPresentationDuration="PT6S" minBufferTime="PT1S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="chunk_$Time$.m4s" initialization="init.m4s" timescale="1000">
        <SegmentTimeline>
          <S t="0" d="2000" r="2"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="1" bandwidth="3000000" width="1920" height="1080"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

        let manifest = parse_mpd(mpd, "https://cdn.example.com/").unwrap();
        let video = &manifest.video_representations[0];
        // init + 3 segments (r=2 means repeat 2 additional times = 3 total)
        assert_eq!(video.segments.len(), 4);
        assert!(video.segments[0].url.contains("init.m4s"));
        assert!(video.segments[1].url.contains("chunk_0.m4s"));
        assert!(video.segments[2].url.contains("chunk_2000.m4s"));
        assert!(video.segments[3].url.contains("chunk_4000.m4s"));
    }

    #[test]
    fn test_parse_byte_range() {
        assert_eq!(parse_byte_range("0-1024"), Some((0, 1024)));
        assert_eq!(parse_byte_range("1025-2048"), Some((1025, 2048)));
        assert_eq!(parse_byte_range("invalid"), None);
    }
}
