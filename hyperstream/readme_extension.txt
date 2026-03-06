# HyperStream Browser Extension Installation Guide

## Installation Steps (Chrome / Edge / Brave)

1. Open your browser and navigate to the Extensions page:
   - Chrome: `chrome://extensions`
   - Edge: `edge://extensions`
   - Brave: `brave://extensions`

2. Enable **Developer Mode** (usually a toggle in the top-right corner).

3. Click the **Load unpacked** button.

4. Select the `browser-extension` folder inside your HyperStream project directory:
   `c:\Users\aditya\Desktop\hdm\hyperstream\browser-extension`

5. The HyperStream extension icon (⚡) should appear in your toolbar.

## Features
- **Intercept Downloads**: Automatically captures downloads and sends them to HyperStream.
- **Context Menu**: Right-click links, images, or videos to "Download with HyperStream".
- **Video Overlay**: Hover over videos on web pages to see a "Download" button.
- **Batch Download**: Right-click on a page -> "Download all links with HyperStream".
- **Active Downloads**: Open the extension popup and click "Refresh active downloads" to see current tasks, including progress and pause/cancel controls.  (requires HyperStream running)
- **Custom Server URL**: If HyperStream is listening on a non-default port or a remote machine, enter its API URL in the popup.
- **Real‑time events**: The extension listens for server-sent events from HyperStream; when a new download is queued (via right‑click or another client) you’ll see a notification and the status list will update automatically. Progress updates are also broadcast so the badge count and list stay in sync with download completion.
- **RSS integration**: Feeds you add in the desktop app are now visible directly inside the popup. Unread counts appear next to each feed, and you can manually refresh any feed. New items trigger a notification and the list updates over SSE.
- **Auto-launch**: Use the   *Install Native Host* button in the app's Advanced settings (or run the `install_native_host` CLI command) to drop a manifest file on your system.  After installing, open the JSON file and add your extension's ID under `allowed_origins` (e.g. `"chrome-extension://<your‑id>/"`).  Once configured, the extension will attempt to launch HyperStream whenever it detects that the server is offline.

---

## Packaging & distribution
You can build a distributable ZIP of either extension variant using the helper script:

```powershell
# from workspace root
./scripts/package-extension.ps1 -Target browser-extension   # or -Target extension
```

The manifest version is used in the filename. Install the ZIP via `chrome://extensions` ("Load unpacked" in dev mode) or publish it to the store.

## Testing (optional)
The codebase doesn’t yet include automated tests for the extension, but you can create Playwright tests that open the popup HTML or load the unpacked extension. Add `@playwright/test` to devDependencies if you need full e2e coverage.

## Troubleshooting
- **"HyperStream not running"**: Ensure the HyperStream desktop application is running. The extension communicates with it via port 14733.
- **Red Status Dot**: Click the extension icon to check connection status.
