# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## RSS Feeds Feature

HyperStream includes a full-featured RSS reader tightly integrated with the download engine:

- Add, edit or remove feeds with custom names and refresh intervals.
- Automatic background polling every minute; new items appear immediately and a toast/OS notification is shown.
- Unread‑count badges on the sidebar and persistent item history stored in `feeds.json` and `feed_items.json`.
- Manual refresh, mark items as read, and one‑click download links from feed entries.
- Optional regex-based auto‑download rules with built-in ReDoS-safe validation.
- Full HTTP API (protected by the shared secret token) exposes endpoints to list feeds, view items, add/update/remove feeds, and request a manual refresh — enabling remote control from extensions or scripts.
- Secure fetching with SSRF protections, redirect limits, size caps and IP blacklist.
- Configurable per-feed enable/disable toggle and interval settings.

The implementation is backed by `src-tauri/src/feeds.rs` and `src/components/FeedsTab.tsx`; see those files for further details and unit tests (`fetch_feed` and `FeedManager`).
