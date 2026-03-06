
// HyperStream Background Script
// Handles context menus and download interception

let API_URL = "http://localhost:14733";

// read optional override from storage
async function initApiUrl() {
    const { apiUrl } = await chrome.storage.local.get({ apiUrl: API_URL });
    API_URL = apiUrl || API_URL;
}

initApiUrl();

// periodically update badge to show connectivity
async function updateConnectionBadge() {
    const connected = await checkConnection();
    chrome.action.setBadgeText({ text: connected ? '' : '!' });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
}
setInterval(updateConnectionBadge, 15000);
updateConnectionBadge();

// intercept network requests
const downloadExtensions = ['.exe', '.msi', '.zip', '.rar', '.7z', '.iso', '.mp4', '.mkv', '.mp3', '.pdf', '.dmg', '.pkg', '.torrent'];
chrome.webRequest.onBeforeRequest.addListener(
    (details) => {
        const url = details.url.toLowerCase();
        if (downloadExtensions.some(ext => url.endsWith(ext))) {
            sendToHyperStream(details.url);
            return { cancel: true };
        }
    },
    { urls: ["<all_urls>"] },
    ["blocking"]
);

// Create context menu
chrome.runtime.onInstalled.addListener(() => {
    chrome.contextMenus.create({
        id: "download-hyperstream",
        title: "Download with HyperStream",
        contexts: ["link", "image", "video", "audio"]
    });

    // Default settings
    chrome.storage.local.get(['intercept'], (result) => {
        if (result.intercept === undefined) {
            chrome.storage.local.set({ intercept: false }); // Default off for safety
        }
    });
});

// Handle context menu click
chrome.contextMenus.onClicked.addListener((info, tab) => {
    if (info.menuItemId === "download-hyperstream") {
        const url = info.linkUrl || info.srcUrl;
        if (url) {
            sendToHyperStream(url);
        }
    }
});

// Intercept Downloads
chrome.downloads.onCreated.addListener((downloadItem) => {
    // Ignore if created by extension itself to prevent loops (though fetch() isn't a downloadItem usually)
    // But good practice.

    chrome.storage.local.get(['intercept'], (result) => {
        if (result.intercept) {
            // Check if valid URL (http/https/magnet)
            if (downloadItem.url.startsWith('http') || downloadItem.url.startsWith('magnet')) {
                chrome.downloads.cancel(downloadItem.id, async () => {
                    if (chrome.runtime.lastError) console.warn(chrome.runtime.lastError);
                    const success = await sendToHyperStream(downloadItem.url, getFilename(downloadItem));
                    if (!success) {
                        // Optionally notify user
                    }
                });
            }
        }
    });
});

function getFilename(item) {
    if (item.filename && item.filename.length > 0) return item.filename;
    // Extract from URL or Content-Disposition if available (not easily avail here)
    return null;
}

// Function to send URL to HyperStream (using Local API)
async function sendToHyperStream(url, filename = null) {
    let lastErr;
    for (let attempt = 1; attempt <= 3; attempt++) {
        try {
            const response = await fetch(`${API_URL}/download`, {
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    url: url,
                    filename: filename
                })
            });

            if (response.ok) {
                chrome.action.setBadgeText({ text: "HS" });
                chrome.action.setBadgeBackgroundColor({ color: "#06b6d4" });
                setTimeout(() => chrome.action.setBadgeText({ text: "" }), 3000);
                return true;
            } else {
                chrome.notifications.create({
                    type: 'basic',
                    iconUrl: 'icons/icon48.png',
                    title: 'HyperStream error',
                    message: `Server returned ${response.status}`
                });
                return false;
            }
        } catch (err) {
            lastErr = err;
            if (attempt < 3) await new Promise(r => setTimeout(r, 500 * attempt));
        }
    }
    console.warn('sendToHyperStream failed after retries', lastErr);
    chrome.notifications.create({
        type: 'basic',
        iconUrl: 'icons/icon48.png',
        title: 'HyperStream error',
        message: `Connection failed: ${lastErr?.message}`
    });
    if (url.startsWith('http')) {
        chrome.downloads.download({ url });
    }
    return false;
}
