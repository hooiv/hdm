// HyperStream Browser Extension - Background Script

const HYPERSTREAM_URL = 'http://localhost:14733';

// ── Video/Audio Stream Content-Type Detection ────────────────────────
const STREAM_CONTENT_TYPES = {
    hls: [
        'application/vnd.apple.mpegurl',
        'application/x-mpegurl',
        'audio/mpegurl',
        'audio/x-mpegurl',
    ],
    dash: [
        'application/dash+xml',
        'video/vnd.mpeg.dash.mpd',
    ],
    video: [
        'video/mp4',
        'video/webm',
        'video/x-matroska',
        'video/x-msvideo',
        'video/quicktime',
        'video/x-flv',
        'video/3gpp',
        'video/ogg',
    ],
    audio: [
        'audio/mpeg',
        'audio/mp4',
        'audio/ogg',
        'audio/flac',
        'audio/wav',
        'audio/webm',
    ],
};

const STREAM_EXTENSIONS = {
    hls: ['.m3u8'],
    dash: ['.mpd'],
    video: ['.mp4', '.webm', '.mkv', '.avi', '.mov', '.flv', '.3gp', '.ogv'],
    audio: ['.mp3', '.m4a', '.ogg', '.flac', '.wav', '.opus', '.aac'],
};

// Per-tab accumulated stream detections: tabId -> Map<url, stream>
const tabStreams = new Map();

// Minimum size threshold to avoid tracking tiny segments (100KB)
const MIN_STREAM_SIZE = 100 * 1024;

// ── Classify a network response ──────────────────────────────────────
function classifyResponse(url, contentType, contentLength) {
    const ct = (contentType || '').toLowerCase().split(';')[0].trim();
    const lower = url.toLowerCase();

    for (const [type, types] of Object.entries(STREAM_CONTENT_TYPES)) {
        if (types.includes(ct)) {
            return { type, source: 'content-type' };
        }
    }

    // Extension fallback
    try {
        const pathname = new URL(url).pathname.toLowerCase();
        for (const [type, exts] of Object.entries(STREAM_EXTENSIONS)) {
            if (exts.some(ext => pathname.endsWith(ext))) {
                // Skip tiny HLS/DASH segments (.ts files are caught by .m3u8 parent)
                return { type, source: 'extension' };
            }
        }
    } catch (e) { /* invalid URL */ }

    // Known video CDN patterns
    if (/googlevideo\.com\/videoplayback/i.test(url) ||
        /\.fbcdn\.net\/.*video/i.test(url) ||
        /video.*\.twimg\.com/i.test(url) ||
        /\.akamaized\.net\/.*\.m3u8/i.test(url)) {
        return { type: 'video', source: 'cdn-pattern' };
    }

    return null;
}

// ── Store detected stream for a tab ──────────────────────────────────
function addStreamForTab(tabId, stream) {
    if (!tabStreams.has(tabId)) {
        tabStreams.set(tabId, new Map());
    }
    const streams = tabStreams.get(tabId);
    // Deduplicate by URL
    if (!streams.has(stream.url)) {
        streams.set(stream.url, stream);
        // Notify content script to show/update the stream panel
        notifyTab(tabId);
        // Forward to HyperStream app
        reportStreamsToApp(tabId);
    }
}

// ── Notify the content script about updated streams ──────────────────
function notifyTab(tabId) {
    const streams = tabStreams.get(tabId);
    if (!streams) return;
    const list = Array.from(streams.values());
    chrome.tabs.sendMessage(tabId, {
        action: 'streams_updated',
        streams: list,
    }).catch(() => { /* tab may not have content script */ });
}

// ── Report detected streams to HyperStream app ──────────────────────
async function reportStreamsToApp(tabId) {
    const streams = tabStreams.get(tabId);
    if (!streams || streams.size === 0) return;

    const list = Array.from(streams.values());
    try {
        const headers = await getAuthHeaders();
        await fetch(`${HYPERSTREAM_URL}/streams`, {
            method: 'POST',
            headers,
            body: JSON.stringify(list),
        });
    } catch (e) {
        // HyperStream may not be running
    }
}

// ── Clean up when a tab is closed or navigated ──────────────────────
chrome.tabs.onRemoved.addListener((tabId) => {
    tabStreams.delete(tabId);
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
    if (changeInfo.url) {
        // Page navigation — reset stream detections for this tab
        tabStreams.delete(tabId);
    }
});

