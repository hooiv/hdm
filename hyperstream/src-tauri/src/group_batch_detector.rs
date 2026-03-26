/// Intelligent Batch Auto-Detection for Download Groups
///
/// Analyzes URLs to identify batches that should be grouped together:
/// - Gallery URLs (same domain, sequential filenames)
/// - Chapter sequences (manga, books, documents)
/// - Album downloads (images from same metadata)
/// - File listings (directories with multiple files)
///
/// Provides confidence scores and suggested grouping strategies.

use regex::Regex;
use std::collections::HashMap;
use url::Url;

/// Confidence score (0.0 to 1.0)
pub type ConfidenceScore = f64;

/// Detected batch pattern
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchPattern {
    /// Sequential numbered files: file_001.jpg, file_002.jpg, ...
    SequentialNumeric,
    /// Alphabetical sequence: page_a.pdf, page_b.pdf, ...
    SequentialAlpha,
    /// Same directory with multiple extensions
    DirectoryBatch,
    /// Image gallery pattern
    ImageGallery,
    /// Document series (chapters, volumes)
    DocumentSeries,
    /// Archive parts (.zip.001, .zip.002)
    ArchiveParts,
    /// Streaming segments (.m3u8 playlist)
    StreamSegments,
    /// Generic batch (domain + similar filename stem)
    Generic,
}

/// Information about a detected batch
#[derive(Debug, Clone)]
pub struct BatchDetection {
    /// Type of batch detected
    pub pattern: BatchPattern,
    /// Confidence (0.0 to 1.0)
    pub confidence: ConfidenceScore,
    /// Common URL prefix
    pub url_prefix: String,
    /// Variably changing part
    pub variable_part: String,
    /// Suggested name for group
    pub suggested_group_name: String,
    /// Count of similar URLs in batch
    pub estimated_count: usize,
    /// Suggested execution strategy
    pub suggested_strategy: String,
    /// Detailed reason
    pub reason: String,
}

/// URL batch analyzer
pub struct BatchDetector;

impl BatchDetector {
    /// Analyze a set of URLs to detect batch patterns
    pub fn detect_batch(urls: &[&str]) -> Option<BatchDetection> {
        if urls.len() < 2 {
            return None;
        }

        // Try each pattern in order of specificity
        if let Some(detection) = Self::detect_sequential_numeric(urls) {
            return Some(detection);
        }

        if let Some(detection) = Self::detect_archive_parts(urls) {
            return Some(detection);
        }

        if let Some(detection) = Self::detect_image_gallery(urls) {
            return Some(detection);
        }

        if let Some(detection) = Self::detect_directory_batch(urls) {
            return Some(detection);
        }

        if let Some(detection) = Self::detect_generic_batch(urls) {
            return Some(detection);
        }

        None
    }

    /// Detect sequential numbered files (file_001.jpg, file_002.jpg)
    fn detect_sequential_numeric(urls: &[&str]) -> Option<BatchDetection> {
        let numeric_re = Regex::new(r"(\d{2,})").ok()?;
        let mut found_numbers = Vec::new();

        for url in urls {
            if let Some(caps) = numeric_re.find(url) {
                found_numbers.push(caps.as_str().to_string());
            }
        }

        if found_numbers.len() != urls.len() {
            return None;
        }

        // Check if numbers are sequential
        let mut numbers: Vec<u32> = found_numbers
            .iter()
            .filter_map(|n| n.parse().ok())
            .collect();

        if numbers.is_empty() || numbers.len() != urls.len() {
            return None;
        }

        numbers.sort();
        let mut is_sequential = true;
        for i in 1..numbers.len() {
            if numbers[i] != numbers[i - 1] + 1 {
                is_sequential = false;
                break;
            }
        }

        if !is_sequential && numbers.len() < 5 {
            return None; // Not sequential enough
        }

        // Extract common prefix
        let common_prefix = Self::find_common_prefix(urls);
        let extension = Self::extract_extension(urls[0]);

        Some(BatchDetection {
            pattern: BatchPattern::SequentialNumeric,
            confidence: 0.95,
            url_prefix: common_prefix.clone(),
            variable_part: format!("_{{001..{}}}", numbers.len()),
            suggested_group_name: format!(
                "Batch ({}x {})",
                urls.len(),
                extension.unwrap_or_else(|| "files".to_string())
            ),
            estimated_count: urls.len(),
            suggested_strategy: "Parallel".to_string(),
            reason: format!(
                "Sequential numeric pattern detected ({}...{})",
                numbers.first().unwrap_or(&0),
                numbers.last().unwrap_or(&0)
            ),
        })
    }

