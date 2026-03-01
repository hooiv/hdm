# HyperStream — Incomplete / Not Implemented Features

> **Generated**: 2026-03-01
> Scraped from `task.md` (all `[ ]` and `[/]` items) + `advanced_features_analysis.md` (never-started features)

---

## 1. Incomplete Sub-tasks (Inside Otherwise-Completed Features)

These are individual items marked `[ ]` inside phases that are otherwise marked complete.

| Phase | Item | Details |
|-------|------|---------|
| **B1** | TLS/JA3 Fingerprint Spoofing | Marked `[ ]` in Phase B. (Note: JA3 was done differently in Phase I/L via `rquest` and simulated headers — B1 itself was never formally closed.) |
| **N2** | WFP Integration | Explicitly marked "Skipped/Deprioritized". Windows Filtering Platform integration for per-app traffic control. |
| **Y2** | `add_download(url, name)` | Plugin API sub-task. The main `host.downloads.add` works, but the named variant `add_download(url, name)` is still `[ ]`. |
| **Z1** | Custom sound file support | Sound events work with embedded WAVs, but letting users pick their own sound files is not done. |
| **Z3** | Auto-Unrar | Marked `[/]` (partial). Core extraction works but the feature was never fully verified end-to-end in a real multi-part scenario. |
| **G1** | P2P Bandwidth Limiting | Deferred. The P2P file sharing works but has no upload speed cap. |

---

## 2. Build / Tooling Issues

| Issue | Status |
|-------|--------|
| **`tauri-winres` build failure** | Requires MSVC / Visual Studio Build Tools (or Windows SDK). Rust code compiles, but final `.exe` packaging fails. |
| **`settings.rs` ChatOps wiring** | ChatOpsManager created but `chatops.rs` references `crate::downloader::manager::DownloadManager` which may not match actual module path. Needs verification. |

---

## 3. Never-Started Features (from `advanced_features_analysis.md`)

### Tier 1 — Standard IDM Parity (Users Expect These)

| Feature | Priority | Complexity | Est. Time |
|---------|----------|------------|-----------|
| **Refresh Download Address** (URL hot-swap for expired links) | HIGH | MEDIUM | 1 week |
| **Dial-Up/VPN Auto-Connect** | LOW | LOW | 2-3 days |

### Tier 2 — Next-Gen Innovation

| Feature | Priority | Complexity | Est. Time |
|---------|----------|------------|-----------|
| **Cloud-to-Cloud (Rclone Bridge)** | MEDIUM | HIGH | 2-3 weeks |
| **Content Deduplication (CAS)** | LOW | MEDIUM | 1-2 weeks |
| **AI Video Subtitling (Whisper)** | LOW | HIGH | 2-3 weeks |
| **Wayback Machine Integration** | LOW | LOW | 3-4 days |
| **Docker Layer Puller** | LOW | MEDIUM | 1-2 weeks |
| **Containerized Sandbox** | LOW | MEDIUM | 1 week |
| **Virtual Drive Mount** | LOW | VERY HIGH | 4-6 weeks |

### Tier 3 — Specialized / Niche

| Feature | Priority | Complexity | Est. Time |
|---------|----------|------------|-----------|
| **Mod Pack Optimizer** | LOW | MEDIUM | 1 week |
| **IPFS Support** | LOW | HIGH | 2-3 weeks |
| **Per-App QoS (Bandwidth Arbitration)** | LOW | HIGH | 2 weeks |
| **WARC Archiving** | LOW | MEDIUM | 1 week |
| **Geofence Routing (Per-File VPN)** | LOW | VERY HIGH | 3-4 weeks |
| **Smart Sleep (Battery Awareness)** | LOW | LOW | 2-3 days |

### Tier 4 — Cutting-Edge / Experimental

| Feature | Priority | Complexity | Est. Time |
|---------|----------|------------|-----------|
| **Direct-to-USB (ISO Flashing)** | LOW | HIGH | 2 weeks |
| **Blockchain Notarization** | LOW | MEDIUM | 1-2 weeks |
| **API Fuzzer/Replay Tool** | LOW | MEDIUM | 1 week |
| **Smart Home (MQTT)** | LOW | LOW | 2-3 days |
| **Metadata Scrubber** | MEDIUM | LOW | 3-4 days |
| **SQL-over-HTTP (DuckDB)** | LOW | HIGH | 2-3 weeks |
| **Steganographic Vault** | LOW | VERY HIGH | 3-4 weeks |
| **Bandwidth Arbitrage** | LOW | MEDIUM | 1 week |
| **C2PA Validator** | LOW | MEDIUM | 1-2 weeks |
| **Ephemeral Web Server** | LOW | LOW | 2-3 days |
| **DOI Resolver (BibTeX)** | LOW | LOW | 2-3 days |
| **AI Upscaling (Real-ESRGAN)** | LOW | VERY HIGH | 4-5 weeks |
| **Mirror Hunter (Hash Search)** | LOW | MEDIUM | 1-2 weeks |
| **DLNA/AirPlay Casting** | LOW | HIGH | 2-3 weeks |
| **Git LFS Accelerator** | LOW | HIGH | 2-3 weeks |
| **TUI Dashboard** | LOW | MEDIUM | 1-2 weeks |

---

## 4. Quick Wins (High Impact, Low Effort — Not Yet Done)

| Feature | Effort | Impact |
|---------|--------|--------|
| **Metadata Scrubber** | 3 days | ⭐⭐ Privacy |
| **Ephemeral Web Server** | 2 days | ⭐⭐ Sharing |
| **Smart Home (MQTT)** | 2 days | ⭐ IoT |
| **DOI Resolver** | 2 days | ⭐ Academic |
| **Wayback Machine** | 3 days | ⭐⭐ Recovery |
| **Custom Sound Files** | 1 day | ⭐ Polish |
| **P2P Bandwidth Limiting** | 1-2 days | ⭐ P2P |

---

## 5. Summary Counts

| Category | Count |
|----------|-------|
| Incomplete sub-tasks in done phases | **6** |
| Build/tooling issues | **2** |
| Never-started Tier 1 features | **2** |
| Never-started Tier 2 features | **7** |
| Never-started Tier 3 features | **6** |
| Never-started Tier 4 features | **16** |
| **Total unfinished items** | **~39** |
