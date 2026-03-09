let HYPERSTREAM_URL = 'http://localhost:14733';
let eventSource = null;

async function initApiUrl() {
    const { apiUrl } = await chrome.storage.local.get({ apiUrl: HYPERSTREAM_URL });
    HYPERSTREAM_URL = apiUrl || HYPERSTREAM_URL;
    const input = document.getElementById('apiUrlInput');
    if (input) {
        input.value = HYPERSTREAM_URL;
        input.addEventListener('change', async () => {
            HYPERSTREAM_URL = input.value.trim() || HYPERSTREAM_URL;
            await chrome.storage.local.set({ apiUrl: HYPERSTREAM_URL });
            await updateStatus();
            await initEventSource();
            await refreshStatus();
            await refreshFeeds();
        });
    }
}

async function getAuthToken() {
    const { authToken } = await chrome.storage.local.get({ authToken: '' });
    return authToken.trim();
}

async function initEventSource() {
    if (!window.EventSource) return;
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }

    const token = await getAuthToken();
    if (!token) return;

    try {
        eventSource = new EventSource(`${HYPERSTREAM_URL}/events?token=${encodeURIComponent(token)}`);
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
                        message: `${msg.count || (msg.links && msg.links.length) || 0} links queued`,
                    });
                } else if (msg.type === 'download_progress') {
                    refreshStatus();
                } else if (msg.type === 'feed_update') {
                    refreshFeeds();
                    chrome.notifications.create({
                        type: 'basic',
                        iconUrl: 'icons/icon48.png',
                        title: 'RSS Feed',
                        message: 'New feed items arrived',
                    });
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

async function checkConnection() {
    try {
        const response = await fetch(`${HYPERSTREAM_URL}/health`);
        const data = await response.json();
        return data.status === 'ok';
    } catch (e) {
        return false;
    }
}

// helpers for feeds API
async function getAuthHeaders(includeJson = false) {
    const token = await getAuthToken();
    const headers = {};
    if (includeJson) {
        headers['Content-Type'] = 'application/json';
    }
    if (token) {
        headers['X-HyperStream-Token'] = token;
    }
    return headers;
}

async function refreshFeeds() {
    try {
        const headers = await getAuthHeaders();
        const res = await fetch(`${HYPERSTREAM_URL}/feeds`, { headers });
        const container = document.getElementById('feeds-list');
        if (!container) return;
        if (!res.ok) {
            container.textContent = res.status === 401 ? 'Auth token required' : 'Error fetching feeds';
            return;
        }
        const list = await res.json();
        if (list.length === 0) {
            container.textContent = 'No feeds';
            return;
        }
        container.innerHTML = '';
        list.forEach(f => {
            const div = document.createElement('div');
            div.textContent = `${f.name} ${f.unread_count||0}`;
            const refBtn = document.createElement('button');
            refBtn.textContent = '↻';
            refBtn.style.marginLeft = '4px';
            refBtn.onclick = () => manualRefresh(f.id);
            div.appendChild(refBtn);
            container.appendChild(div);
        });
    } catch (e) {
        console.error('refreshFeeds error', e);
    }
}

async function manualRefresh(id) {
    try {
        const headers = await getAuthHeaders();
        const res = await fetch(`${HYPERSTREAM_URL}/feeds/${id}/refresh`, { method: 'POST', headers });
        if (!res.ok) throw 'refresh failed';
        refreshFeeds();
    } catch (e) {
        console.error('manual refresh error', e);
    }
}

const EXT_VERSION = chrome.runtime.getManifest().version;

async function updateStatus() {
    const statusDot = document.getElementById('statusDot');
    const statusText = document.getElementById('statusText');

    const connected = await checkConnection();

    if (connected) {
        statusDot.classList.add('connected');
        // fetch version for additional context
        try {
            const verRes = await fetch(`${HYPERSTREAM_URL}/version`);
            if (verRes.ok) {
                const data = await verRes.json();
                statusText.textContent = `Connected (v${data.version})`;
                if (data.version !== EXT_VERSION) {
                    chrome.notifications.create({
                        type: 'basic',
                        iconUrl: 'icons/icon48.png',
                        title: 'Version mismatch',
                        message: `Server v${data.version} vs extension v${EXT_VERSION}`
                    });
                }
            } else {
                statusText.textContent = 'Connected to HyperStream';
            }
        } catch {
            statusText.textContent = 'Connected to HyperStream';
        }
    } else {
        statusDot.classList.remove('connected');
        statusText.textContent = 'HyperStream not running';
        // try to wake desktop app via native messaging if available
        if (chrome.runtime && chrome.runtime.sendNativeMessage) {
            chrome.runtime.sendNativeMessage('com.hyperstream', { action: 'launch' }, () => {
                /* no-op */
            });
        }
    }
}

async function initToggle() {
    const toggle = document.getElementById('enableToggle');
    const { enabled } = await chrome.storage.local.get({ enabled: true });
    toggle.checked = enabled;

    toggle.addEventListener('change', async () => {
        await chrome.storage.local.set({ enabled: toggle.checked });
    });
}

async function initToken() {
    const tokenInput = document.getElementById('tokenInput');
    const saveBtn = document.getElementById('saveToken');
    const tokenStatus = document.getElementById('tokenStatus');

    // Load existing token
    const { authToken } = await chrome.storage.local.get({ authToken: '' });
    if (authToken) {
        // Show masked token
        tokenInput.value = authToken;
        tokenStatus.className = 'token-status valid';
        tokenStatus.textContent = 'Token configured';
    } else {
        tokenStatus.className = 'token-status missing';
        tokenStatus.textContent = 'No token set - downloads will use browser default';
    }

    saveBtn.addEventListener('click', async () => {
        const token = tokenInput.value.trim();
        if (!token) {
            tokenStatus.className = 'token-status missing';
            tokenStatus.textContent = 'Please enter a token';
            return;
        }

        await chrome.storage.local.set({ authToken: token });
        tokenStatus.className = 'token-status valid';
        tokenStatus.textContent = 'Token saved successfully';
        await initEventSource();
        await refreshStatus();
        await refreshFeeds();

        // Brief visual feedback
        saveBtn.textContent = 'Saved!';
        setTimeout(() => { saveBtn.textContent = 'Save'; }, 1500);
    });
}

async function refreshStatus() {
    try {
        const headers = await getAuthHeaders();
        const res = await fetch(`${HYPERSTREAM_URL}/downloads`, { headers });
        const container = document.getElementById('status-list');
        if (!container) return;
        if (!res.ok) {
            container.textContent = res.status === 401 ? 'Auth token required' : 'Error fetching status';
            return;
        }
        const list = await res.json();
        if (list.length === 0) {
            container.textContent = 'No active downloads';
            return;
        }
        container.innerHTML = '';
        let count = 0;
        list.forEach(item => {
            count++;
            const div = document.createElement('div');
            const percent = item.total > 0 ? ((item.downloaded / item.total) * 100).toFixed(1) : '0.0';
            div.textContent = `${item.filename||'(unknown)'} ${percent}% ${item.status}`;
            if (item.can_pause !== false) {
                const pause = document.createElement('button');
                pause.textContent = '⏸';
                pause.style.marginLeft='4px';
                pause.onclick = () => control(item.id,'pause');
                div.appendChild(pause);
            }
            if (item.can_cancel !== false) {
                const cancel = document.createElement('button');
                cancel.textContent = '✖';
                cancel.style.marginLeft='2px';
                cancel.onclick = () => control(item.id,'cancel');
                div.appendChild(cancel);
            }
            container.appendChild(div);
        });
        chrome.action.setBadgeText({ text: count > 0 ? count.toString() : '' });
    } catch (e) {
        console.error('refreshStatus error', e);
    }
}

async function control(id, action) {
    try {
        const headers = await getAuthHeaders(true);
        await fetch(`${HYPERSTREAM_URL}/control`, {
            method: 'POST',
            headers,
            body: JSON.stringify({ id, action })
        });
        refreshStatus();
    } catch (e) {
        console.error('control error', e);
    }
}

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    await initApiUrl();
    await updateStatus();
    await initToggle();
    await initToken();

    const statusBtn = document.getElementById('refresh-status-btn');
    if (statusBtn) statusBtn.addEventListener('click', refreshStatus);
    const feedsBtn = document.getElementById('refresh-feeds-btn');
    if (feedsBtn) feedsBtn.addEventListener('click', refreshFeeds);

    await initEventSource();
    await refreshStatus();

    // initial feed load
    await refreshFeeds();

    // Check connection periodically
    setInterval(updateStatus, 3000);
});
