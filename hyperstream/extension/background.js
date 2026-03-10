
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

const REQUEST_CONTEXT_TTL_MS = 2 * 60 * 1000;
const INTERCEPT_DEDUPE_TTL_MS = 15 * 1000;
const recentRequestContext = new Map();
const recentIntercepts = new Map();
const FORWARDABLE_REQUEST_HEADERS = new Set([
    'authorization',
    'cookie',
    'origin',
    'referer',
    'user-agent',
    'accept',
    'accept-language',
]);

function normalizeTrackedUrl(url) {
    try {
        const parsed = new URL(url);
        parsed.hash = '';
        return parsed.toString();
    } catch {
        return url;
    }
}

function pruneExpiredEntries(store, ttlMs) {
    const now = Date.now();
    for (const [key, value] of store.entries()) {
        if ((value.timestamp || 0) + ttlMs < now) {
            store.delete(key);
        }
    }
}

function canonicalizeHeaderName(name) {
    return name
        .split('-')
        .map(part => part ? part[0].toUpperCase() + part.slice(1).toLowerCase() : part)
        .join('-');
}

function isForwardableHeader(name) {
    return FORWARDABLE_REQUEST_HEADERS.has(name) || name.startsWith('x-');
}

function buildForwardHeaders(requestHeaders = []) {
    const forwarded = {};
    for (const header of requestHeaders) {
        const rawName = (header?.name || '').trim();
        const lowerName = rawName.toLowerCase();
        const value = typeof header?.value === 'string' ? header.value.trim() : '';
        if (!lowerName || !value || !isForwardableHeader(lowerName)) continue;
        forwarded[canonicalizeHeaderName(lowerName)] = value;
    }
    return forwarded;
}

function mergeRequestContext(primary = {}, fallback = {}) {
    const customHeaders = {
        ...(fallback.customHeaders || {}),
        ...(primary.customHeaders || {}),
    };
    const pageUrl = primary.pageUrl || fallback.pageUrl || null;
    if (pageUrl && !Object.keys(customHeaders).some((key) => key.toLowerCase() === 'referer')) {
        customHeaders.Referer = pageUrl;
    }

    return {
        customHeaders: Object.keys(customHeaders).length > 0 ? customHeaders : null,
        pageUrl,
        source: primary.source || fallback.source || null,
    };
}

function rememberRequestContext(url, context = {}) {
    pruneExpiredEntries(recentRequestContext, REQUEST_CONTEXT_TTL_MS);
    recentRequestContext.set(normalizeTrackedUrl(url), {
        ...mergeRequestContext(context),
        timestamp: Date.now(),
    });
}

function getRequestContext(url, fallback = {}) {
    pruneExpiredEntries(recentRequestContext, REQUEST_CONTEXT_TTL_MS);
    const stored = recentRequestContext.get(normalizeTrackedUrl(url));
    return mergeRequestContext(stored || {}, fallback);
}

function markRecentlyIntercepted(url) {
    pruneExpiredEntries(recentIntercepts, INTERCEPT_DEDUPE_TTL_MS);
    recentIntercepts.set(normalizeTrackedUrl(url), { timestamp: Date.now() });
}

function wasRecentlyIntercepted(url) {
    pruneExpiredEntries(recentIntercepts, INTERCEPT_DEDUPE_TTL_MS);
    return recentIntercepts.has(normalizeTrackedUrl(url));
}

function buildPageContext(pageUrl, source) {
    return mergeRequestContext({
        customHeaders: pageUrl ? { Referer: pageUrl } : null,
        pageUrl: pageUrl || null,
        source,
    });
}

function buildCapturedContext(details, source) {
    return mergeRequestContext({
        customHeaders: buildForwardHeaders(details.requestHeaders || []),
        pageUrl: details.initiator || details.documentUrl || null,
        source,
    });
}

// periodically update badge to show connectivity
async function updateConnectionBadge() {
    const connected = await checkConnection();
    chrome.action.setBadgeText({ text: connected ? '' : '!' });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
}
setInterval(updateConnectionBadge, 15000);
updateConnectionBadge();

const downloadExtensions = ['.exe', '.msi', '.zip', '.rar', '.7z', '.iso', '.mp4', '.mkv', '.mp3', '.pdf', '.dmg', '.pkg', '.torrent'];
// intercept network requests after headers are available so auth/referrer survive handoff
chrome.webRequest.onBeforeSendHeaders.addListener(
    (details) => {
        if (!INTERCEPT_ENABLED || !AUTH_TOKEN || !details.url || details.url.startsWith(API_URL)) {
            return;
        }

        const context = buildCapturedContext(details, 'webRequest');
        rememberRequestContext(details.url, context);

        const url = details.url.toLowerCase();
        if (downloadExtensions.some(ext => url.endsWith(ext)) && !wasRecentlyIntercepted(details.url)) {
            markRecentlyIntercepted(details.url);
            sendToHyperStream(details.url, null, context, true);
            return { cancel: true };
        }
    },
    { urls: ["<all_urls>"] },
    ["blocking", "requestHeaders", "extraHeaders"]
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
            sendToHyperStream(url, null, buildPageContext(info.pageUrl || tab?.url || null, 'contextMenu'));
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
        if (wasRecentlyIntercepted(downloadItem.url)) {
            return;
        }
        chrome.downloads.cancel(downloadItem.id, async () => {
            if (chrome.runtime.lastError) console.warn(chrome.runtime.lastError);
            await sendToHyperStream(
                downloadItem.url,
                getFilename(downloadItem),
                buildPageContext(downloadItem.referrer || null, 'downloadsApi'),
                true
            );
        });
    }
});

function getFilename(item) {
    if (item.filename && item.filename.length > 0) return item.filename;
    // Extract from URL or Content-Disposition if available (not easily avail here)
    return null;
}

// Function to send URL to HyperStream (using Local API)
async function sendToHyperStream(url, filename = null, requestContext = {}, allowBrowserFallback = false) {
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

    const context = getRequestContext(url, requestContext);

    let lastErr;
    for (let attempt = 1; attempt <= 3; attempt++) {
        try {
            const response = await fetch(`${API_URL}/download`, {
                method: "POST",
                headers: buildAuthHeaders(AUTH_TOKEN),
                body: JSON.stringify({
                    url,
                    filename,
                    customHeaders: context.customHeaders,
                    pageUrl: context.pageUrl,
                    source: context.source,
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
