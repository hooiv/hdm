#!/usr/bin/env pwsh
<#
.SYNOPSIS
Fast Rust compilation for HyperStream development.

.DESCRIPTION
Optimizes cargo check for Windows + Tauri environment:
- Uses Ninja generator (faster linker)
- Limits parallel jobs to 4 (prevents resource exhaustion)
- Runs incremental checks only (skips unnecessary rebuilds)
- Provides quiet mode for fast feedback loops

.PARAMETER Mode
Check mode: 'quick', 'lib', 'tests', 'full' (default: quick)

.PARAMETER Quiet
Suppress warnings, show only errors (faster output)

.EXAMPLE
./cargo-check-fast.ps1                  # Quick incremental check
./cargo-check-fast.ps1 -Mode lib        # Check library only
./cargo-check-fast.ps1 -Mode full       # Full workspace check
./cargo-check-fast.ps1 -Quiet            # Suppress warnings
#>

param(
    [ValidateSet('quick', 'lib', 'tests', 'full')]
    [string]$Mode = 'quick',
    
    [switch]$Quiet
)

# Set up build environment
$env:CMAKE_GENERATOR = "Ninja"
$env:CARGO_INCREMENTAL = "1"

# Change to workspace
Push-Location "$PSScriptRoot\..\src-tauri"

Write-Host "HyperStream Rust Check ($Mode mode)" -ForegroundColor Cyan
Write-Host "Environment: Ninja + j4 + incremental" -ForegroundColor Gray

$startTime = Get-Date

try {
    $args = @('check', '-p', 'hyperstream', '-j', '4')
    
    # Add mode-specific flags
    switch ($Mode) {
        'lib'   { $args += '--lib' }
        'tests' { $args += '--tests' }
        'full'  { $args += '--workspace' }
        'quick' { } # default, checks lib + binaries
    }
    
    # Add quiet flag if requested
    if ($Quiet) {
        $args += '-q'
    }
    
    # Run cargo check
    & cargo @args
    $exitCode = $LASTEXITCODE
    
    $duration = (Get-Date) - $startTime
    
    if ($exitCode -eq 0) {
        Write-Host "`n✓ Check passed in $($duration.TotalSeconds.ToString('F1'))s" -ForegroundColor Green
    } else {
        Write-Host "`n✗ Check failed (exit code: $exitCode)" -ForegroundColor Red
    }
    
    exit $exitCode
}
finally {
    Pop-Location
}
