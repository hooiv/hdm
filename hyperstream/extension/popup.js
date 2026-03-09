// HyperStream Popup Logic V2

let API_URL = "http://localhost:14733";
let eventSource = null;

function buildAuthHeaders(token, includeJson = false) {
    const headers = {};
    if (includeJson) {
        headers['Content-Type'] = 'application/json';
    }
    if (token) {
        headers['X-HyperStream-Token'] = token;
    }
    return headers;
}

async function getAuthToken() {
    const { authToken } = await chrome.storage.local.get({ authToken: '' });
    return authToken.trim();
}

async function getAuthHeaders(includeJson = false) {
    return buildAuthHeaders(await getAuthToken(), includeJson);
}

function setTokenStatus(message, tone = 'neutral') {
    const statusEl = document.getElementById('tokenStatus');
    if (!statusEl) return;
    statusEl.textContent = message;
    statusEl.style.color = tone === 'error' ? '#f87171' : tone === 'success' ? '#22c55e' : '#94a3b8';
}

// Allow overriding the API URL via storage to support custom ports or remote hosts
async function initApiUrl() {
    const { apiUrl } = await chrome.storage.local.get({ apiUrl: API_URL });
    API_URL = apiUrl || API_URL;
    const input = document.getElementById('apiUrlInput');
    if (input) {
        input.value = API_URL;
        input.addEventListener('change', async () => {
            API_URL = input.value.trim() || API_URL;
            await chrome.storage.local.set({ apiUrl: API_URL });
            await checkHealth();
            await initEventSource();
            await refreshStatus();
        });
    }
}

async function initToken() {
    const tokenInput = document.getElementById('tokenInput');
    const saveBtn = document.getElementById('saveToken');
    if (!tokenInput || !saveBtn) return;

    const existingToken = await getAuthToken();
    if (existingToken) {
        tokenInput.value = existingToken;
        setTokenStatus('Token configured', 'success');
    } else {
        setTokenStatus('No token set — protected routes will be unavailable');
    }

    saveBtn.addEventListener('click', async () => {
        const token = tokenInput.value.trim();
        await chrome.storage.local.set({ authToken: token });
        setTokenStatus(token ? 'Token saved successfully' : 'Token cleared', token ? 'success' : 'neutral');
        saveBtn.textContent = 'Saved!';
        setTimeout(() => { saveBtn.textContent = 'Save Token'; }, 1500);
        await initEventSource();
        await refreshStatus();
    });
}

// State
let allItems = []; // { id, name, type, url, category }
let selectedIds = new Set();
let currentFilter = 'all';
let searchQuery = '';

