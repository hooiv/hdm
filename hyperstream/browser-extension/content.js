// HyperStream Video Overlay - Content Script
// Detects videos and shows download buttons

(function () {
    'use strict';

    const HYPERSTREAM_API = 'http://localhost:14733';
    let overlayContainer = null;
    let activeOverlay = null;

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
            console.log('Could not parse URL for filename');
        }

        try {
            const response = await fetch(`${HYPERSTREAM_API}/download`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ url, filename })
            });

            if (response.ok) {
                showToast(`Downloading: ${filename}`);
            } else {
                showToast('Failed to send to HyperStream');
            }
        } catch (error) {
            showToast('HyperStream is not running');
        }
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
