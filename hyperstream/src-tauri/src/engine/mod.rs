pub mod session;
pub mod intent;
pub mod swarm;
pub mod materializer;
pub mod hls;
pub mod dash;
pub mod multi_source;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadProtocol {
    Http,
    Hls,
    Dash,
}

pub(crate) fn detect_download_protocol(url: &str) -> DownloadProtocol {
    let normalized = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();

    if normalized.ends_with(".m3u8") {
        DownloadProtocol::Hls
    } else if normalized.ends_with(".mpd") {
        DownloadProtocol::Dash
    } else {
        DownloadProtocol::Http
    }
}

pub(crate) async fn start_download_routed(
    app: &tauri::AppHandle,
    state: &crate::core_state::AppState,
    id: String,
    url: String,
    path: String,
    custom_headers: Option<std::collections::HashMap<String, String>>,
    force: bool,
) -> Result<(), String> {
    let result = match detect_download_protocol(&url) {
        DownloadProtocol::Hls => {
            crate::engine::hls::start_hls_download_impl(app, state, id.clone(), url, path, force, custom_headers).await
        }
        DownloadProtocol::Dash => {
            crate::engine::dash::start_dash_download_impl(app, state, id.clone(), url, path, force, custom_headers).await
        }
        DownloadProtocol::Http => {
            crate::engine::session::start_download_impl(app, state, id.clone(), url, path, None, custom_headers, force, None).await
        }
    };

    result
}

#[cfg(test)]
mod tests {
    use super::{detect_download_protocol, DownloadProtocol};

    #[test]
    fn detects_manifest_protocols() {
        assert_eq!(detect_download_protocol("https://example.com/video.m3u8"), DownloadProtocol::Hls);
        assert_eq!(detect_download_protocol("https://example.com/video.mpd"), DownloadProtocol::Dash);
        assert_eq!(detect_download_protocol("https://example.com/file.bin"), DownloadProtocol::Http);
    }

    #[test]
    fn ignores_query_and_fragment_when_detecting_protocol() {
        assert_eq!(
            detect_download_protocol("https://example.com/stream.m3u8?token=abc#live"),
            DownloadProtocol::Hls,
        );
        assert_eq!(
            detect_download_protocol("https://example.com/manifest.mpd?quality=best"),
            DownloadProtocol::Dash,
        );
    }
}
