// HyperStream Browser Extension - Background Script

const HYPERSTREAM_URL = 'http://localhost:14733';

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

// Send download to HyperStream
async function sendToHyperStream(url, filename) {
    try {
        const headers = await getAuthHeaders();
        const response = await fetch(`${HYPERSTREAM_URL}/download`, {
            method: 'POST',
            headers,
            body: JSON.stringify({ url, filename }),
        });

        if (response.status === 401 || response.status === 403) {
            console.warn('Auth token rejected — update token in extension settings');
            return { success: false, message: 'Invalid auth token. Open extension popup to update.' };
        }

        const data = await response.json();
        return data;
    } catch (e) {
        console.error('Failed to send to HyperStream:', e);
        return { success: false, message: e.message };
    }
}

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

    // Cancel the browser download
    chrome.downloads.cancel(downloadItem.id);
    chrome.downloads.erase({ id: downloadItem.id });

    // Send to HyperStream
    const result = await sendToHyperStream(url, filename);

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
                const result = await sendToHyperStream(info.linkUrl, filename);
                if (result.success) showBadge('\u2713');
            }
            break;

        case 'download-video':
            if (info.srcUrl) {
                const filename = info.srcUrl.split('/').pop()?.split('?')[0] || 'video.mp4';
                const result = await sendToHyperStream(info.srcUrl, filename);
                if (result.success) showBadge('\u2713');
            }
            break;

        case 'download-image':
            if (info.srcUrl) {
                const filename = info.srcUrl.split('/').pop()?.split('?')[0] || 'image.jpg';
                const result = await sendToHyperStream(info.srcUrl, filename);
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
        'iso', 'img'
    ];

    const links = document.querySelectorAll('a[href]');
    const downloadableLinks = [];

    for (const link of links) {
        const href = link.href;
        if (!href || href.startsWith('javascript:') || href.startsWith('#')) continue;

        const url = new URL(href, window.location.origin);
        const filename = url.pathname.split('/').pop() || 'download';
        const ext = filename.split('.').pop()?.toLowerCase();

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
        sendToHyperStream(message.url, message.filename)
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
});
