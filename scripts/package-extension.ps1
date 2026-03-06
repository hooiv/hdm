# PowerShell script to package the browser extension folders into ZIPs suitable for distribution.
# Assumes you are in workspace root when running.

param(
    [string]$Target = "browser-extension", # or "extension" for alternative build
    [string]$OutDir = "./dist"
)

if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}

$base = "$PSScriptRoot/.." | Resolve-Path | Select-Object -ExpandProperty Path
$extPath = Join-Path $base $Target
if (-not (Test-Path $extPath)) {
    Write-Error "Extension folder '$Target' does not exist."
    exit 1
}

$manifest = Get-Content -Path (Join-Path $extPath 'manifest.json') -Raw | ConvertFrom-Json
$version = $manifest.version -as [string]
if (-not $version) { $version = '0.0.0' }

$zipName = "hyperstream-${Target}-$version.zip"
$zipPath = Join-Path $OutDir $zipName

if (Test-Path $zipPath) { Remove-Item $zipPath }

Write-Host "Packaging $Target → $zipPath"
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($extPath, $zipPath)

Write-Host "Done. Use chrome://extensions (Developer mode) to load the ZIP or unpacked folder."