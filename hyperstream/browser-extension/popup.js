const HYPERSTREAM_URL = 'http://localhost:14733';

async function checkConnection() {
    try {
        const response = await fetch(`${HYPERSTREAM_URL}/health`);
        const data = await response.json();
        return data.status === 'ok';
    } catch (e) {
        return false;
    }
}

async function updateStatus() {
    const statusDot = document.getElementById('statusDot');
    const statusText = document.getElementById('statusText');

    const connected = await checkConnection();

    if (connected) {
        statusDot.classList.add('connected');
        statusText.textContent = 'Connected to HyperStream';
    } else {
        statusDot.classList.remove('connected');
        statusText.textContent = 'HyperStream not running';
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

        // Brief visual feedback
        saveBtn.textContent = 'Saved!';
        setTimeout(() => { saveBtn.textContent = 'Save'; }, 1500);
    });
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
    updateStatus();
    initToggle();
    initToken();

    // Check connection periodically
    setInterval(updateStatus, 3000);
});
