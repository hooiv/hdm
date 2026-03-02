import os
import re

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'
OUT_DIR = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\commands'

if not os.path.exists(OUT_DIR):
    os.makedirs(OUT_DIR)

with open(LIB_PATH, 'r', encoding='utf-8') as f:
    content = f.read()

def extract_commands(src):
    commands = []
    rest = src
    idx = 0
    while True:
        start_idx = rest.find("#[tauri::command]", idx)
        if start_idx == -1:
            break
            
        brace_idx = rest.find("{", start_idx)
        if brace_idx == -1:
            break
            
        brace_count = 1
        end_idx = brace_idx + 1
        while brace_count > 0 and end_idx < len(rest):
            char = rest[end_idx]
            if char == '{':
                brace_count += 1
            elif char == '}':
                brace_count -= 1
            end_idx += 1
            
        command_code = rest[start_idx:end_idx]
        
        match = re.search(r'(?:async\s+)?fn\s+([a-zA-Z_0-9]+)\s*\(', command_code)
        name = match.group(1) if match else "unknown"
        
        if name != "unknown":
            commands.append((name, command_code, start_idx, end_idx))
            
        idx = end_idx
        
    return commands

cmds = extract_commands(content)
print(f"Extracted {len(cmds)} commands.")

# Make commands/mod.rs
commands_code = "\n\n".join([c[1] for c in cmds])
commands_file_content = f"""
use tauri::{{State, AppHandle, Manager}};
use tauri::menu::Menu;
use tauri::tray::TrayIconBuilder;
use std::sync::{{Arc, Mutex}};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::broadcast;
// Add all necessary imports pulled from lib.rs
use crate::*;
use crate::downloader::manager::DownloadManager;
use crate::downloader::disk::{{DiskWriter, WriteRequest}};
use crate::http_server::StreamingSource;
use crate::persistence::SavedDownload;
use crate::settings::Settings;

{commands_code}
"""

with open(os.path.join(OUT_DIR, 'mod.rs'), 'w', encoding='utf-8') as f:
    f.write(commands_file_content)

# Remove the commands from new_content
new_content = content
for _, _, start, end in reversed(cmds):
    new_content = new_content[:start] + new_content[end:]
    
# Add `pub mod commands;` at the top
new_content = "pub mod commands;\n" + new_content

# Now replace the items inside generate_handler!
# Find the generate_handler! block
match = re.search(r'generate_handler!\[(.*?)\]', new_content, re.DOTALL)
if match:
    handler_block = match.group(1)
    new_handler_block = handler_block
    cmd_names = [c[0] for c in cmds]
    for cmd in cmd_names:
        # replace exact match of command, skip if already commands::
        # \b ensures word boundary
        new_handler_block = re.sub(r'(?<!commands::)\b' + cmd + r'\b', f"commands::{cmd}", new_handler_block)
        
    new_content = new_content.replace(handler_block, new_handler_block)

with open(LIB_PATH, 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Split completed successfully and lib.rs patched.")
