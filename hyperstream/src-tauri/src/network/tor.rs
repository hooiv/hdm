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
    if TOR_BOOTSTRAPPED.load(Ordering::Relaxed) {
        return Ok(SOCKS_PORT.load(Ordering::Relaxed));
    }

    println!("Initializing Tor...");

    let config = TorClientConfig::default();
    let client = TorClient::create_bootstrapped(config).await
        .map_err(|e| format!("Failed to bootstrap Tor: {}", e))?;

    println!("Tor Bootstrapped Successfully!");
    
    // Store Client
    {
        let mut guard = TOR_CLIENT.lock().await;
        *guard = Some(client.clone());
    }
    TOR_BOOTSTRAPPED.store(true, Ordering::Relaxed);

    // Start SOCKS5 Proxy
    let port = start_socks_proxy(client).await?;
    SOCKS_PORT.store(port, Ordering::Relaxed);
    
    Ok(port)
}

pub fn get_socks_port() -> Option<u16> {
    let port = SOCKS_PORT.load(Ordering::Relaxed);
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
        _ => return Err("Unsupported address type".into()),
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);

    println!("Tor Proxy: Connecting to {}:{}", host, port);

    // 3. Connect via Tor
    let target_stream = client.connect((host.as_str(), port)).await?;

    // 4. Reply Success
    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0,0,0,0, 0,0]).await?;

    // 5. Pipe
    let (mut ri, mut wi) = stream.into_split();
    let (mut ro, mut wo) = target_stream.split();

    let client_to_server = tokio::io::copy(&mut ri, &mut wo);
    let server_to_client = tokio::io::copy(&mut ro, &mut wi);

    tokio::select! {
        _ = client_to_server => {},
        _ = server_to_client => {},
    }

    Ok(())
}
