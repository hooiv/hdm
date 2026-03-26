# HyperStream Build Scripts

## cargo-check-fast.ps1

Fast Rust compilation for Windows development, optimized for Tauri + boring-sys2.

### Quick Start
```powershell
cd d:\hdm\hyperstream
./scripts/cargo-check-fast.ps1              # Quick check (lib + bins, ~10-20s)
./scripts/cargo-check-fast.ps1 -Quiet       # Suppress warnings
./scripts/cargo-check-fast.ps1 -Mode lib    # Lib only (~5-15s)
./scripts/cargo-check-fast.ps1 -Mode full   # Full workspace with tests
```

### What It Does
- ✅ Sets `CMAKE_GENERATOR = "Ninja"` (2-3x faster linker)
- ✅ Limits jobs to 4 (prevents Windows resource exhaustion)
- ✅ Enables incremental compilation
- ✅ Skips unnecessary rebuilds
- ✅ Reports completion time

### Performance
- **First check** (after `cargo clean`): 2-3 minutes
- **Incremental** (normal): 10-20 seconds
- **Lib only** (fastest): 5-15 seconds

### Options
| Mode | Target | Use Case |
|---|---|---|
| `quick` | lib + binaries | Default, balanced |
| `lib` | lib only | Fastest feedback |
| `tests` | lib + test targets | Before running tests |
| `full` | workspace + tests | Before commits |

### Troubleshooting
If check hangs:
```powershell
Get-Process cargo -ErrorAction SilentlyContinue | Stop-Process -Force
./scripts/cargo-check-fast.ps1
```

For more details, see [`CARGO_CHECK_OPTIMIZATION.md`](/CARGO_CHECK_OPTIMIZATION.md)
