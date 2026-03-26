# HyperStream Rust Build Optimization Guide

## Quick Performance Summary

**Current Status**: Tauri + boring-sys2 compilation on Windows
- First check after `cargo clean`: ~2-3 minutes (unavoidable, C++ TLS compilation)
- Incremental checks (after first): ~10-30 seconds (normal range)
- Your timeouts suggest either stale lock files or resource exhaustion

## Fastest Development Loop

**Use VS Code Rust Analyzer** (Recommended)
- Runs background incremental checking automatically
- No manual command needed
- Instant feedback on saves
- Install: `rust-analyzer` extension (likely already installed)

## CLI-Based Speed Options

### 1. Quick Lib Check (Fastest)
```powershell
./scripts/cargo-check-fast.ps1 -Mode lib -Quiet
# Only checks library code, skips binaries
# ~5-15 seconds for incremental
```

### 2. Default Quick Check (Balanced)
```powershell
./scripts/cargo-check-fast.ps1 -Quiet
# Checks lib + binaries, skips tests
# ~10-20 seconds for incremental
```

### 3. Full Workspace Check (Most Thorough)
```powershell
./scripts/cargo-check-fast.ps1 -Mode full
# Checks everything including tests
# ~15-30 seconds for incremental
```

### 4. Manual One-Liners (if script unavailable)
```powershell
# Super fast lib check
$env:CMAKE_GENERATOR = "Ninja"; cargo check -p hyperstream --lib -j 4 -q

# Quick full check
$env:CMAKE_GENERATOR = "Ninja"; cargo check -p hyperstream -j 4 -q

# With warnings
$env:CMAKE_GENERATOR = "Ninja"; cargo check -p hyperstream -j 4
```

## Environment Variables to Optimize

**Always set these before running cargo:**
```powershell
$env:CMAKE_GENERATOR = "Ninja"      # 2-3x faster linker than MSVC
$env:CARGO_INCREMENTAL = "1"        # Incremental compilation
$env:CARGO_BUILD_JOBS = "4"         # Prevent resource exhaustion on Windows
```

## Troubleshooting Slow Checks

### If check hangs or timeout:
```powershell
# Kill any stuck cargo processes
Get-Process cargo -ErrorAction SilentlyContinue | Stop-Process -Force

# Clean and retry (takes longer but resets state)
cargo clean
./scripts/cargo-check-fast.ps1 -Mode lib -Quiet
```

### If still slow:
1. **Reduce parallel jobs further**:
   ```powershell
   cargo check -p hyperstream -j 2 -q
   ```

2. **Check disk space** (boring-sys2 needs ~500MB temp):
   ```powershell
   Get-Volume | Where-Object DriveLetter -eq 'D' | Select-Object SizeRemaining
   ```

3. **Verify Ninja is installed**:
   ```powershell
   ninja --version
   # Should print version. If not, CMake may fall back to slower MSVC.
   ```

## Performance Benchmarks (on typical Windows machine)

| Check Type | First Build | Incremental | Notes |
|---|---|---|---|
| Full workspace | 3-4 min | 20-30 sec | Includes tests, slowest |
| Default quick | 2-3 min | 10-20 sec | Lib + binaries, recommended |
| Lib only | 1-2 min | 5-15 sec | **Fastest** for quick feedback |
| No changes | N/A | 2-5 sec | Instant validation pass |

## Why Rust Analyzer is Better than `cargo check`

1. **Incremental**: Only re-checks files you changed
2. **Background**: Runs while you code, not blocking
3. **IDE Integration**: Error squiggles appear as you type
4. **Faster**: Aggressive caching, no subprocess overhead

**Setup (if not present)**:
```powershell
# VS Code command palette: Ctrl+Shift+P
# Search: "Extensions: Install Extensions"
# Install: "rust-analyzer" by The Rust Programming Language
```

## Advanced: Cargo Build Profile for Dev

Add to `src-tauri/Cargo.toml` if you want faster debug builds:
```toml
[profile.dev]
opt-level = 1              # Minimal optimization, faster build
lto = false                # Disable link-time optimization
codegen-units = 256        # Parallel codegen

[profile.dev.package."*"]
opt-level = 2              # Keep dependencies optimized
```

This trades runtime speed for build speed during development (you don't run binaries during dev, just check compilation).

## Quick Reference

| Task | Command |
|---|---|
| Quick feedback loop | Use Rust Analyzer in VS Code |
| Fast manual check | `./scripts/cargo-check-fast.ps1 -Quiet` |
| Check before commit | `./scripts/cargo-check-fast.ps1 -Mode full` |
| Debug stuck cargo | `Get-Process cargo \| Stop-Process -Force` |
| Maximize speed | `cargo check -p hyperstream --lib -j 2 -q` |

## Key Insight

**The bottleneck is NOT your Rust code—it's boring-sys2 (TLS library) compiling C++.**

First check: Unavoidable, build the C++ code (~2-3 min)
Incremental: Fast, only recompile changed Rust (~10-30 sec)

If you're seeing full rebuild times on every check → suspect stale lock files or `CMAKE_GENERATOR` not being set.
