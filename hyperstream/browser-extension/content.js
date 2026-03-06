// HyperStream Video Overlay - Content Script
// Detects videos, shows download buttons + floating stream panel

(function () {
    'use strict';

    const HYPERSTREAM_API = 'http://localhost:14733';
    let overlayContainer = null;
    let activeOverlay = null;
    let streamPanel = null;
    let detectedStreams = [];

    // Video source patterns
    const videoPatterns = [
        /\.mp4(\?.*)?$/i,
        /\.webm(\?.*)?$/i,
        /\.mkv(\?.*)?$/i,
        /\.avi(\?.*)?$/i,
        /\.mov(\?.*)?$/i,
        /blob:/i,
        /googlevideo\.com/i,
        /videoplayback/i
    ];

    // Stream type icons (SVG)
    const STREAM_ICONS = {
        hls: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 7l-7 5 7 5V7z"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>',
        dash: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 7l-7 5 7 5V7z"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>',
        video: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5,3 19,12 5,21"/></svg>',
        audio: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>',
    };

    // ── Floating Stream Detection Panel ──────────────────────────────
    function createStreamPanel() {
        if (streamPanel) return;

        streamPanel = document.createElement('div');
        streamPanel.id = 'hyperstream-stream-panel';
        streamPanel.innerHTML = `
            <div class="hs-panel-header">
                <div class="hs-panel-title">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                        <polyline points="7,10 12,15 17,10"/>
                        <line x1="12" y1="15" x2="12" y2="3"/>
                    </svg>
                    <span>HyperStream</span>
                    <span class="hs-stream-count">0</span>
                </div>
                <button class="hs-panel-toggle" title="Toggle panel">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <polyline points="6,9 12,15 18,9"/>
                    </svg>
                </button>
            </div>
            <div class="hs-panel-body"></div>
        `;

        document.body.appendChild(streamPanel);

        // Toggle panel expand/collapse
        let expanded = false;
        const toggle = streamPanel.querySelector('.hs-panel-toggle');
        const body = streamPanel.querySelector('.hs-panel-body');
        const header = streamPanel.querySelector('.hs-panel-header');

        toggle.addEventListener('click', (e) => {
            e.stopPropagation();
            expanded = !expanded;
            body.style.display = expanded ? 'block' : 'none';
            toggle.style.transform = expanded ? 'rotate(180deg)' : '';
            streamPanel.classList.toggle('hs-expanded', expanded);
        });

        header.addEventListener('click', (e) => {
            if (e.target.closest('.hs-panel-toggle')) return;
            expanded = !expanded;
            body.style.display = expanded ? 'block' : 'none';
            toggle.style.transform = expanded ? 'rotate(180deg)' : '';
            streamPanel.classList.toggle('hs-expanded', expanded);
        });

        // Make panel draggable
        makeDraggable(streamPanel, header);
    }

    function makeDraggable(panel, handle) {
        let isDragging = false;
        let startX, startY, startRight, startBottom;

        handle.addEventListener('mousedown', (e) => {
            if (e.target.closest('button')) return;
            isDragging = true;
            startX = e.clientX;
            startY = e.clientY;
            const rect = panel.getBoundingClientRect();
            startRight = window.innerWidth - rect.right;
            startBottom = window.innerHeight - rect.bottom;
            e.preventDefault();
        });

        document.addEventListener('mousemove', (e) => {
            if (!isDragging) return;
            const dx = e.clientX - startX;
            const dy = e.clientY - startY;
            panel.style.right = Math.max(0, startRight - dx) + 'px';
            panel.style.bottom = Math.max(0, startBottom - dy) + 'px';
        });

        document.addEventListener('mouseup', () => {
            isDragging = false;
        });
    }

    function updateStreamPanel(streams) {
        detectedStreams = streams;
        if (streams.length === 0) {
            if (streamPanel) {
                streamPanel.style.display = 'none';
            }
            return;
        }

        createStreamPanel();
        streamPanel.style.display = 'block';

        const count = streamPanel.querySelector('.hs-stream-count');
        count.textContent = streams.length;

        const body = streamPanel.querySelector('.hs-panel-body');
        body.innerHTML = '';

        streams.forEach((stream, index) => {
            const item = document.createElement('div');
            item.className = 'hs-stream-item';

            const icon = STREAM_ICONS[stream.stream_type] || STREAM_ICONS.video;
            const label = getStreamLabel(stream);
            const badge = stream.stream_type.toUpperCase();
            const quality = stream.quality ? `<span class="hs-quality">${stream.quality}</span>` : '';
            const size = stream.size ? `<span class="hs-size">${formatSize(stream.size)}</span>` : '';

            item.innerHTML = `
                <div class="hs-stream-info">
                    <span class="hs-stream-icon">${icon}</span>
                    <span class="hs-stream-label" title="${escapeHtml(stream.url)}">${escapeHtml(label)}</span>
                    <span class="hs-badge hs-badge-${stream.stream_type}">${badge}</span>
                    ${quality}
                    ${size}
                </div>
                <button class="hs-download-btn" data-index="${index}" title="Download with HyperStream">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                        <polyline points="7,10 12,15 17,10"/>
                        <line x1="12" y1="15" x2="12" y2="3"/>
                    </svg>
                </button>
            `;

            const btn = item.querySelector('.hs-download-btn');
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                downloadStream(stream);
                btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2"><polyline points="20,6 9,17 4,12"/></svg>';
                setTimeout(() => {
                    btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7,10 12,15 17,10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>';
                }, 1500);
            });

            body.appendChild(item);
        });
    }

    function getStreamLabel(stream) {
        try {
            const url = new URL(stream.url);
            const filename = url.pathname.split('/').pop();
            if (filename && filename.length > 1 && filename.includes('.')) {
                return decodeURIComponent(filename).substring(0, 50);
            }
            return url.hostname + url.pathname.substring(0, 30);
        } catch {
            return stream.url.substring(0, 50);
        }
    }

    function formatSize(bytes) {
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
        if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
        return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
    }

    function escapeHtml(str) {
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    async function downloadStream(stream) {
        chrome.runtime.sendMessage({
            action: 'downloadStream',
            stream: stream,
        }, (response) => {
            if (response?.success) {
                showToast(`Downloading: ${getStreamLabel(stream)}`);
            } else {
                showToast('Failed to send to HyperStream');
            }
        });
    }

    // ── Listen for stream updates from background ────────────────────
    chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
        if (message.action === 'streams_updated') {
            updateStreamPanel(message.streams);
        }
    });

    // Request initial streams state on load
    chrome.runtime.sendMessage({ action: 'getStreams' }, (response) => {
        if (response?.streams?.length > 0) {
            updateStreamPanel(response.streams);
        }
    });

    // Create overlay container
    function createOverlayContainer() {
        if (overlayContainer) return;

        overlayContainer = document.createElement('div');
        overlayContainer.id = 'hyperstream-overlay-container';
        overlayContainer.style.cssText = `
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            pointer-events: none;
            z-index: 2147483647;
        `;
        document.body.appendChild(overlayContainer);
    }

    // Create download button overlay for a video
    function createVideoOverlay(video) {
        const overlay = document.createElement('div');
        overlay.className = 'hyperstream-video-overlay';

        const button = document.createElement('button');
        button.className = 'hyperstream-download-btn';
        button.innerHTML = `
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                <polyline points="7,10 12,15 17,10"/>
                <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            <span>Download with HyperStream</span>
        `;

        button.addEventListener('click', (e) => {
            e.preventDefault();
            e.stopPropagation();
            downloadVideo(video);
        });

        overlay.appendChild(button);
        return overlay;
    }

    // Position overlay on video
    function positionOverlay(overlay, video) {
        const rect = video.getBoundingClientRect();
        overlay.style.cssText = `
            position: fixed;
            top: ${rect.top + 10}px;
            right: ${window.innerWidth - rect.right + 10}px;
            pointer-events: auto;
            z-index: 2147483647;
            opacity: 0;
            transition: opacity 0.2s;
        `;
    }

    // Get video source URL
    function getVideoSource(video) {
        // Direct src
        if (video.src && !video.src.startsWith('blob:')) {
            return video.src;
        }

        // Source elements
        const sources = video.querySelectorAll('source');
        for (const source of sources) {
            if (source.src && !source.src.startsWith('blob:')) {
                return source.src;
            }
        }

        // Check currentSrc
        if (video.currentSrc && !video.currentSrc.startsWith('blob:')) {
            return video.currentSrc;
        }

        return null;
    }

    // Download video via HyperStream
    async function downloadVideo(video) {
        const url = getVideoSource(video);

        if (!url) {
            showToast('Could not detect video source. Try right-click → Copy video URL');
            return;
        }

        // Extract filename
        let filename = 'video.mp4';
        try {
            const urlObj = new URL(url);
            const pathParts = urlObj.pathname.split('/');
            const lastPart = pathParts[pathParts.length - 1];
            if (lastPart && lastPart.includes('.')) {
                filename = decodeURIComponent(lastPart.split('?')[0]);
            }
        } catch (e) {
            // ignore filename parsing errors
        }

        chrome.runtime.sendMessage({
            action: 'download',
            url: url,
            filename: filename,
        }, (response) => {
            if (response?.success) {
                showToast(`Downloading: ${filename}`);
            } else {
                showToast('HyperStream is not running or failed');
            }
        });
    }

    // Show toast notification
    function showToast(message) {
        const toast = document.createElement('div');
        toast.className = 'hyperstream-toast';
        toast.textContent = message;
        document.body.appendChild(toast);

        setTimeout(() => toast.classList.add('show'), 10);
        setTimeout(() => {
            toast.classList.remove('show');
            setTimeout(() => toast.remove(), 300);
        }, 3000);
    }

    // Handle video element
    function handleVideo(video) {
        if (video.dataset.hyperstreamHandled) return;
        video.dataset.hyperstreamHandled = 'true';

        createOverlayContainer();

        const overlay = createVideoOverlay(video);
        overlayContainer.appendChild(overlay);

        // Position on mouse enter
        video.addEventListener('mouseenter', () => {
            positionOverlay(overlay, video);
            activeOverlay = overlay;
            setTimeout(() => overlay.style.opacity = '1', 10);
        });

        video.addEventListener('mouseleave', (e) => {
            // Check if mouse moved to overlay
            const rect = overlay.getBoundingClientRect();
            if (e.clientX >= rect.left && e.clientX <= rect.right &&
                e.clientY >= rect.top && e.clientY <= rect.bottom) {
                return;
            }
            overlay.style.opacity = '0';
        });

        overlay.addEventListener('mouseleave', () => {
            overlay.style.opacity = '0';
        });

        // Update position on scroll/resize
        const updatePosition = () => {
            if (overlay.style.opacity === '1') {
                positionOverlay(overlay, video);
            }
        };

        window.addEventListener('scroll', updatePosition, { passive: true });
        window.addEventListener('resize', updatePosition, { passive: true });
    }

    // Scan for videos
    function scanForVideos() {
        const videos = document.querySelectorAll('video');
        videos.forEach(handleVideo);
    }

    // Use MutationObserver to detect new videos
    const observer = new MutationObserver((mutations) => {
        for (const mutation of mutations) {
            for (const node of mutation.addedNodes) {
                if (node.nodeType === Node.ELEMENT_NODE) {
                    if (node.tagName === 'VIDEO') {
                        handleVideo(node);
                    }
                    const videos = node.querySelectorAll?.('video');
                    if (videos) {
                        videos.forEach(handleVideo);
                    }
                }
            }
        }
    });

    // Initialize
    function init() {
        scanForVideos();
        observer.observe(document.body, {
            childList: true,
            subtree: true
        });
    }

    // Wait for DOM
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }

    // Re-scan periodically for dynamically loaded videos
    setInterval(scanForVideos, 2000);

})();