// ── Network Response Monitoring (Content-Type sniffing) ──────────────
chrome.webRequest.onHeadersReceived.addListener(
    (details) => {
        // Only monitor main_frame, sub_frame, xmlhttprequest, media, other
        // Skip extension/chrome internal requests
        if (details.tabId < 0) return;

        const headers = details.responseHeaders || [];
        let contentType = '';
        let contentLength = 0;
        for (const h of headers) {
            const name = h.name.toLowerCase();
            if (name === 'content-type') contentType = h.value || '';
            if (name === 'content-length') contentLength = parseInt(h.value, 10) || 0;
        }

        const classification = classifyResponse(details.url, contentType, contentLength);
        if (!classification) return;

        // For direct video/audio, skip tiny responses (< 100KB) which are likely thumbnails or previews
        if ((classification.type === 'video' || classification.type === 'audio') && contentLength > 0 && contentLength < MIN_STREAM_SIZE) {
            return;
        }

        // Get page info
        chrome.tabs.get(details.tabId, (tab) => {
            if (chrome.runtime.lastError) return;
            const stream = {
                url: details.url,
                content_type: contentType || null,
                stream_type: classification.type,
                page_url: tab?.url || null,
                page_title: tab?.title || null,
                quality: guessQuality(details.url, contentLength),
                size: contentLength > 0 ? contentLength : null,
            };
            addStreamForTab(details.tabId, stream);
        });
    },
    { urls: ['<all_urls>'] },
    ['responseHeaders']
);

// ── Quality guessing from URL patterns ───────────────────────────────
function guessQuality(url, size) {
    const lower = url.toLowerCase();
    if (/2160|4k|uhd/i.test(lower)) return '4K';
    if (/1080|fullhd|full_hd/i.test(lower)) return '1080p';
    if (/720|hd/i.test(lower)) return '720p';
    if (/480|sd/i.test(lower)) return '480p';
    if (/360/i.test(lower)) return '360p';
    if (/240/i.test(lower)) return '240p';
    if (/144/i.test(lower)) return '144p';
    // Size-based guess for direct videos
    if (size > 500 * 1024 * 1024) return 'HD+';
    if (size > 100 * 1024 * 1024) return 'HD';
    return null;
}

// Get the stored auth token
async function getAuthToken() {
    const { authToken } = await chrome.storage.local.get({ authToken: '' });
    return authToken;
}

// Build headers with auth token
async function getAuthHeaders() {
    const token = await getAuthToken();
    const headers = { 'Content-Type': 'application/json' };
    if (token) {
        headers['X-HyperStream-Token'] = token;
    }
    return headers;
}

