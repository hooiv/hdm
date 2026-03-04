use std::path::Path;
use tokio::fs;

/// Execute a SQL query on a downloaded CSV or JSON file.
/// Uses a simple in-memory approach: parse the file, then filter/project based on SQL-like syntax.
/// Supports: SELECT columns FROM file WHERE conditions ORDER BY column LIMIT n
pub async fn query_file(file_path: String, sql: String) -> Result<serde_json::Value, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Restrict to downloads directory to prevent arbitrary file reads
    let settings = crate::settings::load_settings();
    let downloads_dir = Path::new(&settings.download_dir);
    let canon_target = dunce::canonicalize(path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    let canon_downloads = dunce::canonicalize(downloads_dir)
        .unwrap_or_else(|_| downloads_dir.to_path_buf());
    if !canon_target.starts_with(&canon_downloads) {
        return Err("Only files inside the download directory can be queried".to_string());
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    // Cap file size to prevent OOM when loading into memory
    let file_size = fs::metadata(path).await.map_err(|e| format!("Metadata error: {}", e))?.len();
    if file_size > 100 * 1024 * 1024 {
        return Err(format!("File too large for SQL query: {} bytes (max 100 MB)", file_size));
    }

    let content = fs::read_to_string(path).await.map_err(|e| format!("Read error: {}", e))?;

    let rows: Vec<serde_json::Value> = match ext.as_str() {
        "json" => parse_json_data(&content)?,
        "csv" => parse_csv_data(&content)?,
        _ => return Err("Only .csv and .json files are supported for SQL queries.".to_string()),
    };

    if rows.is_empty() {
        return Ok(serde_json::json!({
            "columns": [],
            "rows": [],
            "total_rows": 0,
            "query": sql,
        }));
    }

    // Parse SQL-like query
    let query = parse_simple_sql(&sql)?;

    // Apply WHERE filter
    let filtered: Vec<&serde_json::Value> = rows.iter()
        .filter(|row| matches_where(row, &query.where_clause))
        .collect();

    // Apply SELECT projection
    let projected: Vec<serde_json::Value> = filtered.iter()
        .map(|row| project_columns(row, &query.select_columns))
        .collect();

    // Apply ORDER BY (simple string sort)
    let mut sorted = projected;
    if let Some(ref order_col) = query.order_by {
        sorted.sort_by(|a, b| {
            let va = a.get(order_col).and_then(|v| v.as_str()).unwrap_or("");
            let vb = b.get(order_col).and_then(|v| v.as_str()).unwrap_or("");
            va.cmp(vb)
        });
    }

    // Apply LIMIT
    let limited: Vec<serde_json::Value> = if let Some(limit) = query.limit {
        sorted.into_iter().take(limit).collect()
    } else {
        sorted.into_iter().take(100).collect() // Default limit 100
    };

    let columns: Vec<String> = if let Some(first) = limited.first() {
        if let Some(obj) = first.as_object() {
            obj.keys().cloned().collect()
        } else { vec![] }
    } else { vec![] };

    Ok(serde_json::json!({
        "columns": columns,
        "rows": limited,
        "total_rows": filtered.len(),
        "query": sql,
    }))
}

struct SimpleQuery {
    select_columns: Vec<String>, // empty = all (*)
    where_clause: Vec<(String, String, String)>, // (column, op, value)
    order_by: Option<String>,
    limit: Option<usize>,
}

fn parse_simple_sql(sql: &str) -> Result<SimpleQuery, String> {
    let sql_upper = sql.to_uppercase();
    let sql_trimmed = sql.trim();

    let mut query = SimpleQuery {
        select_columns: vec![],
        where_clause: vec![],
        order_by: None,
        limit: None,
    };

    // Extract SELECT columns
    if let Some(select_pos) = sql_upper.find("SELECT") {
        let after_select = &sql_trimmed[select_pos + 6..];
        let from_pos = after_select.to_uppercase().find("FROM").unwrap_or(after_select.len());
        let cols_str = after_select[..from_pos].trim();
        if cols_str != "*" {
            query.select_columns = cols_str.split(',').map(|s| s.trim().to_string()).collect();
        }
    }

    // Extract WHERE conditions (simple: column = 'value' or column > value)
    if let Some(where_pos) = sql_upper.find("WHERE") {
        let after_where = &sql_trimmed[where_pos + 5..];
        let end_pos = after_where.to_uppercase().find("ORDER")
            .or_else(|| after_where.to_uppercase().find("LIMIT"))
            .unwrap_or(after_where.len());
        let where_str = after_where[..end_pos].trim();
        
        // Split by AND
        for condition in where_str.split("AND") {
            let cond = condition.trim();
            for op in &[">=", "<=", "!=", "=", ">", "<", "LIKE"] {
                if let Some(op_pos) = cond.to_uppercase().find(op) {
                    let col = cond[..op_pos].trim().to_string();
                    let val = cond[op_pos + op.len()..].trim().trim_matches('\'').trim_matches('"').to_string();
                    query.where_clause.push((col, op.to_string(), val));
                    break;
                }
            }
        }
    }

    // Extract ORDER BY
    if let Some(order_pos) = sql_upper.find("ORDER BY") {
        let after_order = &sql_trimmed[order_pos + 8..];
        let end_pos = after_order.to_uppercase().find("LIMIT").unwrap_or(after_order.len());
        query.order_by = Some(after_order[..end_pos].trim().to_string());
    }

    // Extract LIMIT
    if let Some(limit_pos) = sql_upper.find("LIMIT") {
        let after_limit = &sql_trimmed[limit_pos + 5..].trim();
        if let Ok(n) = after_limit.parse::<usize>() {
            query.limit = Some(n);
        }
    }

    Ok(query)
}

fn matches_where(row: &serde_json::Value, conditions: &[(String, String, String)]) -> bool {
    if conditions.is_empty() { return true; }

    for (col, op, val) in conditions {
        let field_val = row.get(col).and_then(|v| {
            if let Some(s) = v.as_str() { Some(s.to_string()) }
            else { Some(v.to_string()) }
        }).unwrap_or_default();

        let matches = match op.as_str() {
            "=" => field_val == *val,
            "!=" => field_val != *val,
            ">" => field_val.parse::<f64>().unwrap_or(0.0) > val.parse::<f64>().unwrap_or(0.0),
            "<" => field_val.parse::<f64>().unwrap_or(0.0) < val.parse::<f64>().unwrap_or(0.0),
            ">=" => field_val.parse::<f64>().unwrap_or(0.0) >= val.parse::<f64>().unwrap_or(0.0),
            "<=" => field_val.parse::<f64>().unwrap_or(0.0) <= val.parse::<f64>().unwrap_or(0.0),
            "LIKE" => field_val.to_lowercase().contains(&val.to_lowercase().replace('%', "")),
            _ => false,
        };

        if !matches { return false; }
    }
    true
}

fn project_columns(row: &serde_json::Value, columns: &[String]) -> serde_json::Value {
    if columns.is_empty() {
        return row.clone();
    }
    
    let mut obj = serde_json::Map::new();
    if let Some(map) = row.as_object() {
        for col in columns {
            if let Some(val) = map.get(col) {
                obj.insert(col.clone(), val.clone());
            }
        }
    }
    serde_json::Value::Object(obj)
}

fn parse_json_data(content: &str) -> Result<Vec<serde_json::Value>, String> {
    let parsed: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    if let Some(arr) = parsed.as_array() {
        Ok(arr.clone())
    } else {
        Ok(vec![parsed])
    }
}

fn parse_csv_data(content: &str) -> Result<Vec<serde_json::Value>, String> {
    let mut lines = content.lines();
    let header_line = lines.next().ok_or("Empty CSV file")?;
    let headers: Vec<&str> = header_line.split(',').map(|s| s.trim().trim_matches('"')).collect();

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() { continue; }
        let values: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
        let mut obj = serde_json::Map::new();
        for (i, header) in headers.iter().enumerate() {
            let val = values.get(i).unwrap_or(&"");
            obj.insert(header.to_string(), serde_json::Value::String(val.to_string()));
        }
        rows.push(serde_json::Value::Object(obj));
    }

    Ok(rows)
}
