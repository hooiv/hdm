use arti_client::{TorClient, TorClientConfig};
use tor_rtcompat::PreferredRuntime;
use std::sync::Arc;
use tokio::sync::Mutex;
use lazy_static::lazy_static;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

lazy_static! {
    static ref TOR_CLIENT: Arc<Mutex<Option<TorClient<PreferredRuntime>>>> = Arc::new(Mutex::new(None));
    static ref TOR_BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);
    static ref SOCKS_PORT: AtomicU16 = AtomicU16::new(0);
}

pub async fn init_tor() -> Result<u16, String> {
    // Use the TOR_CLIENT mutex as the single synchronization point to prevent double-bootstrap
    let mut guard = TOR_CLIENT.lock().await;
    
    if guard.is_some() {
        let port = SOCKS_PORT.load(Ordering::Acquire);
        return Ok(port);
    }

    println!("Initializing Tor...");

    let config = TorClientConfig::default();
    let client = TorClient::create_bootstrapped(config).await
        .map_err(|e| format!("Failed to bootstrap Tor: {}", e))?;

    println!("Tor Bootstrapped Successfully!");

    // Start SOCKS5 Proxy BEFORE signaling readiness
    let port = start_socks_proxy(client.clone()).await?;
    SOCKS_PORT.store(port, Ordering::Release);
    
    // Store Client — must be last so get_socks_port sees valid port
    *guard = Some(client);
    TOR_BOOTSTRAPPED.store(true, Ordering::Release);
    
    Ok(port)
}

pub fn get_socks_port() -> Option<u16> {
    if !TOR_BOOTSTRAPPED.load(Ordering::Acquire) {
        return None;
    }
    let port = SOCKS_PORT.load(Ordering::Acquire);
    if port > 0 { Some(port) } else { None }
}

async fn start_socks_proxy(client: TorClient<PreferredRuntime>) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").await
        .map_err(|e| format!("Failed to bind SOCKS listener: {}", e))?;
    
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    println!("Tor SOCKS5 Proxy listening on 127.0.0.1:{}", port);

    let client_clone = client.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let c = client_clone.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_socks_connection(stream, c).await {
                            eprintln!("SOCKS error: {}", e);
                        }
                    });
                }
                Err(e) => eprintln!("SOCKS accept error: {}", e),
            }
        }
    });

    Ok(port)
}

async fn handle_socks_connection(mut stream: TcpStream, client: TorClient<PreferredRuntime>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Handshake
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;
    if buf[0] != 0x05 { return Err("Not SOCKS5".into()); }
    
    let n_methods = buf[1] as usize;
    let mut methods = vec![0u8; n_methods];
    stream.read_exact(&mut methods).await?;

    // Respond: Initial handshake (0x00 = No Auth)
    stream.write_all(&[0x05, 0x00]).await?;

    // 2. Request
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    // header[1] == 0x01 (Connect)
    
    let addr_type = header[3];
    let host = match addr_type {
        0x01 => { // IPv4
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        },
        0x03 => { // Domain
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;
            String::from_utf8_lossy(&domain).to_string()
        },
        0x04 => { // IPv6
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await?;
            let addr = std::net::Ipv6Addr::from(ip);
            addr.to_string()
        },
        _ => {
            // SOCKS5 error reply: address type not supported (0x08)
            let _ = stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0,0,0,0, 0,0]).await;
            return Err("Unsupported address type".into());
        },
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);

    println!("Tor Proxy: Connecting to {}:{}", host, port);

    // 3. Connect via Tor
    let target_stream = match client.connect((host.as_str(), port)).await {
        Ok(s) => s,
        Err(e) => {
            // SOCKS5 error reply: connection refused (0x05) or general failure (0x01)
            let _ = stream.write_all(&[0x05, 0x05, 0x00, 0x01, 0,0,0,0, 0,0]).await;
            return Err(e.into());
        }
    };

    // 4. Reply Success
    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0,0,0,0, 0,0]).await?;

    // 5. Pipe data bidirectionally
    let (mut ri, mut wi) = stream.into_split();
    let (mut ro, mut wo) = target_stream.split();

    let c2s = tokio::io::copy(&mut ri, &mut wo);
    let s2c = tokio::io::copy(&mut ro, &mut wi);
    tokio::pin!(c2s, s2c);

    // When one direction finishes, drain the other to avoid data loss.
    // select! alone would drop the losing branch mid-transfer.
    tokio::select! {
        _ = &mut c2s => {
            // Client finished sending (EOF). Drain remaining server→client data.
            let _ = s2c.await;
        },
        _ = &mut s2c => {
            // Server finished sending. Client should stop shortly.
            let _ = c2s.await;
        },
    }

    Ok(())
}
