
// HyperStream Popup Logic

const API_URL = "http://localhost:14733";

document.addEventListener('DOMContentLoaded', async () => {
    checkHealth();
    scanCurrentTab();

    document.getElementById('scan-btn').addEventListener('click', scanCurrentTab);

    // Settings Logic
    const toggle = document.getElementById('intercept-toggle');

    // Load saved state
    chrome.storage.local.get(['intercept'], (result) => {
        toggle.checked = result.intercept || false;
    });

    // Save state on change
    toggle.addEventListener('change', (e) => {
        chrome.storage.local.set({ intercept: e.target.checked });
    });
});

async function checkHealth() {
    const statusEl = document.getElementById('status');
    try {
        const res = await fetch(`${API_URL}/health`);
        if (res.ok) {
            statusEl.className = 'status-badge online';
            statusEl.innerText = 'CONNECTED';
        } else {
            throw new Error('Not OK');
        }
    } catch (e) {
        statusEl.className = 'status-badge offline';
        statusEl.innerText = 'OFFLINE';
    }
}

function scanCurrentTab() {
    chrome.tabs.query({ active: true, currentWindow: true }, function (tabs) {
        if (!tabs[0]) return;

        chrome.tabs.sendMessage(tabs[0].id, { action: "scan" }, function (response) {
            if (chrome.runtime.lastError) {
                // Content script might not be injected yet or restricted page
                console.log("Scan error:", chrome.runtime.lastError.message);
                // Inject if needed? 
                // Since we added content_scripts in manifest, it should be there for new pages.
                // For existing pages, we might need reload.
                document.getElementById('empty-state').innerText = "Please reload the page.";
                return;
            }

            if (response) {
                renderResults(response);
            }
        });
    });
}

function renderResults(data) {
    const mediaList = document.getElementById('media-list');
    const linksList = document.getElementById('links-list');
    const mediaSec = document.getElementById('media-section');
    const linksSec = document.getElementById('links-section');
    const emptyState = document.getElementById('empty-state');

    mediaList.innerHTML = '';
    linksList.innerHTML = '';
    let hasItems = false;

    if (data.media && data.media.length > 0) {
        hasItems = true;
        mediaSec.style.display = 'block';
        data.media.forEach(m => {
            const el = createItem(m.filename, m.type, m.url);
            mediaList.appendChild(el);
        });
    } else {
        mediaSec.style.display = 'none';
    }

    if (data.links && data.links.length > 0) {
        hasItems = true;
        linksSec.style.display = 'block';
        data.links.forEach(l => {
            const el = createItem(l.filename, 'File', l.url);
            linksList.appendChild(el);
        });
    } else {
        linksSec.style.display = 'none';
    }

    emptyState.style.display = hasItems ? 'none' : 'block';
    if (!hasItems) emptyState.innerText = "No media or downloads found.";
}

function createItem(name, type, url) {
    const div = document.createElement('div');
    div.className = 'item';
    div.innerHTML = `
        <div class="item-info">
            <div class="item-name" title="${name}">${name}</div>
            <div class="item-type">${type}</div>
        </div>
        <button class="btn-download" title="Download">⬇</button>
    `;
    div.querySelector('.btn-download').onclick = (e) => {
        e.stopPropagation();
        triggerDownload(url, name);
    };
    div.onclick = () => triggerDownload(url, name);
    return div;
}

async function triggerDownload(url, filename) {
    try {
        const response = await fetch(`${API_URL}/download`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ url, filename })
        });

        if (response.ok) {
            // Flash success
            const btn = document.getElementById('scan-btn');
            const original = btn.innerText;
            btn.innerText = "Download Started!";
            btn.style.background = "#22c55e";
            setTimeout(() => {
                btn.innerText = original;
                btn.style.background = ""; // Reset to CSS default
            }, 2000);
        }
    } catch (e) {
        alert("Failed to send to HyperStream. Is it running?");
    }
}
