import os
import re

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'
# We will create intermediate files to test
OUT_DIR = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\commands'

if not os.path.exists(OUT_DIR):
    os.makedirs(OUT_DIR)

with open(LIB_PATH, 'r', encoding='utf-8') as f:
    content = f.read()

# Find all #[tauri::command] blocks.
# A function block starts with #[tauri::command] and ends when curly braces balance.
matches = []
# We'll search for "#[tauri::command]" then find the start of the fn, then count braces until it closes.

def extract_commands(src):
    commands = []
    rest = src
    idx = 0
    while True:
        # Find next command
        start_idx = rest.find("#[tauri::command]", idx)
        if start_idx == -1:
            break
            
        # Find the opening brace of this function
        brace_idx = rest.find("{", start_idx)
        if brace_idx == -1:
            break
            
        # Count braces to find the end
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
        
        # Extract name using regex
        match = re.search(r'(?:async\s+)?fn\s+([a-zA-Z_0-9]+)\s*\(', command_code)
        name = match.group(1) if match else "unknown"
        
        commands.append((name, command_code, start_idx, end_idx))
        idx = end_idx
        
    return commands

cmds = extract_commands(content)
print(f"Extracted {len(cmds)} commands.")

# Instead of putting them in 15 domains, let's put them all in commands/mod.rs initially, 
# or just group them automatically based on their name.
# Or wait, let's just create commands/mod.rs with all of them, removing them from lib.rs.
# This cuts lib.rs by ~1500 lines immediately!

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

# Now remove the commands from lib.rs
new_content = content
# Remove in reverse order so indices don't shift
for _, _, start, end in reversed(cmds):
    new_content = new_content[:start] + new_content[end:]
    
with open(LIB_PATH.replace("lib.rs", "lib_stripped.rs"), 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Created lib_stripped.rs and commands/mod.rs")
