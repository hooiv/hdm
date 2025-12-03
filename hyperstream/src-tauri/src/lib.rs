use tauri::Emitter;
use std::io::Write;
use futures_util::StreamExt;

#[derive(Clone, serde::Serialize)]
struct Payload {
    downloaded: u64,
    total: u64,
}

#[tauri::command]
async fn start_download(url: String, path: String, window: tauri::Window) -> Result<(), String> {
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let total_size = res.content_length().unwrap_or(0);

    let mut stream = res.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        window.emit("download_progress", Payload { downloaded, total: total_size }).unwrap();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_download])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
