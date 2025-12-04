// HyperStream Browser Extension - Background Script

const HYPERSTREAM_URL = 'http://localhost:9876';

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
        const response = await fetch(`${HYPERSTREAM_URL}/download`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ url, filename }),
        });
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

    // Check if HyperStream is running
    const connected = await checkConnection();
    if (!connected) {
        console.log('HyperStream not running, allowing browser download');
        return;
    }

    // Get the URL and filename
    const url = downloadItem.finalUrl || downloadItem.url;
    const filename = downloadItem.filename ? downloadItem.filename.split(/[/\\]/).pop() : null;

    console.log('Intercepting download:', url);

    // Cancel the browser download
    chrome.downloads.cancel(downloadItem.id);
    chrome.downloads.erase({ id: downloadItem.id });

    // Send to HyperStream
    const result = await sendToHyperStream(url, filename);

    if (result.success) {
        // Show notification
        chrome.action.setBadgeText({ text: '✓' });
        chrome.action.setBadgeBackgroundColor({ color: '#22c55e' });
        setTimeout(() => chrome.action.setBadgeText({ text: '' }), 2000);
    } else {
        console.error('Failed to send to HyperStream:', result.message);
        // Fallback: restart the download in browser
        chrome.downloads.download({ url });
    }
});

// Initialize
chrome.runtime.onInstalled.addListener(() => {
    chrome.storage.local.set({ enabled: true });
    console.log('HyperStream extension installed');
});
