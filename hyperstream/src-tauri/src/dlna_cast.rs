use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize, Clone, Debug)]
pub struct DlnaDevice {
    pub name: String,
    pub location: String,
    pub device_type: String,
}

/// Discover DLNA/UPnP media renderers on the local network via SSDP.
/// Sends M-SEARCH multicast and parses LOCATION headers.
pub async fn discover_dlna() -> Result<Vec<DlnaDevice>, String> {
    let search_request = format!(
        "M-SEARCH * HTTP/1.1\r\n\
        HOST: 239.255.255.250:1900\r\n\
        MAN: \"ssdp:discover\"\r\n\
        ST: urn:schemas-upnp-org:service:AVTransport:1\r\n\
        MX: 3\r\n\r\n"
    );

    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("Socket bind error: {}", e))?;

    let multicast_addr: std::net::SocketAddr = "239.255.255.250:1900".parse().unwrap();
    socket.send_to(search_request.as_bytes(), multicast_addr)
        .await
        .map_err(|e| format!("Send error: {}", e))?;

    let mut devices = Vec::new();
    let mut buf = [0u8; 2048];

    loop {
        match tokio::time::timeout(Duration::from_secs(4), socket.recv_from(&mut buf)).await {
            Ok(Ok((size, _addr))) => {
                let response = String::from_utf8_lossy(&buf[..size]).to_string();
                
                // Extract LOCATION header
                if let Some(location) = extract_header(&response, "LOCATION") {
                    // Validate SSDP LOCATION is a local network URL before fetching
                    if validate_local_network_url(&location).is_err() {
                        eprintln!("[DLNA] Skipping non-local LOCATION: {}", location);
                        continue;
                    }
                    // Try to fetch device description XML
                    let name = fetch_device_name(&location).await.unwrap_or_else(|_| "Unknown Device".to_string());
                    
                    // Avoid duplicates
                    if !devices.iter().any(|d: &DlnaDevice| d.location == location) {
                        devices.push(DlnaDevice {
                            name,
                            location,
                            device_type: "MediaRenderer".to_string(),
                        });
                    }
                }
            }
            Ok(Err(_)) | Err(_) => break, // Socket error or timeout
        }
    }

    Ok(devices)
}

/// Cast a media file to a DLNA renderer by sending a SetAVTransportURI SOAP action.
/// Starts an ephemeral HTTP server to serve the file to the renderer.
pub async fn cast_to_dlna(file_path: String, device_location: String) -> Result<String, String> {
    // Validate device_location is a local network URL to prevent SSRF
    validate_local_network_url(&device_location)?;

    // Path validation: ensure file is within the download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon = dunce::canonicalize(&file_path)
        .map_err(|e| format!("Cannot resolve file path: {}", e))?;
    if !canon.starts_with(&download_dir) {
        return Err("File must be within the download directory".to_string());
    }

    // Start an ephemeral share so the DLNA renderer can fetch the file
    let share = crate::ephemeral_server::EPHEMERAL_MANAGER
        .start_share(file_path.clone(), 120) // 2-hour timeout for media playback
        .await?;

    let media_url = share.url.clone();

    let filename = std::path::Path::new(&file_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Get the AVTransport control URL from device description
    let control_url = get_av_transport_url(&device_location).await?;

    // XML-escape the media URL to prevent SOAP/XML injection
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;")
         .replace('>', "&gt;").replace('"', "&quot;")
         .replace('\'', "&apos;")
    }

    // Send SetAVTransportURI SOAP action
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <CurrentURI>{}</CurrentURI>
      <CurrentURIMetaData></CurrentURIMetaData>
    </u:SetAVTransportURI>
  </s:Body>
</s:Envelope>"#,
        xml_escape(&media_url)
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let res = client.post(&control_url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\"")
        .body(soap_body)
        .send()
        .await
        .map_err(|e| format!("SOAP request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("DLNA device returned: {}", res.status()));
    }

    // Now send Play command
    let play_body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>"#;

    let _play_res = client.post(&control_url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", "\"urn:schemas-upnp-org:service:AVTransport:1#Play\"")
        .body(play_body)
        .send()
        .await
        .map_err(|e| format!("Play command failed: {}", e))?;

    Ok(format!("Casting {} to DLNA device. Media URL: {}", filename, media_url))
}

fn extract_header(response: &str, header: &str) -> Option<String> {
    for line in response.lines() {
        let upper = line.to_uppercase();
        if upper.starts_with(&format!("{}:", header.to_uppercase())) {
            return Some(line[header.len() + 1..].trim().to_string());
        }
    }
    None
}

/// Validate that a URL points to a local/private network address (for DLNA SSRF prevention)
fn validate_local_network_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {},
        s => return Err(format!("Unsupported scheme for DLNA: {}", s)),
    }
    let host = parsed.host_str().ok_or("No host in DLNA URL")?;
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_private() || v4.is_link_local() || v4.is_loopback() {
                    return Ok(());
                }
            }
            std::net::IpAddr::V6(v6) => {
                // fe80::/10 link-local or ::1 loopback
                if v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    return Ok(());
                }
            }
        }
        return Err("DLNA device URL must be on the local network".to_string());
    }
    // Non-IP hostnames (e.g. mDNS .local names) — allow
    if host.ends_with(".local") || host == "localhost" {
        return Ok(());
    }
    Err("DLNA device URL must be a local network address".to_string())
}

async fn fetch_device_name(location: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(location).send().await.map_err(|e| e.to_string())?;
    let body = res.text().await.map_err(|e| e.to_string())?;

    // Extract <friendlyName> from XML
    if let Some(start) = body.find("<friendlyName>") {
        let after = &body[start + 14..];
        if let Some(end) = after.find("</friendlyName>") {
            return Ok(after[..end].to_string());
        }
    }

    Ok("Unknown Device".to_string())
}

async fn get_av_transport_url(device_location: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(device_location).send().await.map_err(|e| e.to_string())?;
    let body = res.text().await.map_err(|e| e.to_string())?;

    // Find AVTransport controlURL in the device description XML
    if let Some(pos) = body.find("AVTransport") {
        let after = &body[pos..];
        if let Some(ctrl_start) = after.find("<controlURL>") {
            let ctrl_after = &after[ctrl_start + 12..];
            if let Some(ctrl_end) = ctrl_after.find("</controlURL>") {
                let control_path = &ctrl_after[..ctrl_end];
                // Build absolute URL
                let base = reqwest::Url::parse(device_location).map_err(|e| e.to_string())?;
                let absolute = base.join(control_path).map_err(|e| e.to_string())?;
                return Ok(absolute.to_string());
            }
        }
    }

    Err("Could not find AVTransport control URL in device description".to_string())
}
