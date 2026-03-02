import os
import re

HOOK_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src\hooks\useDownloadActions.ts'

with open(HOOK_PATH, 'r', encoding='utf-8') as f:
    content = f.read()

# Add useToast import
if "import { useToast }" not in content:
    content = content.replace("import { invoke } from '@tauri-apps/api/core';", "import { invoke } from '@tauri-apps/api/core';\nimport { useToast } from '../contexts/ToastContext';")

# Add const toast = useToast(); inside hook
if "const toast = useToast();" not in content:
    content = content.replace("export function useDownloadActions(task: DownloadTask, filePath: string) {", "export function useDownloadActions(task: DownloadTask, filePath: string) {\n    const toast = useToast();")

# Regex replaces for alerts
# Error alerts
content = re.sub(r"alert\('([^']+ failed: ' \+ err)\);", r"toast.error('\1');", content)
content = re.sub(r"alert\('([^']+ failed: '\s*\+\s*err)\);", r"toast.error('\1');", content)

# Success alerts (starts with ✅ or contains Successful or Complete or extracted or Notarized)
def replace_alert(match):
    # match.group() is the whole alert(...)
    inner = match.group(1) # inside alert( ... )
    
    if "failed:" in inner.lower() or "error" in inner.lower() or "invalid" in inner.lower() or "no " in inner.lower() or "❌" in inner:
        return f"toast.error({inner});"
    elif "✅" in inner or "🛡️" in inner or "📜 Notarized" in inner or "Complete" in inner or "extracted!" in inner or "hidden!" in inner or "Extracted" in inner or "Successfully" in inner or "Refreshed" in inner or "refreshed" in inner or "Found" in inner or "Subtitles " in inner:
        return f"toast.success({inner});"
    else:
        return f"toast.info({inner});"

content = re.sub(r"alert\((.*?)\);", replace_alert, content)

with open(HOOK_PATH, 'w', encoding='utf-8') as f:
    f.write(content)

print("Alerts replaced in hook.")
