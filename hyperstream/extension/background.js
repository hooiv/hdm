// Create context menu
chrome.runtime.onInstalled.addListener(() => {
    chrome.contextMenus.create({
        id: "download-hyperstream",
        title: "Download with HyperStream",
        contexts: ["link", "image", "video", "audio"]
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

// Function to send URL to HyperStream (using Local API)
async function sendToHyperStream(url) {
    try {
        const response = await fetch("http://localhost:14733/download", {
            method: "POST",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify({
                url: url,
                filename: null // Backend will deduce or ask
            })
        });

        if (response.ok) {
            showNotification("Download started in HyperStream");
        } else {
            showNotification("Failed to start download. Is HyperStream running?");
        }
    } catch (error) {
        console.error("HyperStream Error:", error);
        showNotification("Connection failed. Is HyperStream running?");
    }
}

function showNotification(message) {
    // In MV3 service workers, we can't use alert().
    // We could use chrome.notifications if we had permission, or just console log.
    // For now, assume user checks the app.
    console.log(message);

    // Optional: Badge text
    chrome.action.setBadgeText({ text: "OK" });
    setTimeout(() => chrome.action.setBadgeText({ text: "" }), 3000);
}
