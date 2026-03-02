import os

LIB_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src-tauri\src\lib.rs'

with open(LIB_PATH, 'r', encoding='utf-8') as f:
    lines = f.readlines()

new_lines = []

new_lines.append("pub mod core_state;\n")
new_lines.append("pub use core_state::*;\n")
new_lines.append("pub mod engine;\n")
new_lines.append("pub use engine::session::*;\n")

i = 0
while i < len(lines):
    line = lines[i]
    
    # Remove state struct definitions
    if "type SlimSegment =" in line:
        i += 30
        continue
        
    # Remove start_download_impl
    if "pub async fn start_download_impl(" in line:
        start_impl_braces = line.count("{") - line.count("}")
        while i < len(lines):
            i += 1
            if i < len(lines):
                start_impl_braces += lines[i].count("{") - lines[i].count("}")
                if start_impl_braces <= 0 and "}" in lines[i]:
                    break
        i += 1
        continue

    new_lines.append(line)
    i += 1

with open(LIB_PATH, 'w', encoding='utf-8') as f:
    f.writelines(new_lines)

print("lib.rs cleaned.")
