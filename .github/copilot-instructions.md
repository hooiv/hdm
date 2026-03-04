# HyperStream — Copilot Instructions

## Architecture Overview

**Tauri v2 desktop app**: Rust backend (`src-tauri/src/`) + React 19 frontend (`src/`) communicating via Tauri IPC. The app is a feature-rich download manager with multi-segment downloading, torrents, P2P sharing, browser extension integration, and 40+ feature modules.

### Backend (Rust)

- **Entry point**: `main.rs` → `lib.rs` — the central hub declaring all modules, `#[tauri::command]` functions, and the Tauri Builder setup
- **Core download flow**: `engine/session.rs::start_download_impl()` orchestrates the entire download lifecycle — HTTP client setup, segment initialization, worker spawning, progress monitoring, and completion side-effects (webhooks, MQTT, ChatOps, sounds, auto-extract)
- **Download primitives**: `downloader/` — `manager.rs` (segment tracking + work-stealing), `disk.rs` (async write coalescing), `network.rs` (retry strategies), `http_client.rs` ("First Byte Scout" capability detection)
- **Shared state**: `core_state.rs::AppState` — held via `Mutex<HashMap>` for downloads, `Arc` wrappers for P2P/torrent/chatops managers
- **Settings**: `settings.rs` — loaded from disk JSON on every access (no in-memory cache). Uses `#[serde(default)]` extensively for forward-compatible deserialization
- **Persistence**: `persistence.rs` — flat `downloads.json` with `Vec<SavedDownload>`, simple upsert/remove operations
- **Feature modules**: ~40 single-file modules (e.g., `feeds.rs`, `spider.rs`, `cloud_bridge.rs`, `mqtt_client.rs`) each exposing `#[tauri::command]` functions

### Frontend (React + TypeScript + Tailwind)

- **Root**: `App.tsx` — monolithic component owning all state via `useState`/`useRef` (no external state library)
- **Layout**: Tab-based navigation (`downloads | torrents | feeds | search | plugins`) via `Layout.tsx` sidebar
- **Modals**: Lazy-loaded (`React.lazy`) with `isOpen`/`onClose` props and `AnimatePresence` animations
- **Settings**: Decomposed into `components/settings/{GeneralTab,NetworkTab,CloudTab,...}.tsx`

## Key Conventions

### Tauri Command Pattern
All IPC commands use `Result<T, String>` — errors are plain strings via `.map_err(|e| e.to_string())`. Commands are registered in a single `tauri::generate_handler![...]` block at the bottom of `lib.rs` (~150+ commands). When adding a new command:
1. Define `#[tauri::command]` fn in `lib.rs` or `commands.rs`
2. Add the function name to the `generate_handler![]` macro invocation
3. Accept `State<'_, AppState>` for shared state, `AppHandle` for emitting events

### Frontend-Backend Communication
- **Commands**: `invoke("command_name", { args })` from `@tauri-apps/api/core`. A typed wrapper exists at `src/api/commands.ts` but is not consistently used — many components call `invoke` directly
- **Events**: Backend emits via `app.emit("event_name", payload)`. Frontend listens in `useEffect` with `listen<PayloadType>()`. Key events: `download_progress`, `extension_download`, `clipboard_url`, `batch_links`
- **Progress**: Emitted at ~30fps as `SlimSegment` tuples `(id, start, end, cursor, state, speed)` — deliberately compact for serialization performance

### Styling
- **Dark-only** glassmorphism design: `backdrop-blur-xl`, translucent backgrounds (`bg-cyan-500/10`), Tailwind utility classes inline
- Icons: `lucide-react`. Animations: `framer-motion` everywhere
- Fonts: Outfit (headings), JetBrains Mono (code), Inter (body) — loaded in `index.css`
- Custom CSS variables in `App.css` (`--bg-dark`, `--text-main`); custom scrollbar styles

### Rust Patterns
- **Concurrency**: `Arc<Mutex<T>>` for shared state, `tokio::sync::broadcast` for stop signals, `std::sync::mpsc` for disk writes, `tokio::select!` for cancellation
- **Static globals**: `GLOBAL_LIMITER` (speed), `CLIPBOARD_MONITOR`, `AUDIO_PLAYER` — lazy_static singletons for cross-cutting concerns
- **Network evasion**: `network/masq.rs` (Chrome impersonation), `network/sni_fragment.rs` (DPI bypass), `network/tor.rs`, `network/tls_ja3.rs`