// Check if HyperStream is running
async function checkConnection() {
    try {
        const response = await fetch(`${HYPERSTREAM_URL}/health`);
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

// Send download to HyperStream
async function sendToHyperStream(url, filename, requestContext = {}) {
    const context = getRequestContext(url, requestContext);
    // Retry loop with exponential backoff to handle brief outages
    let lastErr;
    for (let attempt = 1; attempt <= 3; attempt++) {
        try {
            const headers = await getAuthHeaders();
            const response = await fetch(`${HYPERSTREAM_URL}/download`, {
                method: 'POST',
                headers,
                body: JSON.stringify({
                    url,
                    filename,
                    customHeaders: context.customHeaders,
                    pageUrl: context.pageUrl,
                    source: context.source,
                }),
            });

            if (response.status === 401 || response.status === 403) {
                console.warn('Auth token rejected — update token in extension settings');
                chrome.notifications.create({
                    type: 'basic',
                    iconUrl: 'icons/icon48.png',
                    title: 'HyperStream error',
                    message: 'Invalid auth token. Update in popup.',
                });
                return { success: false, message: 'Invalid auth token. Open extension popup to update.' };
            }

            const data = await response.json();
            return data;
        } catch (e) {
            lastErr = e;
            console.warn(`attempt ${attempt} failed to reach HyperStream, retrying...`, e);
            if (attempt < 3) {
                await new Promise(res => setTimeout(res, 500 * attempt));
                continue;
            }
        }
    }
    console.error('Failed to send to HyperStream after retries:', lastErr);
    chrome.notifications.create({
        type: 'basic',
        iconUrl: 'icons/icon48.png',
        title: 'HyperStream error',
        message: `Failed to queue download: ${lastErr?.message}`,
    });
    if (url.startsWith('http')) {
        chrome.downloads.download({ url });
    }
    return { success: false, message: lastErr?.message || 'unknown error' };
}

// Periodically update badge to reflect connection status
async function updateConnectionBadge() {
    const connected = await checkConnection();
    chrome.action.setBadgeText({ text: connected ? '' : '!' });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
}
setInterval(updateConnectionBadge, 15000);
updateConnectionBadge();

// Capture browser request headers before browser-managed downloads strip them away.
chrome.webRequest.onBeforeSendHeaders.addListener(
    (details) => {
        if (!details.url || details.url.startsWith(HYPERSTREAM_URL)) {
            return;
        }

        const context = buildCapturedContext(details, 'webRequest');
        rememberRequestContext(details.url, context);

        const lower = details.url.toLowerCase();
        const matches = downloadExtensions.some(ext => lower.endsWith(ext));
        if (matches && !wasRecentlyIntercepted(details.url)) {
            markRecentlyIntercepted(details.url);
            sendToHyperStream(details.url, null, context);
            return { cancel: true };
        }
    },
    { urls: ['<all_urls>'] },
    ['blocking', 'requestHeaders', 'extraHeaders']
);

// WebRequest interception for common file types (blocking rule)
const downloadExtensions = ['.exe', '.msi', '.zip', '.rar', '.7z', '.iso', '.mp4', '.mkv', '.mp3', '.pdf', '.dmg', '.pkg', '.torrent'];

// Listen for download events
chrome.downloads.onCreated.addListener(async (downloadItem) => {
    // Check if extension is enabled
    const { enabled } = await chrome.storage.local.get({ enabled: true });
    if (!enabled) return;

    // Check if we have an auth token
    const token = await getAuthToken();
    if (!token) return; // No token configured — allow default browser download

    // Check if HyperStream is running
    const connected = await checkConnection();
    if (!connected) {
        // HyperStream not running; allow default browser download
        return;
    }

    // Get the URL and filename
    const url = downloadItem.finalUrl || downloadItem.url;
    const filename = downloadItem.filename ? downloadItem.filename.split(/[/\\]/).pop() : null;
    if (wasRecentlyIntercepted(url)) {
        return;
    }

    // Cancel the browser download
    chrome.downloads.cancel(downloadItem.id);
    chrome.downloads.erase({ id: downloadItem.id });

    // Send to HyperStream
    const result = await sendToHyperStream(
        url,
        filename,
        buildPageContext(downloadItem.referrer || null, 'downloadsApi')
    );

    if (result.success) {
        // Show notification
        showBadge('\u2713');
    } else {
        console.warn('Failed to send to HyperStream:', result.message);
        // Fallback: restart the download in browser
        chrome.downloads.download({ url });
    }
});

// Create context menu
chrome.runtime.onInstalled.addListener(() => {
    chrome.storage.local.set({ enabled: true });

    // Context menu for downloading links
    chrome.contextMenus.create({
        id: 'download-link',
        title: 'Download with HyperStream',
        contexts: ['link']
    });

    // Context menu for downloading all links on page
    chrome.contextMenus.create({
        id: 'download-all-links',
        title: 'Download all links with HyperStream',
        contexts: ['page']
    });

    // Context menu for videos
    chrome.contextMenus.create({
        id: 'download-video',
        title: 'Download video with HyperStream',
        contexts: ['video']
    });

    // Context menu for images
    chrome.contextMenus.create({
        id: 'download-image',
        title: 'Download image with HyperStream',
        contexts: ['image']
    });
});

// Handle context menu clicks
chrome.contextMenus.onClicked.addListener(async (info, tab) => {
    const connected = await checkConnection();

    if (!connected) {
        return;
    }

    switch (info.menuItemId) {
        case 'download-link':
            if (info.linkUrl) {
                const filename = info.linkUrl.split('/').pop()?.split('?')[0] || 'download';
                const result = await sendToHyperStream(info.linkUrl, filename, buildPageContext(info.pageUrl || tab?.url || null, 'contextMenu'));
                if (result.success) showBadge('\u2713');
            }
            break;

        case 'download-video':
            if (info.srcUrl) {
                const filename = info.srcUrl.split('/').pop()?.split('?')[0] || 'video.mp4';
                const result = await sendToHyperStream(info.srcUrl, filename, buildPageContext(info.pageUrl || tab?.url || null, 'contextMenu'));
                if (result.success) showBadge('\u2713');
            }
            break;

        case 'download-image':
            if (info.srcUrl) {
                const filename = info.srcUrl.split('/').pop()?.split('?')[0] || 'image.jpg';
                const result = await sendToHyperStream(info.srcUrl, filename, buildPageContext(info.pageUrl || tab?.url || null, 'contextMenu'));
                if (result.success) showBadge('\u2713');
            }
            break;

        case 'download-all-links':
            chrome.scripting.executeScript({
                target: { tabId: tab.id },
                function: gatherDownloadableLinks
            }).then(async (results) => {
                if (results && results[0] && results[0].result) {
                    const links = results[0].result;

                    // Send all links as batch to HyperStream for user review
                    try {
                        const headers = await getAuthHeaders();
                        const response = await fetch(`${HYPERSTREAM_URL}/batch`, {
                            method: 'POST',
                            headers,
                            body: JSON.stringify(links.map(l => ({
                                url: l.url,
                                filename: l.filename
                            })))
                        });

                        if (response.ok) {
                            showBadge(links.length.toString());
                        }
                    } catch (e) {
                        console.warn('Failed to send batch to HyperStream:', e);
                    }
                }
            });
            break;
    }
});

// Function to gather downloadable links (injected into page)
function gatherDownloadableLinks() {
    const downloadExtensions = [
        'zip', 'rar', '7z', 'tar', 'gz',
        'exe', 'msi', 'dmg', 'pkg',
        'pdf', 'doc', 'docx', 'xls', 'xlsx',
        'mp4', 'mkv', 'avi', 'mov', 'webm',
        'mp3', 'flac', 'wav', 'aac',
        'iso', 'img',
        'torrent',
        'm3u8', 'ts'
    ];

    const links = document.querySelectorAll('a[href]');
    const downloadableLinks = [];

    for (const link of links) {
        const href = link.href;
        if (!href || href.startsWith('javascript:') || href.startsWith('#')) continue;

        const url = new URL(href, window.location.origin);
        const filename = url.pathname.split('/').pop() || 'download';
        let ext = filename.split('.').pop()?.toLowerCase();
        if ((!ext || ext === '') && filename.includes('?')) {
            ext = filename.split('?')[0].split('.').pop()?.toLowerCase();
        }

        if (ext && downloadExtensions.includes(ext)) {
            downloadableLinks.push({
                url: href,
                filename: filename
            });
        }
    }

    return downloadableLinks;
}

// Show badge helper
function showBadge(text) {
    chrome.action.setBadgeText({ text });
    chrome.action.setBadgeBackgroundColor({ color: '#22c55e' });
    setTimeout(() => chrome.action.setBadgeText({ text: '' }), 2000);
}

// Listen for messages from content script or popup
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.action === 'download') {
        sendToHyperStream(message.url, message.filename, buildPageContext(sender.tab?.url || message.pageUrl || null, 'runtimeMessage'))
            .then(result => sendResponse(result));
        return true; // Keep message channel open for async response
    }

    if (message.action === 'checkConnection') {
        checkConnection().then(connected => sendResponse({ connected }));
        return true;
    }

    if (message.action === 'setAuthToken') {
        chrome.storage.local.set({ authToken: message.token });
        sendResponse({ success: true });
        return false;
    }

    if (message.action === 'getAuthToken') {
        getAuthToken().then(token => sendResponse({ token }));
        return true;
    }

    if (message.action === 'getStreams') {
        // Content script or popup requesting current tab streams
        const tabId = sender.tab?.id || message.tabId;
        const streams = tabStreams.get(tabId);
        sendResponse({ streams: streams ? Array.from(streams.values()) : [] });
        return false;
    }

    if (message.action === 'downloadStream') {
        // Download a specific detected stream
        const stream = message.stream;
        if (stream && stream.url) {
            let filename = null;
            try {
                const pathname = new URL(stream.url).pathname;
                filename = pathname.split('/').pop() || null;
            } catch (e) { /* ignore */ }
            sendToHyperStream(stream.url, filename, buildPageContext(stream.page_url || sender.tab?.url || null, 'streamDetector'))
                .then(result => sendResponse(result));
            return true;
        }
        sendResponse({ success: false, message: 'No stream URL' });
        return false;
    }

    if (message.action === 'scanPageForStreams') {
        // Request the backend to scan a page URL for streams
        const tabId = sender.tab?.id || message.tabId;
        (async () => {
            try {
                const result = await sendToHyperStream(
                    message.pageUrl,
                    null,
                    buildPageContext(sender.tab?.url || message.pageUrl || null, 'pageScan')
                );
                sendResponse({ success: Boolean(result?.success), message: result?.message });
            } catch (e) {
                sendResponse({ success: false, message: e.message });
            }
        })();
        return true;
    }
});
