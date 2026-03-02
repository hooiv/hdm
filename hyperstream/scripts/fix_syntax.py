import re

HOOK_PATH = r'c:\Users\aditya\Desktop\hdm\hyperstream\src\hooks\useDownloadActions.ts'

with open(HOOK_PATH, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix toast.error('... failed: ' + err'); -> toast.error('... failed: ' + err);
content = content.replace(" + err');", " + err);")

with open(HOOK_PATH, 'w', encoding='utf-8') as f:
    f.write(content)

print("Syntax fixed.")
