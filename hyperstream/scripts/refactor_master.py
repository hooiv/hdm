import os

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'
COMMANDS_DIR = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\commands'

with open(LIB_PATH, 'r', encoding='utf-8') as f:
    lines = f.readlines()

new_lib_lines = []
commands_lines = ["use tauri::{State, AppHandle, Manager};\nuse std::sync::{Arc, Mutex};\nuse tokio::sync::broadcast;\nuse std::collections::HashMap;\nuse crate::*;\nuse crate::core_state::*;\nuse crate::engine::session::*;\nuse crate::downloader::manager::DownloadManager;\nuse crate::downloader::disk::{DiskWriter, WriteRequest};\nuse crate::http_server::StreamingSource;\nuse crate::persistence::SavedDownload;\nuse crate::settings::Settings;\n\n"]
state_lines = ["use std::sync::{Arc, Mutex};\nuse std::collections::HashMap;\nuse tokio::sync::broadcast;\nuse crate::downloader::manager::DownloadManager;\nuse crate::http_server;\nuse crate::network;\n\n"]
engine_lines = ["use std::sync::{Arc, Mutex};\nuse std::sync::atomic::{AtomicU64, Ordering};\nuse tokio::sync::broadcast;\nuse crate::core_state::*;\nuse crate::*;\n\n"]

new_lib_lines.append("pub mod core_state;\npub use core_state::*;\npub mod engine;\npub use engine::session::*;\npub mod commands;\n\n")

i = 0
in_commands_struct = False

while i < len(lines):
    line = lines[i]
    
    # 1. Extract State
    if "type SlimSegment =" in line:
        state_lines.append(line)
        i += 1
        while i < len(lines):
            state_lines.append(lines[i])
            i += 1
            if "pub(crate) chatops_manager: Arc<network::chatops::ChatOpsManager>," in lines[i-1]:
                state_lines.append(lines[i]) # The closing brace
                i += 1
                break
        continue

    # 2. Extract engine
    if "pub async fn start_download_impl(" in line:
        start_impl_braces = line.count("{") - line.count("}")
        engine_lines.append(line)
        while i < len(lines):
            i += 1
            if i < len(lines):
                engine_lines.append(lines[i])
                start_impl_braces += lines[i].count("{") - lines[i].count("}")
                if start_impl_braces <= 0 and "}" in lines[i]:
                    i += 1
                    break
        continue

    # 3. Extract Tauri Commands
    if "#[tauri::command]" in line:
        cmd_buffer = [line]
        if i+1 < len(lines):
            next_line = lines[i+1]
            cmd_buffer.append(next_line)
            brace_count = next_line.count("{") - next_line.count("}")
            i += 1
            while i < len(lines):
                if brace_count <= 0 and "}" in lines[i]:
                    break
                i += 1
                if i < len(lines):
                    cmd_buffer.append(lines[i])
                    brace_count += lines[i].count("{") - lines[i].count("}")
            i += 1
            commands_lines.extend(cmd_buffer)
            commands_lines.append("\n")
            continue

    # 4. Inject `use` into run()
    if "pub fn run() {" in line:
        new_lib_lines.append(line)
        new_lib_lines.append("    use crate::commands::*;\n")
        i += 1
        continue

    new_lib_lines.append(line)
    i += 1

# Make state structs pub
PUB_REPLACES = {
    "type SlimSegment": "pub type SlimSegment",
    "struct Payload": "pub struct Payload",
    "struct DownloadSession": "pub struct DownloadSession",
    "pub(crate) struct AppState": "pub struct AppState",
    "id: String,": "pub id: String,",
    "downloaded: u64,": "pub downloaded: u64,",
    "total: u64,": "pub total: u64,",
    "segments: Vec<SlimSegment>,": "pub segments: Vec<SlimSegment>,",
    "manager: Arc": "pub manager: Arc",
    "stop_tx: broadcast::Sender": "pub stop_tx: broadcast::Sender",
    "url: String,": "pub url: String,",
    "path: String,": "pub path: String,",
    "file_writer: Arc": "pub file_writer: Arc",
    "pub(crate) downloads": "pub downloads",
    "pub(crate) p2p_node": "pub p2p_node",
    "pub(crate) p2p_file_map": "pub p2p_file_map",
    "pub(crate) torrent_manager": "pub torrent_manager",
    "pub(crate) connection_manager": "pub connection_manager",
    "pub(crate) chatops_manager": "pub chatops_manager",
}

final_state = "".join(state_lines)
for old, new in PUB_REPLACES.items():
    final_state = final_state.replace(old, new)

with open(r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\core_state.rs', 'w', encoding='utf-8') as f:
    f.write(final_state)

with open(r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\engine\session.rs', 'w', encoding='utf-8') as f:
    f.writelines(engine_lines)

if not os.path.exists(COMMANDS_DIR):
    os.makedirs(COMMANDS_DIR)

with open(os.path.join(COMMANDS_DIR, 'mod.rs'), 'w', encoding='utf-8') as f:
    f.writelines(commands_lines)

with open(LIB_PATH, 'w', encoding='utf-8') as f:
    f.writelines(new_lib_lines)

print("Master refactoring complete!")
