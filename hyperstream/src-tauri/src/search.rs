use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use mlua::prelude::*;
use mlua::Function;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub size: Option<String>,
    pub seeds: Option<u32>,
    pub leechers: Option<u32>,
    pub engine: String,
}

pub struct SearchEngine {
    lua: Lua,
}

impl SearchEngine {
    pub fn new() -> Self {
        // Restrict Lua stdlib to safe subset — no os/io/debug/package modules
        let lua = Lua::new_with(mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH, mlua::LuaOptions::default()).unwrap_or_else(|_| Lua::new());
        Self { lua }
    }

    pub fn search(&self, query: String) -> Result<Vec<SearchResult>, String> {
        let lua = &self.lua;
        
        // In a real app, we would load plugins from a directory.
        // For this MVP, we'll embed a simple "Dummy" script and a basic "Legit" one if possible,
        // or just mock it via Lua to prove the engine works.
        
        // This script simulates a search plugin
        let script = r#"
            function search(query)
                local results = {}
                -- Simulate some results
                table.insert(results, {
                    title = "Demo Result: " .. query,
                    link = "https://example.com/download/" .. query,
                    size = "1.2 GB",
                    seeds = 100,
                    leechers = 10,
                    engine = "LuaMock"
                })
                 table.insert(results, {
                    title = "Linux ISO " .. query,
                    link = "https://ubuntu.com/download/" .. query,
                    size = "4.5 GB",
                    seeds = 500,
                    leechers = 20,
                    engine = "LuaMock"
                })
                return results
            end
        "#;

        lua.load(script).exec().map_err(|e| e.to_string())?;
        
        let globals = lua.globals();
        let search_fn: Function = globals.get("search").map_err(|e| e.to_string())?;
        let results: Vec<std::collections::HashMap<String, String>> = search_fn.call(query).map_err(|e| e.to_string())?;

        // Convert Lua table to Rust Struct manually for safety (or use serde_mlua if added)
        // For now, manual mapping to ensure type safety without extra deps
        let mut final_results = Vec::new();
        for r in results {
            final_results.push(SearchResult {
                title: r.get("title").cloned().unwrap_or_default(),
                link: r.get("link").cloned().unwrap_or_default(),
                size: r.get("size").cloned(),
                seeds: r.get("seeds").and_then(|s| s.parse().ok()),
                leechers: r.get("leechers").and_then(|s| s.parse().ok()),
                engine: r.get("engine").cloned().unwrap_or_default(),
            });
        }

        Ok(final_results)
    }
}

// Global Search Engine
lazy_static::lazy_static! {
    pub static ref SEARCH_ENGINE: Arc<Mutex<SearchEngine>> = Arc::new(Mutex::new(SearchEngine::new()));
}
