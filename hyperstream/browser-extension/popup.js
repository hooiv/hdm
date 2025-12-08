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

// Initialize
document.addEventListener('DOMContentLoaded', () => {
    updateStatus();
    initToggle();

    // Check connection periodically
    setInterval(updateStatus, 3000);
});
