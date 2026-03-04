
// HyperStream Background Script
// Handles context menus and download interception

const API_URL = "http://localhost:14733";

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
                // Pause immediately
                // We cannot "prevent" creation, but we can cancel.
                chrome.downloads.cancel(downloadItem.id, async () => {
                    if (chrome.runtime.lastError) console.warn(chrome.runtime.lastError);

                    // Send to HyperStream
                    const success = await sendToHyperStream(downloadItem.url, getFilename(downloadItem));

                    if (!success) {
                        // Fallback: If failed, we could re-download, but that's tricky since we just cancelled.
                        // Ideally we warn user.
                        // HyperStream failed; user must download manually
                        // Optional: Create a notification?
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
            // sent to HyperStream
            // Badge
            chrome.action.setBadgeText({ text: "HS" });
            chrome.action.setBadgeBackgroundColor({ color: "#06b6d4" });
            setTimeout(() => chrome.action.setBadgeText({ text: "" }), 3000);
            return true;
        } else {
            // hyperstream returned error
            return false;
        }
    } catch (error) {
        // hyperstream connection error
        return false;
    }
}
