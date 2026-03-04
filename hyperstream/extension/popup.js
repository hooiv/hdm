// HyperStream Popup Logic V2

const API_URL = "http://localhost:14733";

// State
let allItems = []; // { id, name, type, url, category }
let selectedIds = new Set();
let currentFilter = 'all';
let searchQuery = '';

document.addEventListener('DOMContentLoaded', async () => {
    checkHealth();
    setupEventListeners();
    scanCurrentTab();
});

function setupEventListeners() {
    // Search
    document.getElementById('search-input').addEventListener('input', (e) => {
        searchQuery = e.target.value.toLowerCase();
        renderList();
    });

    // Filters
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentFilter = btn.dataset.filter;
            renderList();
        });
    });

    // Buttons
    document.getElementById('rescan-btn').addEventListener('click', scanCurrentTab);
    document.getElementById('download-selected-btn').addEventListener('click', downloadSelected);

    // Settings
    const toggle = document.getElementById('intercept-toggle');
    chrome.storage.local.get(['intercept'], (result) => { toggle.checked = result.intercept || false; });
    toggle.addEventListener('change', (e) => { chrome.storage.local.set({ intercept: e.target.checked }); });
}

function processData(data) {
    allItems = [];
    selectedIds.clear();

    // Process Media
    if (data.media) {
        data.media.forEach((m, idx) => {
            allItems.push({
                id: `m_${idx}`,
                name: m.filename,
                type: m.type,
                url: m.url,
                category: 'media'
            });
        });
    }

    // Process Links
    if (data.links) {
        data.links.forEach((l, idx) => {
            const ext = l.filename.split('.').pop().toLowerCase();
            let cat = 'other';
            if (['mp4', 'mkv', 'webm', 'mp3', 'wav'].includes(ext)) cat = 'media';
            if (['pdf', 'doc', 'docx', 'xls'].includes(ext)) cat = 'doc';

            allItems.push({
                id: `l_${idx}`,
                name: l.filename,
                type: ext.toUpperCase(),
                url: l.url,
                category: cat
            });
        });
    }
}

function renderList() {
    const listEl = document.getElementById('results-list');
    const emptyState = document.getElementById('empty-state');
    listEl.innerHTML = '';

    const filtered = allItems.filter(item => {
        const matchesFilter = currentFilter === 'all' || item.category === currentFilter || (currentFilter === 'other' && !['media', 'doc'].includes(item.category));
        const matchesSearch = item.name.toLowerCase().includes(searchQuery);
        return matchesFilter && matchesSearch;
    });

    if (filtered.length === 0) {
        emptyState.style.display = 'block';
        emptyState.innerText = allItems.length === 0 ? "No items found." : "No matches.";
        listEl.style.display = 'none';
        updateActionButtons();
        return;
    }

    emptyState.style.display = 'none';
    listEl.style.display = 'flex';

    filtered.forEach(item => {
        const el = createItemElement(item);
        listEl.appendChild(el);
    });

    updateActionButtons();
}

function createItemElement(item) {
    const div = document.createElement('div');
    div.className = 'item';

    // Checkbox
    const cbWrapper = document.createElement('div');
    cbWrapper.className = 'checkbox-wrapper';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.className = 'checkbox';
    cb.checked = selectedIds.has(item.id);
    cb.onclick = (e) => {
        e.stopPropagation();
        toggleSelection(item.id);
    };
    cbWrapper.appendChild(cb);

    // Build DOM safely — no innerHTML with user-controlled data
    const infoDiv = document.createElement('div');
    infoDiv.className = 'item-info';

    const nameDiv = document.createElement('div');
    nameDiv.className = 'item-name';
    nameDiv.title = item.name;
    nameDiv.textContent = item.name;

    const typeDiv = document.createElement('div');
    typeDiv.className = 'item-type';
    typeDiv.textContent = item.type;

    infoDiv.appendChild(nameDiv);
    infoDiv.appendChild(typeDiv);

    const downloadBtn = document.createElement('button');
    downloadBtn.className = 'btn-download';
    downloadBtn.title = 'Download';
    downloadBtn.textContent = '\u2B07'; // ⬇

    div.appendChild(cbWrapper);
    div.appendChild(infoDiv);
    div.appendChild(downloadBtn);

    // Attach event listeners
    cb.onclick = (e) => {
        e.stopPropagation();
        toggleSelection(item.id, e.target.checked);
    };

    div.onclick = () => {
        const newChecked = !selectedIds.has(item.id);
        toggleSelection(item.id, newChecked);
        div.querySelector('.checkbox').checked = newChecked;
    };

    downloadBtn.onclick = (e) => {
        e.stopPropagation();
        triggerDownload(item.url, item.name);
    };

    return div;
}

function toggleSelection(id, isSelected) {
    if (isSelected) selectedIds.add(id);
    else selectedIds.delete(id);
    updateActionButtons();
}

function updateActionButtons() {
    const btn = document.getElementById('download-selected-btn');
    const count = selectedIds.size;
    btn.innerText = count > 0 ? `Download (${count})` : "Download Selected";
    btn.disabled = count === 0;
}

async function downloadSelected() {
    const btn = document.getElementById('download-selected-btn');
    btn.innerText = "Sending...";
    btn.disabled = true;

    let successCount = 0;

    const validItems = allItems.filter(i => selectedIds.has(i.id));
    for (const item of validItems) {
        const ok = await triggerDownload(item.url, item.name, true); // Silent mode
        if (ok) successCount++;
    }

    btn.innerText = `Sent ${successCount} files!`;
    setTimeout(() => {
        selectedIds.clear();
        renderList(); // Refreshes UI to clear checks
    }, 1500);
}

// ... existing helpers ...
async function checkHealth() {
    const statusEl = document.getElementById('status');
    const btn = document.getElementById('download-selected-btn');
    try {
        const res = await fetch(`${API_URL}/health`);
        if (res.ok) {
            statusEl.className = 'status-badge online';
            statusEl.innerText = 'CONNECTED';
        } else { throw new Error(); }
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
                document.getElementById('empty-state').innerText = "Refresh page to enable scanning.";
                return;
            }
            if (response) {
                processData(response);
                renderList();
            }
        });
    });
}

async function triggerDownload(url, filename, silent = false) {
    try {
        const response = await fetch(`${API_URL}/download`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ url, filename })
        });

        if (response.ok) {
            if (!silent) {
                // Visual feedback for single click
                chrome.action.setBadgeText({ text: "OK" });
                setTimeout(() => chrome.action.setBadgeText({ text: "" }), 1000);
            }
            return true;
        }
    } catch (e) {
        if (!silent) alert("Connection Failed");
    }
    return false;
}
