import os
import re

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'
LIB_OUT_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib_new.rs'
COMMANDS_DIR = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\commands'

with open(LIB_PATH, 'r', encoding='utf-8') as f:
    lines = f.readlines()

command_groups = {
    "archive": ["extract_archive", "cleanup_archive", "check_unrar_available", "download_as_warc"],
    "network": ["add_magnet_link", "play_torrent", "get_torrents", "create_p2p_share", "join_p2p_share", "list_p2p_sessions", "close_p2p_session", "get_p2p_stats", "set_p2p_upload_limit", "get_p2p_upload_limit", "check_wayback_availability", "get_wayback_url", "download_ipfs"],
    "security": ["scrub_metadata", "get_file_metadata", "validate_c2pa", "notarize_file", "verify_notarization", "run_in_sandbox", "stego_hide", "stego_extract"],
    "system": ["get_settings", "save_settings", "open_folder", "open_file", "list_usb_drives", "flash_to_usb", "mount_drive", "unmount_drive", "get_scheduled_downloads", "cancel_scheduled_download", "schedule_download"],
    "media": ["mux_video_audio", "check_ffmpeg_installed", "set_custom_sound_path", "clear_custom_sound_path", "get_custom_sound_paths", "generate_subtitles", "discover_dlna", "cast_to_dlna"],
    "devtools": ["export_data", "import_data", "generate_lan_pairing_code", "get_lan_pairing_qr_data", "get_local_ip", "replay_request", "fuzz_url", "query_file", "launch_tui_dashboard", "set_geofence_rule", "get_geofence_rules", "resolve_doi", "fetch_docker_manifest"],
    "cloud": ["rclone_list_remotes", "rclone_transfer", "upload_to_cloud"],
    "download": ["start_download", "pause_download", "get_downloads", "remove_download_entry", "refresh_download_url", "arbitrage_download", "optimize_mods", "set_download_priority", "get_qos_stats", "find_mirrors", "start_ephemeral_share", "stop_ephemeral_share", "list_ephemeral_shares", "crawl_website"]
}

# Add any unmapped command to misc.rs
commands_found = []
new_lib_lines = []
current_domains = {k: [] for k in command_groups.keys()}
current_domains["misc"] = []

i = 0
in_command = False
brace_count = 0
command_buffer = []

while i < len(lines):
    line = lines[i]
    if not in_command and "#[tauri::command]" in line:
        in_command = True
        brace_count = 0
        command_buffer = [line]
        
        # Check next line for function name
        next_line = lines[i+1]
        command_buffer.append(next_line)
        i += 1
        
        match = re.search(r'(?:async\s+)?fn\s+([a-zA-Z_0-9]+)\s*\(', next_line)
        if match:
            cmd_name = match.group(1)
            commands_found.append(cmd_name)
        else:
            cmd_name = "unknown"
            
        brace_count += next_line.count("{") - next_line.count("}")
        
    elif in_command:
        command_buffer.append(line)
        brace_count += line.count("{") - line.count("}")
        if brace_count <= 0 and "}" in line: # Found end of function
            in_command = False
            
            # Map to domain
            mapped_domain = "misc"
            for domain, cmds in command_groups.items():
                if cmd_name in cmds:
                    mapped_domain = domain
                    break
                    
            if mapped_domain == "misc" and cmd_name != "unknown":
                # Print warning for unmapped
                print(f"Warning: Command '{cmd_name}' not mapped to a specific domain. Putting in misc.rs")
                
            current_domains[mapped_domain].extend(command_buffer)
            current_domains[mapped_domain].append("\n")
    else:
        new_lib_lines.append(line)
    
    i += 1

print(f"Extracted {len(commands_found)} commands.")

# Now we have all commands in `current_domains`
# Write them to commands/*.rs
if not os.path.exists(COMMANDS_DIR):
    os.makedirs(COMMANDS_DIR)

mod_rs_lines = []
for domain, code_lines in current_domains.items():
    if not code_lines: continue
    mod_rs_lines.append(f"pub mod {domain};\n")
    
    with open(os.path.join(COMMANDS_DIR, f"{domain}.rs"), 'w', encoding='utf-8') as f:
        # Standard imports needed for commands
        f.write("use tauri::{State, AppHandle};\n")
        f.write("use crate::AppState;\n\n")
        f.writelines(code_lines)

with open(os.path.join(COMMANDS_DIR, "mod.rs"), 'w', encoding='utf-8') as f:
    f.writelines(mod_rs_lines)
    
# Now replacing lib.rs generate_handler! macro. This is tricky.
# We need to find `.invoke_handler(tauri::generate_handler![` and replace all the commands inside it
# with `crate::commands::{domain}::{command_name}`

with open(LIB_OUT_PATH, 'w', encoding='utf-8') as f:
    f.writelines(new_lib_lines)

print("Split completed successfully into staging files.")