    /// Detect archive parts (.zip.001, .zip.002)
    fn detect_archive_parts(urls: &[&str]) -> Option<BatchDetection> {
        let archive_re = Regex::new(r"\.(\w+)\.(\d{3})$").ok()?;
        let mut archive_type = None;
        let mut part_numbers = Vec::new();

        for url in urls {
            if let Some(caps) = archive_re.captures(url) {
                let ext = caps.get(1)?.as_str();
                let part: u32 = caps.get(2)?.as_str().parse().ok()?;

                if archive_type.is_none() {
                    archive_type = Some(ext.to_string());
                } else if archive_type.as_deref() != Some(ext) {
                    return None; // Mixed archive types
                }

                part_numbers.push(part);
            } else {
                return None;
            }
        }

        if part_numbers.len() != urls.len() {
            return None;
        }

        part_numbers.sort();
        let archive_ext = archive_type?;

        Some(BatchDetection {
            pattern: BatchPattern::ArchiveParts,
            confidence: 0.99,
            url_prefix: Self::find_common_prefix(urls),
            variable_part: format!(".{}.{{001..{}}}", archive_ext, urls.len()),
            suggested_group_name: format!(
                "Archive parts ({})",
                archive_ext.to_uppercase()
            ),
            estimated_count: urls.len(),
            suggested_strategy: "Sequential".to_string(),
            reason: format!(
                "Archive parts detected: {}.001 through {}.{:03}",
                archive_ext,
                archive_ext,
                urls.len()
            ),
        })
    }

    /// Detect image gallery pattern
    fn detect_image_gallery(urls: &[&str]) -> Option<BatchDetection> {
        let image_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];
        let valid_images = urls
            .iter()
            .filter(|u| {
                image_exts
                    .iter()
                    .any(|ext| u.to_lowercase().ends_with(ext))
            })
            .count();

        if valid_images < urls.len() * 80 / 100 {
            return None; // Less than 80% are images
        }

        if valid_images < 3 {
            return None; // Need at least 3 images
        }

        let common_prefix = Self::find_common_prefix(urls);
        let domain = Self::extract_domain(&common_prefix)?;

