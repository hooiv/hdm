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

    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Socket bind error: {}", e))?;
    
    socket.set_read_timeout(Some(std::time::Duration::from_secs(4)))
        .map_err(|e| format!("Timeout error: {}", e))?;

    let multicast_addr: std::net::SocketAddr = "239.255.255.250:1900".parse().unwrap();
    socket.send_to(search_request.as_bytes(), multicast_addr)
        .map_err(|e| format!("Send error: {}", e))?;

    let mut devices = Vec::new();
    let mut buf = [0u8; 2048];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((size, _addr)) => {
                let response = String::from_utf8_lossy(&buf[..size]).to_string();
                
                // Extract LOCATION header
                if let Some(location) = extract_header(&response, "LOCATION") {
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
            Err(_) => break, // Timeout
        }
    }

    Ok(devices)
}

/// Cast a media file to a DLNA renderer by sending a SetAVTransportURI SOAP action.
pub async fn cast_to_dlna(file_path: String, device_location: String) -> Result<String, String> {
    // Start a temporary HTTP server to serve the file
    let local_ip = local_ip_address::local_ip()
        .map_err(|e| format!("Cannot get local IP: {}", e))?;

    let filename = std::path::Path::new(&file_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp4")
        .to_lowercase();

    let _mime_type = match ext.as_str() {
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => "application/octet-stream",
    };

    // Use ephemeral server port
    let media_url = format!("http://{}:8765/cast/{}", local_ip, urlencoding::encode(&filename));

    // Get the AVTransport control URL from device description
    let control_url = get_av_transport_url(&device_location).await?;

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
        media_url
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
