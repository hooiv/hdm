use std::io::{self};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

/// A wrapper around TcpStream that fragments the initial TLS ClientHello packet.
/// This is used to evade Deep Packet Inspection (DPI) that blocks sites based on SNI.
pub struct SniFragmentedStream {
    stream: TcpStream,
    fragmented: bool,
}

impl SniFragmentedStream {
    pub async fn connect(addr: impl tokio::net::ToSocketAddrs) -> io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?; // Essential for fragmentation to work immediately
        Ok(SniFragmentedStream { 
            stream,
            fragmented: false, 
        })
    }
}

impl AsyncRead for SniFragmentedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for SniFragmentedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Check if this looks like a TLS ClientHello
        // Byte 0: 0x16 (Handshake)
        // Byte 1-2: Version (0x0301 TLS 1.0 ... 0x0303 TLS 1.2/1.3 often compatible)
        // But for evasion, we just want to split the FIRST packet if it looks like a handshake
        // regardless of exact version, to break the SNI parser.
        
        if !self.fragmented && buf.len() > 5 && buf[0] == 0x16 {
            // Heuristic: Fragment after the 5-byte Record Header
            // The SNI extension usually comes later in the packet.
            // By sending [Header] ... [Rest], the DPI box might fail to reassemble 
            // the state or treat it as "Partial/Invalid" and pass it.
            
            // Note: Some advanced evasion techniques split the SNI host string itself: "you" + "tube.com".
            // Here we do a simple Header split (5 bytes).
            let fragment_len = 5;
            
            // Delegate to inner stream but only write 5 bytes
            match Pin::new(&mut self.stream).poll_write(cx, &buf[0..fragment_len]) {
                Poll::Ready(Ok(n)) => {
                    self.fragmented = true;
                    // We successfully wrote the fragment. 
                    // Return n (likely 5). 
                    // The caller (TLS library) sees partial write and will call us again with offset 5.
                    Poll::Ready(Ok(n))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Pin::new(&mut self.stream).poll_write(cx, buf)
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
