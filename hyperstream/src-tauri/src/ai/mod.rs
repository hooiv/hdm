use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use serde::{Serialize, Deserialize};
use std::path::Path;


pub mod upscale;

// Simple in-memory Index
#[derive(Serialize, Deserialize, Clone)]
pub struct SearchResult {
    pub path: String,
    pub score: f32, // Cosine similarity
    pub preview: String,
}

#[derive(Serialize, Deserialize)]
struct IndexedFile {
    path: String,
    embedding: Vec<f32>,
    text_preview: String,
    timestamp: u64,
}

pub struct AiEngine {
    #[allow(dead_code)]
    model: Option<TextEmbedding>,
    index: Mutex<Vec<IndexedFile>>,
}

lazy_static! {
    pub static ref AI_ENGINE: Arc<AiEngine> = Arc::new(AiEngine::new());
}

impl AiEngine {
    pub fn new() -> Self {
        // Load model lazily
        Self {
            model: None, // Initialized on first use or explicit init
            index: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    pub fn init(&self) -> Result<(), String> {
        // Only valid if we had interior mutability for model, but TextEmbedding is expensive.
        // For now, we will re-instantiate or use a Mutex if needed.
        // Actually, TextEmbedding includes the ONNX model.
        // Let's change `model` to Mutex<Option<TextEmbedding>> or use a lazy init inside search.
        Ok(())
    }
}

// Global helper since we need Mutex for the model
lazy_static! {
    static ref EMBEDDING_MODEL: Mutex<Option<TextEmbedding>> = Mutex::new(None);
}

pub fn initialize_model() -> Result<(), String> {
    let mut guard = EMBEDDING_MODEL.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        println!("Loading AI Model (all-MiniLM-L6-v2)...");
        let mut options = InitOptions::default();
        options.model_name = EmbeddingModel::AllMiniLML6V2;
        options.show_download_progress = true;
        let model = TextEmbedding::try_new(options).map_err(|e| format!("Failed to load model: {}", e))?;
        *guard = Some(model);
        println!("AI Model Loaded!");
    }
    Ok(())
}

fn get_embedding(text: &str) -> Result<Vec<f32>, String> {
    let mut guard = EMBEDDING_MODEL.lock().map_err(|e| e.to_string())?;
    
    if guard.is_none() {
        // Auto-init
        drop(guard); // drop lock before init to avoid deadlock if init calls lock (it doesn't, but safe practice)
        initialize_model()?;
        guard = EMBEDDING_MODEL.lock().map_err(|e| e.to_string())?;
    }

    if let Some(model) = guard.as_ref() {
        let embeddings = model.embed(vec![text], None).map_err(|e| e.to_string())?;
        Ok(embeddings[0].clone())
    } else {
        Err("Model not initialized".into())
    }
}

pub fn index_file(path: &str) -> Result<(), String> {
    let path_obj = Path::new(path);
    if !path_obj.exists() { return Err("File not found".into()); }

    // Validate path is within download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&settings.download_dir));
    if let Ok(canon) = dunce::canonicalize(path) {
        if !canon.starts_with(&download_dir) {
            return Err("File must be within the download directory".to_string());
        }
    }
    
    // 1. Extract Text
    let ext = path_obj.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    let text = match ext.as_str() {
        "txt" | "md" | "json" | "csv" | "log" => std::fs::read_to_string(path).unwrap_or_default(),
        "pdf" => {
            lopdf::Document::load(path)
                .map(|doc| {
                    // Extract first 5 pages for better context (not just page 1)
                    let pages: Vec<u32> = (1..=5).collect();
                    doc.extract_text(&pages).unwrap_or_default()
                })
                .unwrap_or_default()
        },
        _ => return Ok(()), // Skip unknown types
    };

    if text.trim().is_empty() { return Ok(()); }
    
    // Limit text size
    let text_preview = text.chars().take(200).collect::<String>();
    let text_to_embed = text.chars().take(1000).collect::<String>(); // Embed first 1k chars

    // 2. Embed
    let vector = get_embedding(&text_to_embed)?;

    // 3. Store
    let mut engine = AI_ENGINE.index.lock().unwrap_or_else(|e| e.into_inner());
    // Remove existing
    engine.retain(|f| f.path != path);
    
    // Cap index size to prevent unbounded memory growth
    if engine.len() >= 10_000 {
        engine.remove(0); // Remove oldest entry
    }

    engine.push(IndexedFile {
        path: path.to_string(),
        embedding: vector,
        text_preview,
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
    });
    
    println!("Indexed: {}", path);
    Ok(())
}

pub fn semantic_search(query: &str) -> Result<Vec<SearchResult>, String> {
    let query_vec = get_embedding(query)?;
    let engine = AI_ENGINE.index.lock().unwrap_or_else(|e| e.into_inner());
    
    let mut results = Vec::new();
    
    for file in engine.iter() {
        let score = cosine_similarity(&query_vec, &file.embedding);
        if score > 0.4 { // Threshold
            results.push(SearchResult {
                path: file.path.clone(),
                score,
                preview: file.text_preview.clone(),
            });
        }
    }
    
    // Sort by score desc
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a * norm_b)
}
