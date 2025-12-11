
// HyperStream Content Script
// Scans for media and downloadable links

console.log("HyperStream Content Script Loaded");

function scanPage() {
    const media = [];
    const links = [];

    // 1. Scan Video/Audio Elements
    document.querySelectorAll('video, audio').forEach((el, index) => {
        if (el.src) {
            media.push({
                type: el.tagName.toLowerCase(),
                url: el.src,
                filename: `media_${index}_${Date.now()}`
            });
        }
        // Check sources inside
        el.querySelectorAll('source').forEach(source => {
            if (source.src) {
                media.push({
                    type: el.tagName.toLowerCase(),
                    url: source.src,
                    filename: `media_src_${index}`
                });
            }
        });
    });

    // 2. Scan Links for common extensions
    const extensions = ['.exe', '.msi', '.zip', '.rar', '.7z', '.iso', '.mp4', '.mkv', '.mp3', '.pdf', '.dmg', '.pkg'];
    document.querySelectorAll('a').forEach(a => {
        if (a.href) {
            const lower = a.href.toLowerCase();
            const hasExt = extensions.some(ext => lower.includes(ext));
            if (hasExt) {
                links.push({
                    url: a.href,
                    filename: a.innerText.trim() || a.href.split('/').pop().split('?')[0] || 'download'
                });
            }
        }
    });

    return { media, links, title: document.title };
}

// Listen for messages from Popup
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
    if (request.action === "scan") {
        const data = scanPage();
        sendResponse(data);
    }
});
