import sys
import os

target_file = 'src/lib.rs'
if not os.path.exists(target_file):
    print(f"Error: {target_file} not found")
    sys.exit(1)

with open(target_file, 'r', encoding='utf-8') as f:
    content = f.read()

target_marker = 'aggregator.start_loop(app_handle).await;\n                 });\n             }'
if target_marker not in content:
    # Try different whitespace pattern (common in some editors)
    target_marker = 'aggregator.start_loop(app_handle).await;\n                });\n            }'

if target_marker in content:
    replacement = target_marker + '''

             // Start Parallel Mirror Retry monitoring loop
             {
                 let app_handle = app.handle().clone();
                 let retry_manager = app.handle().state::<AppState>().parallel_mirror_retry.clone();
                 tauri::async_runtime::spawn(async move {
                     retry_manager.start_loop(app_handle).await;
                 });
             }'''
    new_content = content.replace(target_marker, replacement)
    with open(target_file, 'w', encoding='utf-8', newline='\n') as f:
        f.write(new_content)
    print("Successfully added loop to lib.rs")
else:
    print("Could not find the target marker for loop insertion")
    # Print a snippet to help debug
    idx = content.find('Start Mirror Aggregator discovery loop')
    if idx != -1:
        print("Marker context:")
        print(content[idx:idx+200])
    sys.exit(1)
