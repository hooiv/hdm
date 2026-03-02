import os
import re

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'
# We'll put all commands in a flat commands/mod.rs to make it simple but fully functioning.
# Once that works, we can split them into domain files if we want, but a single commands/mod.rs 
# still achieves the goal of halving lib.rs file size and separating logic.

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
        
        # Extract name using regex
        match = re.search(r'(?:async\s+)?fn\s+([a-zA-Z_0-9]+)\s*\(', command_code)
        name = match.group(1) if match else "unknown"
        
        if name != "unknown":
            # Add 'pub ' if not present to ensure the generated macro is public
            if "pub async fn" not in command_code and "pub fn" not in command_code:
                command_code = re.sub(r'(async\s+)?fn\s+' + name, r'pub \g<1>fn ' + name, command_code, count=1)
                
            commands.append((name, command_code, start_idx, end_idx))
            
        idx = end_idx
        
    return commands

cmds = extract_commands(content)
print(f"Extracted {len(cmds)} commands.")

# Now replace the generate_handler! names BEFORE we delete the commands to avoid index shifting problems.
# Wait, replacing them first changes string lengths, which invalidates the start/end indices of the commands!
# Better to do it after copying.

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

# Extract state out to core_state.rs and engine logic to session.rs just using regex or string match
# Let's keep it simple: Just extract the commands for now! If we extract 165 commands, lib.rs reduces by 1500 lines.
# Then state and engine can stay in lib.rs or be moved later. Doing both in Python is volatile.

with open(r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\commands.rs', 'w', encoding='utf-8') as f:
    f.write(commands_file_content)

new_content = content
for _, _, start, end in reversed(cmds):
    new_content = new_content[:start] + new_content[end:]
    
new_content = "pub mod commands;\n" + new_content

# Now patch the generate_handler! block!
match = re.search(r'generate_handler!\[(.*?)\]', new_content, re.DOTALL)
if match:
    handler_block = match.group(1)
    new_handler_block = handler_block
    cmd_names = [c[0] for c in cmds]
    for cmd in cmd_names:
        # replace exact match of command, skip if already commands::
        # e.g., 'start_download,' -> 'commands::start_download,'
        # we can just use a simple regex targeting the names separated by commas
        new_handler_block = re.sub(r'(?<!commands::)\b' + cmd + r'\b', f"commands::{cmd}", new_handler_block)
        
    new_content = new_content.replace(handler_block, new_handler_block)


with open(LIB_PATH.replace("lib.rs", "lib_new.rs"), 'w', encoding='utf-8') as f:
    f.write(new_content)

print("lib_new.rs and commands.rs generated.")