        Some(BatchDetection {
            pattern: BatchPattern::ImageGallery,
            confidence: 0.88,
            url_prefix: common_prefix,
            variable_part: "[image name]".to_string(),
            suggested_group_name: format!("Image gallery from {}", domain),
            estimated_count: valid_images,
            suggested_strategy: "Parallel".to_string(),
            reason: format!("{} images from same gallery detected", valid_images),
        })
    }

    /// Detect directory batch (same path, different files)
    fn detect_directory_batch(urls: &[&str]) -> Option<BatchDetection> {
        let mut parsed_urls: Vec<Url> = urls
            .iter()
            .filter_map(|u| Url::parse(u).ok())
            .collect();

        if parsed_urls.len() != urls.len() {
            return None;
        }

        // Check if all from same domain
        let first_host = parsed_urls[0].host_str()?;
        if !parsed_urls.iter().all(|u| u.host_str() == Some(first_host)) {
            return None;
        }

        // Check if paths are similar (same directory)
        let mut paths: Vec<_> = parsed_urls.iter().map(|u| u.path()).collect();
        let common_dir = Self::find_common_path(&paths)?;

        if paths.len() < 3 {
            return None; // Need at least 3
        }

        Some(BatchDetection {
            pattern: BatchPattern::DirectoryBatch,
            confidence: 0.82,
            url_prefix: format!(
                "{}://{}{}",
                parsed_urls[0].scheme(),
                first_host,
                common_dir
            ),
            variable_part: "[filename]".to_string(),
            suggested_group_name: format!("Files from {}", first_host),
            estimated_count: paths.len(),
            suggested_strategy: "Parallel".to_string(),
            reason: format!(
                "{} files from same directory: {}",
                paths.len(),
                common_dir
            ),
        })
    }

    /// Detect generic batch (domain + similar stems)
    fn detect_generic_batch(urls: &[&str]) -> Option<BatchDetection> {
        let mut parsed_urls: Vec<Url> = urls
            .iter()
            .filter_map(|u| Url::parse(u).ok())
            .collect();

        if parsed_urls.len() < 3 {
            return None;
        }

        // Check if all from same domain
        let first_host = parsed_urls[0].host_str()?;
        let all_same_host = parsed_urls.iter().all(|u| u.host_str() == Some(first_host));

        if !all_same_host {
            return None;
        }

        Some(BatchDetection {
            pattern: BatchPattern::Generic,
            confidence: 0.70,
            url_prefix: format!("{}://{}", parsed_urls[0].scheme(), first_host),
            variable_part: "[path]".to_string(),
            suggested_group_name: format!("Downloads from {}", first_host),
            estimated_count: urls.len(),
            suggested_strategy: "Hybrid".to_string(),
            reason: format!("{} files from same host", urls.len()),
        })
    }

    /// Find common prefix among URLs
    fn find_common_prefix(urls: &[&str]) -> String {
        if urls.is_empty() {
            return String::new();
        }

        let mut prefix = urls[0].to_string();
        for url in &urls[1..] {
            while !url.starts_with(&prefix) && !prefix.is_empty() {
                prefix.pop();
            }
        }

        // Clean up trailing special chars
        while prefix.ends_with(['/', '_', '-', '.']) {
            prefix.pop();
        }

        prefix
    }

    /// Find common directory path
    fn find_common_path(paths: &[&str]) -> Option<String> {
        if paths.is_empty() {
            return None;
        }

        let mut common = paths[0].to_string();

        for path in &paths[1..] {
            while !path.starts_with(&common) && !common.is_empty() {
                if common.ends_with('/') {
                    common.pop();
                }
                common.pop();
            }
        }

        if common.is_empty() || common == "/" {
            return None;
        }

        Some(common)
    }

    /// Extract file extension
    fn extract_extension(url: &str) -> Option<String> {
        url.split('.').last().map(|s| s.to_lowercase())
    }

    /// Extract domain from URL
    fn extract_domain(url: &str) -> Option<String> {
        if let Ok(parsed) = Url::parse(url) {
            parsed.host_str().map(|s| s.to_string())
        } else {
            // Try manual extraction for malformed URLs
            let url = url.trim_start_matches("http://").trim_start_matches("https://");
            let domain = url.split('/').next()?;
            Some(domain.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_numeric() {
        let urls = vec![
            "http://example.com/image_001.jpg",
            "http://example.com/image_002.jpg",
            "http://example.com/image_003.jpg",
        ];
        let detection = BatchDetector::detect_sequential_numeric(&urls).unwrap();
        assert_eq!(detection.pattern, BatchPattern::SequentialNumeric);
        assert!(detection.confidence > 0.9);
    }

    #[test]
    fn test_archive_parts() {
        let urls = vec![
            "http://example.com/file.zip.001",
            "http://example.com/file.zip.002",
            "http://example.com/file.zip.003",
        ];
        let detection = BatchDetector::detect_archive_parts(&urls).unwrap();
        assert_eq!(detection.pattern, BatchPattern::ArchiveParts);
        assert_eq!(detection.confidence, 0.99);
    }

    #[test]
    fn test_image_gallery() {
        let urls = vec![
            "http://example.com/gallery/pic1.jpg",
            "http://example.com/gallery/pic2.png",
            "http://example.com/gallery/pic3.jpg",
        ];
        let detection = BatchDetector::detect_image_gallery(&urls).unwrap();
        assert_eq!(detection.pattern, BatchPattern::ImageGallery);
    }

    #[test]
    fn test_generic_batch() {
        let urls = vec![
            "http://example.com/path1/file1.txt",
            "http://example.com/path2/file2.txt",
            "http://example.com/path3/file3.txt",
        ];
        let detection = BatchDetector::detect_generic_batch(&urls).unwrap();
        assert_eq!(detection.pattern, BatchPattern::Generic);
    }

    #[test]
    fn test_auto_detect() {
        let urls = vec![
            "http://example.com/chapter_01.pdf",
            "http://example.com/chapter_02.pdf",
            "http://example.com/chapter_03.pdf",
        ];

        let detected = BatchDetector::detect_batch(
            &urls.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );

        assert!(detected.is_some());
        let detection = detected.unwrap();
        assert!(detection.confidence > 0.8);
    }
}