## Build & Dev

```bash
# Install JS dependencies (run once or after package.json changes)
cd hyperstream && npm install

# Frontend-only dev server (port 1420, HMR on 1421) — no Rust compilation
cd hyperstream && npm run dev

# Full Tauri dev — compiles Rust + launches app with hot-reload frontend
# Note: args must be passed after `--` because the npm script is named "tauri"
cd hyperstream && npm run tauri -- dev

# Production build (first run ~20 min; outputs MSI + NSIS installer)
# Output: src-tauri/target/release/bundle/{msi,nsis}/
$env:CMAKE_GENERATOR = "Ninja"; cd hyperstream; npm run tauri -- build
```

> 🔧 **Developer tip:** during the Vite build you may see a warning about 
> `baseline-browser-mapping` being stale. Update it with:
> ```bash
> cd hyperstream && npm i baseline-browser-mapping@latest -D
> ```
> to silence the warning.


**Windows prerequisites**:
- MSVC / Visual Studio Build Tools (C++ workload) — required by `tauri-winres` and `boring-sys2`
- CMake ≥ 3.18 and Ninja (already installed) — use Ninja as the generator to avoid MSVC version-detection bugs in the `cmake` crate
- **`CMAKE_GENERATOR = "Ninja"` must be set** before every `cargo` / `npm run tauri` invocation; without it `boring-sys2` fails to build
  - a convenience PowerShell wrapper is available at `hyperstream/scripts/build-windows.ps1` (`./scripts/build-windows.ps1`), which sets this variable for you.

**`boring-sys2` CRT fix**: five packages — `boring-sys2`, `boring2`, `tokio-boring2`, `boring-sys`, and `boring` — are all pinned to `opt-level = 3` in `[profile.dev.package.*]` inside `src-tauri/Cargo.toml` so they link against the release CRT (`msvcrt`) and avoid a `msvcrtd`/`msvcrt` mismatch at link time.

## Key Files to Read First

| Purpose | File |
|---|---|
| All shared state & types | `src-tauri/src/core_state.rs` |
| Download lifecycle | `src-tauri/src/engine/session.rs` |
| Segment management | `src-tauri/src/downloader/manager.rs` |
| Command registration | bottom of `src-tauri/src/lib.rs` (~line 1554) |
| Frontend types | `src/types/index.ts` |
| Typed API wrapper | `src/api/commands.ts` |
| App state & event wiring | `src/App.tsx` |
| Settings schema | `src/components/settings/types.ts` |

## MCP Tools

Use MCP (Model Context Protocol) tools as much as possible when available. Prefer MCP tools over manual approaches for tasks like:
- **Documentation lookup**: Use `context7` or `deepwiki` MCP tools to fetch up-to-date library docs (Tauri v2, tokio, serde, framer-motion, etc.) instead of relying on training data
- **Web research**: Use `firecrawl` or `fetch_webpage` for scraping docs, examples, or API references
- **Browser automation**: Use `playwright` MCP tools for testing or interacting with the app
- **Memory**: Use `memory` MCP tools to persist context across sessions (architecture decisions, known issues, user preferences)

## Post-Edit Code Review

After completing any file edits, **always perform a code review** before finishing. This is mandatory. The review must:
1. Re-read every changed file and verify correctness — no leftover placeholders, no broken imports, no syntax errors
2. Check cross-file consistency — if you added a Tauri command, confirm it's in `generate_handler![]`; if you added a type, confirm frontend types match
3. Run `get_errors` on all modified files to catch compile/lint issues
4. Verify the change aligns with existing patterns (error handling as `Result<T, String>`, `#[serde(default)]` on new Settings fields, etc.)
5. Summarize what was changed and flag any risks or follow-up items

## Gotchas

- `lib.rs` and `commands.rs` together contain ~3900 lines of command definitions — search by function name, don't scroll
- Settings are re-read from disk on every `load_settings()` call — there is no in-memory cache
- The `engine/` directory contains both production code (`session.rs`) and partially-implemented next-gen abstractions (`intent.rs`, `swarm.rs`, `materializer.rs`)
- Utility functions like `formatBytes` are duplicated across frontend components rather than shared
- Browser extension code lives in two directories: `browser-extension/` (Chrome) and `extension/` (alternate)
