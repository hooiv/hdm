import sys
import os

target_file = 'src/lib.rs'
if not os.path.exists(target_file):
    print(f"Error: {target_file} not found")
    sys.exit(1)

with open(target_file, 'r', encoding='utf-8') as f:
    lines = f.readlines()

new_lines = []
fixed = False
for line in lines:
    if 'mirror_aggregator' in line and 'parallel_mirror_retry' in line:
        new_lines.append('                  mirror_aggregator: Arc::new(crate::network::mirror_aggregator::MirrorAggregator::new()),\n')
        new_lines.append('                  parallel_mirror_retry: Arc::new(crate::parallel_mirror_retry::ParallelMirrorRetryManager::new(Default::default())),\n')
        fixed = True
    else:
        new_lines.append(line)

if fixed:
    with open(target_file, 'w', encoding='utf-8', newline='\n') as f:
        f.writelines(new_lines)
    print("Successfully fixed lib.rs")
else:
    print("Could not find the target line to fix")
    sys.exit(1)
