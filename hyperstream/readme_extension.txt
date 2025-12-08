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

## Troubleshooting
- **"HyperStream not running"**: Ensure the HyperStream desktop application is running. The extension communicates with it via port 14733.
- **Red Status Dot**: Click the extension icon to check connection status.
