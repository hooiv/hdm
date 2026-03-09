use mlua::{Table, Value};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;

pub const MAX_QUERY_LEN: usize = 256;
const MAX_RESULTS_PER_PROVIDER: usize = 100;
const MAX_RESULTS_TOTAL: usize = 250;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub size: Option<String>,
    pub seeds: Option<u32>,
    pub leechers: Option<u32>,
    pub engine: String,
}

pub fn sanitize_query(query: &str) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Search query cannot be empty".to_string());
    }
    if trimmed.len() > MAX_QUERY_LEN {
        return Err(format!("Search query too long (max {} chars)", MAX_QUERY_LEN));
    }
    Ok(trimmed.to_string())
}

pub fn normalize_provider_results(value: Value, default_engine: &str) -> mlua::Result<Vec<SearchResult>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::Table(table) => normalize_results_table(table, default_engine),
        _ => Err(mlua::Error::RuntimeError(
            "search() must return an array of result tables".to_string(),
        )),
    }
}

pub fn finalize_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut sorted = results;
    sorted.sort_by(compare_results);

    let mut seen_links = HashSet::new();
    let mut deduped = Vec::new();

    for result in sorted {
        let key = result.link.trim().to_string();
        if seen_links.insert(key) {
            deduped.push(result);
        }
    }

    deduped.truncate(MAX_RESULTS_TOTAL);
    deduped
}

fn normalize_results_table(table: Table, default_engine: &str) -> mlua::Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    for entry in table.sequence_values::<Table>() {
        if results.len() >= MAX_RESULTS_PER_PROVIDER {
            break;
        }

        match entry {
            Ok(row) => {
                if let Some(result) = normalize_result_row(row, default_engine)? {
                    results.push(result);
                }
            }
            Err(err) => eprintln!("Skipping malformed search result row: {}", err),
        }
    }

    Ok(results)
}

fn normalize_result_row(row: Table, default_engine: &str) -> mlua::Result<Option<SearchResult>> {
    let title = normalize_string(row.get::<_, Option<String>>("title")?);
    let link = normalize_string(row.get::<_, Option<String>>("link")?);

    let (title, link) = match (title, link) {
        (Some(title), Some(link)) if is_supported_link(&link) => (title, link),
        _ => return Ok(None),
    };

    let engine = normalize_string(row.get::<_, Option<String>>("engine")?)
        .unwrap_or_else(|| default_engine.to_string());

    Ok(Some(SearchResult {
        title,
        link,
        size: normalize_string(row.get::<_, Option<String>>("size")?),
        seeds: parse_optional_u32(&row, "seeds")?,
        leechers: parse_optional_u32(&row, "leechers")?,
        engine,
    }))
}

fn normalize_string(value: Option<String>) -> Option<String> {
    value.map(|item| item.trim().to_string()).filter(|item| !item.is_empty())
}

fn parse_optional_u32(row: &Table, key: &str) -> mlua::Result<Option<u32>> {
    match row.get::<_, Value>(key)? {
        Value::Nil => Ok(None),
        Value::Integer(value) => Ok(u32::try_from(value).ok()),
        Value::Number(value) if value.is_finite() && value >= 0.0 && value <= u32::MAX as f64 => {
            Ok(Some(value as u32))
        }
        Value::String(value) => Ok(value
            .to_str()
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok())),
        _ => Ok(None),
    }
}

fn is_supported_link(link: &str) -> bool {
    let lower = link.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("magnet:")
        || lower.starts_with("ftp://")
        || lower.starts_with("ftps://")
}

fn compare_results(left: &SearchResult, right: &SearchResult) -> Ordering {
    right
        .seeds
        .unwrap_or(0)
        .cmp(&left.seeds.unwrap_or(0))
        .then_with(|| right.leechers.unwrap_or(0).cmp(&left.leechers.unwrap_or(0)))
        .then_with(|| left.title.to_ascii_lowercase().cmp(&right.title.to_ascii_lowercase()))
        .then_with(|| left.engine.to_ascii_lowercase().cmp(&right.engine.to_ascii_lowercase()))
        .then_with(|| left.link.cmp(&right.link))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    #[test]
    fn sanitize_query_rejects_empty_values() {
        assert!(sanitize_query("   ").is_err());
    }

    #[test]
    fn normalize_provider_results_skips_invalid_rows_and_defaults_engine() {
        let lua = Lua::new();
        let table: Table = lua
            .load(
                r#"
                return {
                    { title = "  Ubuntu ISO  ", link = "https://example.com/ubuntu.iso", seeds = "42" },
                    { title = "Fedora", link = "magnet:?xt=urn:btih:abcdef", leechers = 8 },
                    { title = "Missing link" },
                    { title = "Bad scheme", link = "file:///tmp/test.iso" }
                }
                "#,
            )
            .eval()
            .unwrap();

        let results = normalize_provider_results(Value::Table(table), "ProviderX").unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Ubuntu ISO");
        assert_eq!(results[0].engine, "ProviderX");
        assert_eq!(results[0].seeds, Some(42));
        assert_eq!(results[1].link, "magnet:?xt=urn:btih:abcdef");
        assert_eq!(results[1].leechers, Some(8));
    }

    #[test]
    fn finalize_results_dedupes_and_sorts_by_quality() {
        let results = vec![
            SearchResult {
                title: "Result B".to_string(),
                link: "https://example.com/shared".to_string(),
                size: None,
                seeds: Some(10),
                leechers: Some(2),
                engine: "Beta".to_string(),
            },
            SearchResult {
                title: "Result A".to_string(),
                link: "https://example.com/shared".to_string(),
                size: None,
                seeds: Some(100),
                leechers: Some(1),
                engine: "Alpha".to_string(),
            },
            SearchResult {
                title: "Result C".to_string(),
                link: "magnet:?xt=urn:btih:123".to_string(),
                size: None,
                seeds: Some(250),
                leechers: Some(40),
                engine: "Gamma".to_string(),
            },
        ];

        let finalized = finalize_results(results);

        assert_eq!(finalized.len(), 2);
        assert_eq!(finalized[0].title, "Result C");
        assert_eq!(finalized[1].title, "Result A");
    }
}
