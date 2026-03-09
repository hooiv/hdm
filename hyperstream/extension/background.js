
// HyperStream Background Script
// Handles context menus and download interception

const DEFAULT_API_URL = "http://localhost:14733";
let API_URL = DEFAULT_API_URL;
let AUTH_TOKEN = "";
let INTERCEPT_ENABLED = false;

function buildAuthHeaders(token) {
    const headers = { "Content-Type": "application/json" };
    if (token) {
        headers["X-HyperStream-Token"] = token;
    }
    return headers;
}

async function initSettings() {
    const { apiUrl, authToken, intercept } = await chrome.storage.local.get({
        apiUrl: DEFAULT_API_URL,
        authToken: '',
        intercept: false,
    });
    API_URL = apiUrl || DEFAULT_API_URL;
    AUTH_TOKEN = (authToken || '').trim();
    INTERCEPT_ENABLED = Boolean(intercept);
}

chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== 'local') return;
    if (changes.apiUrl) {
        API_URL = changes.apiUrl.newValue || DEFAULT_API_URL;
    }
    if (changes.authToken) {
        AUTH_TOKEN = (changes.authToken.newValue || '').trim();
    }
    if (changes.intercept) {
        INTERCEPT_ENABLED = Boolean(changes.intercept.newValue);
    }
});

initSettings();

async function checkConnection() {
    try {
        const response = await fetch(`${API_URL}/health`);
        if (!response.ok) return false;
        const data = await response.json();
        return data.status === 'ok';
    } catch (e) {
        return false;
    }
}

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
        if (!INTERCEPT_ENABLED || !AUTH_TOKEN) {
            return;
        }
        const url = details.url.toLowerCase();
        if (downloadExtensions.some(ext => url.endsWith(ext))) {
            sendToHyperStream(details.url, null, true);
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
    if (!INTERCEPT_ENABLED || !AUTH_TOKEN) {
        return;
    }

    // Check if valid URL (http/https/magnet)
    if (downloadItem.url.startsWith('http') || downloadItem.url.startsWith('magnet')) {
        chrome.downloads.cancel(downloadItem.id, async () => {
            if (chrome.runtime.lastError) console.warn(chrome.runtime.lastError);
            await sendToHyperStream(downloadItem.url, getFilename(downloadItem), true);
        });
    }
});

function getFilename(item) {
    if (item.filename && item.filename.length > 0) return item.filename;
    // Extract from URL or Content-Disposition if available (not easily avail here)
    return null;
}

// Function to send URL to HyperStream (using Local API)
async function sendToHyperStream(url, filename = null, allowBrowserFallback = false) {
    if (!AUTH_TOKEN) {
        chrome.notifications.create({
            type: 'basic',
            iconUrl: 'icons/icon48.png',
            title: 'HyperStream token required',
            message: 'Paste the Browser Extension Token into the HyperStream extension popup.'
        });
        if (allowBrowserFallback && url.startsWith('http')) {
            chrome.downloads.download({ url });
        }
        return false;
    }

    let lastErr;
    for (let attempt = 1; attempt <= 3; attempt++) {
        try {
            const response = await fetch(`${API_URL}/download`, {
                method: "POST",
                headers: buildAuthHeaders(AUTH_TOKEN),
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
                const message = response.status === 401
                    ? 'Stored extension token was rejected by HyperStream'
                    : `Server returned ${response.status}`;
                chrome.notifications.create({
                    type: 'basic',
                    iconUrl: 'icons/icon48.png',
                    title: 'HyperStream error',
                    message
                });
                if (allowBrowserFallback && url.startsWith('http')) {
                    chrome.downloads.download({ url });
                }
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
    if (allowBrowserFallback && url.startsWith('http')) {
        chrome.downloads.download({ url });
    }
    return false;
}