document.addEventListener('DOMContentLoaded', async () => {
    await initApiUrl();
    await initToken();
    await checkHealth();
    setupEventListeners();
    scanCurrentTab();
    await initEventSource();
    await refreshStatus();
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
    const statusBtn = document.getElementById('refresh-status-btn');
    if (statusBtn) statusBtn.addEventListener('click', refreshStatus);

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

// Active download status helpers
async function initEventSource() {
    if (!window.EventSource) return;
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }

    const token = await getAuthToken();
    if (!token) return;

    try {
        eventSource = new EventSource(`${API_URL}/events?token=${encodeURIComponent(token)}`);
        eventSource.onmessage = (e) => {
            try {
                const msg = JSON.parse(e.data);
                if (msg.type === 'download_requested' || msg.type === 'extension_download') {
                    refreshStatus();
                    chrome.notifications.create({
                        type: 'basic',
                        iconUrl: 'icons/icon48.png',
                        title: 'HyperStream',
                        message: 'New download queued',
                    });
                } else if (msg.type === 'batch_links') {
                    chrome.notifications.create({
                        type: 'basic',
                        iconUrl: 'icons/icon48.png',
                        title: 'HyperStream',
                        message: `${msg.count || msg.links?.length || 0} links added to queue`,
                    });
                } else if (msg.type === 'download_progress') {
                    // update status badge and the list if visible
                    refreshStatus();
                }
            } catch {}
        };
        eventSource.onerror = (e) => {
            console.warn('EventSource connection error', e);
        };
    } catch (e) {
        console.warn('EventSource init failed', e);
    }
}

async function refreshStatus() {
    try {
        const headers = await getAuthHeaders();
        const res = await fetch(`${API_URL}/downloads`, { headers });
        if (!res.ok) {
            renderStatusError(res.status === 401 ? 'Auth token required' : 'Unable to fetch active downloads');
            return;
        }
        const list = await res.json();
        renderStatusList(list);
    } catch (e) {
        console.error('Status fetch failed', e);
        renderStatusError('Unable to fetch active downloads');
    }
}

function formatSpeed(bps) {
    if (!bps || bps === 0) return '';
    const units = ['B/s','KB/s','MB/s','GB/s'];
    let i = 0;
    let val = bps;
    while (val >= 1024 && i < units.length - 1) {
        val /= 1024;
        i++;
    }
    return `${val.toFixed(1)} ${units[i]}`;
}

function renderStatusList(list) {
    const container = document.getElementById('status-list');
    if (!container) return;
    container.innerHTML = '';
    if (!list || list.length === 0) {
        container.textContent = 'No active downloads';
        return;
    }
    let count = 0;
    list.forEach(item => {
        count++;
        const div = document.createElement('div');
        div.className = 'status-item';
        const percent = item.total > 0 ? ((item.downloaded / item.total) * 100).toFixed(1) : '0.0';
        let label = `${item.filename || '(unknown)'} ${percent}% ${item.status}`;
        if (item.speed_bps) label += ` ${formatSpeed(item.speed_bps)}`;
        div.textContent = label;
        if (item.can_pause !== false) {
            const pauseBtn = document.createElement('button');
            pauseBtn.textContent = '⏸';
            pauseBtn.title = 'Pause';
            pauseBtn.onclick = () => controlDownload(item.id, 'pause');
            div.appendChild(pauseBtn);
        }
        if (item.can_cancel !== false) {
            const cancelBtn = document.createElement('button');
            cancelBtn.textContent = '✖';
            cancelBtn.title = 'Cancel';
            cancelBtn.onclick = () => controlDownload(item.id, 'cancel');
            div.appendChild(cancelBtn);
        }
        container.appendChild(div);
    });
    chrome.action.setBadgeText({ text: count > 0 ? count.toString() : '' });
}

function renderStatusError(message) {
    const container = document.getElementById('status-list');
    if (!container) return;
    container.textContent = message;
}

// expose for unit tests
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { formatSpeed, buildAuthHeaders };
}

async function controlDownload(id, action) {
    try {
        const headers = await getAuthHeaders(true);
        await fetch(`${API_URL}/control`, {
            method: 'POST',
            headers,
            body: JSON.stringify({ id, action })
        });
        refreshStatus();
    } catch (e) {
        console.error('control call failed', e);
    }
}

// ... existing helpers ...
const EXT_VERSION = (typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.getManifest)
    ? chrome.runtime.getManifest().version
    : '0.0.0';

async function checkHealth() {
    const statusEl = document.getElementById('status');
    try {
        const res = await fetch(`${API_URL}/health`);
        if (res.ok) {
            // attempt to fetch version
            let verText = 'CONNECTED';
            try {
                const vres = await fetch(`${API_URL}/version`);
                if (vres.ok) {
                    const vdata = await vres.json();
                    verText = `v${vdata.version}`;
                    if (vdata.version !== EXT_VERSION) {
                        chrome.notifications.create({
                            type: 'basic',
                            iconUrl: 'icons/icon48.png',
                            title: 'Version mismatch',
                            message: `Server v${vdata.version} vs extension v${EXT_VERSION}`
                        });
                    }
                }
            } catch {}
            statusEl.className = 'status-badge online';
            statusEl.innerText = verText;
        } else { throw new Error(); }
    } catch (e) {
        statusEl.className = 'status-badge offline';
        statusEl.innerText = 'OFFLINE';
        if (chrome.runtime && chrome.runtime.sendNativeMessage) {
            chrome.runtime.sendNativeMessage('com.hyperstream', { action: 'launch' }, () => {});
        }
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
        const headers = await getAuthHeaders(true);
        const response = await fetch(`${API_URL}/download`, {
            method: "POST",
            headers,
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
        if (response.status === 401) {
            setTokenStatus('Saved token was rejected by HyperStream', 'error');
        }
    } catch (e) {
        if (!silent) alert("Connection Failed");
    }
    return false;
}
