//! A SOCKS listener on the loopback interface that forwards to Tor.
//!
//! The handshake is parsed by `tor-socksproto`, the same crate Arti's own
//! proxy uses, so both SOCKS5 and SOCKS4a clients are understood. The Monero
//! wallet library speaks SOCKS4a by default.
//!
//! Only `CONNECT` to a `.onion` hostname is served. Everything else is
//! answered with a SOCKS error and closed.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tor_socksproto::{
    Buffer, Handshake as _, NextStep, SocksCmd, SocksProxyHandshake, SocksRequest, SocksStatus,
};

use crate::onion::onion_hostname;

/// How long a client may take to finish the SOCKS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Opens streams to onion services. Implemented by the Tor client, and by a
/// stand-in in tests so the listener can be tested without a network.
pub(crate) trait Connector: Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    fn connect(
        &self,
        onion_host: &str,
        port: u16,
    ) -> impl Future<Output = Result<Self::Stream, ConnectFailure>> + Send;
}

/// Why a stream could not be opened, reduced to what a SOCKS reply can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectFailure {
    /// Tor is not ready, or the failure has no more specific description.
    General,
    /// Tor could not get to its own network.
    NetworkUnreachable,
    /// The onion service could not be reached.
    HostUnreachable,
    /// The attempt timed out.
    TimedOut,
    /// The target was rejected as not allowed.
    NotAllowed,
}

impl ConnectFailure {
    fn socks_status(self) -> SocksStatus {
        match self {
            ConnectFailure::General => SocksStatus::GENERAL_FAILURE,
            ConnectFailure::NetworkUnreachable => SocksStatus::NETWORK_UNREACHABLE,
            ConnectFailure::HostUnreachable => SocksStatus::HOST_UNREACHABLE,
            ConnectFailure::TimedOut => SocksStatus::TTL_EXPIRED,
            ConnectFailure::NotAllowed => SocksStatus::NOT_ALLOWED,
        }
    }
}

/// Binds a listener to an OS-chosen port on `127.0.0.1`.
pub(crate) async fn bind_loopback() -> io::Result<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    Ok((listener, addr))
}

/// Accepts connections until `shutdown` changes or its sender is dropped.
pub(crate) async fn serve<C: Connector>(
    listener: TcpListener,
    connector: Arc<C>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = shutdown.changed() => return,
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else {
                    // Transient accept errors (for example, out of file
                    // descriptors) must not kill the listener.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let connector = Arc::clone(&connector);
                let mut shutdown = shutdown.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = shutdown.changed() => {}
                        _ = handle_client(stream, connector) => {}
                    }
                });
            }
        }
    }
}

/// Serves one client. Errors only close that client's connection.
async fn handle_client<C: Connector>(mut client: TcpStream, connector: Arc<C>) {
    let _ = client.set_nodelay(true);

    let handshake = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_request(&mut client)).await;
    let Ok(Ok((request, read_ahead))) = handshake else {
        return;
    };

    if request.command() != SocksCmd::CONNECT {
        refuse(&mut client, &request, SocksStatus::COMMAND_NOT_SUPPORTED).await;
        return;
    }
    let Some(host) = onion_hostname(request.addr()) else {
        refuse(&mut client, &request, SocksStatus::NOT_ALLOWED).await;
        return;
    };

    let mut upstream = match connector.connect(&host, request.port()).await {
        Ok(stream) => stream,
        Err(failure) => {
            refuse(&mut client, &request, failure.socks_status()).await;
            return;
        }
    };

    let Ok(reply) = request.reply(SocksStatus::SUCCEEDED, None) else {
        return;
    };
    if client.write_all(&reply).await.is_err() {
        return;
    }
    if !read_ahead.is_empty() && upstream.write_all(&read_ahead).await.is_err() {
        return;
    }

    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    let _ = upstream.shutdown().await;
}

/// Runs the SOCKS handshake and returns the request plus any bytes the
/// client sent after it.
async fn read_request(client: &mut TcpStream) -> io::Result<(SocksRequest, Vec<u8>)> {
    let mut handshake = SocksProxyHandshake::new();
    let mut buffer = Buffer::new();
    loop {
        match handshake.step(&mut buffer).map_err(invalid)? {
            NextStep::Send(data) => client.write_all(&data).await?,
            NextStep::Recv(mut recv) => {
                let received = client.read(recv.buf()).await?;
                recv.note_received(received).map_err(invalid)?;
            }
            NextStep::Finished(finished) => return Ok(finished.into_output_and_vec()),
        }
    }
}

async fn refuse(client: &mut TcpStream, request: &SocksRequest, status: SocksStatus) {
    if let Ok(reply) = request.reply(status, None) {
        let _ = client.write_all(&reply).await;
    }
    let _ = client.shutdown().await;
}

fn invalid(error: tor_socksproto::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const ONION: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";

    /// Stands in for Tor: "connects" every onion target to a local echo
    /// server and records what it was asked for.
    struct EchoConnector {
        echo: SocketAddr,
        calls: AtomicUsize,
        last_target: Mutex<Option<(String, u16)>>,
        fail_with: Option<ConnectFailure>,
    }

    impl Connector for EchoConnector {
        type Stream = TcpStream;

        async fn connect(&self, onion_host: &str, port: u16) -> Result<TcpStream, ConnectFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_target.lock().unwrap() = Some((onion_host.to_owned(), port));
            if let Some(failure) = self.fail_with {
                return Err(failure);
            }
            TcpStream::connect(self.echo)
                .await
                .map_err(|_| ConnectFailure::General)
        }
    }

    struct Harness {
        socks: SocketAddr,
        connector: Arc<EchoConnector>,
        _shutdown: watch::Sender<bool>,
    }

    async fn start(fail_with: Option<ConnectFailure>) -> Harness {
        let echo_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let echo = echo_listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = echo_listener.accept().await {
                tokio::spawn(async move {
                    let (mut reader, mut writer) = stream.split();
                    let _ = tokio::io::copy(&mut reader, &mut writer).await;
                });
            }
        });

        let connector = Arc::new(EchoConnector {
            echo,
            calls: AtomicUsize::new(0),
            last_target: Mutex::new(None),
            fail_with,
        });
        let (listener, socks) = bind_loopback().await.unwrap();
        let (shutdown, shutdown_rx) = watch::channel(false);
        tokio::spawn(serve(listener, Arc::clone(&connector), shutdown_rx));
        Harness {
            socks,
            connector,
            _shutdown: shutdown,
        }
    }

    /// SOCKS5, no authentication, CONNECT to a hostname.
    async fn socks5_connect(socks: SocketAddr, host: &str, port: u16) -> (TcpStream, u8) {
        let mut stream = TcpStream::connect(socks).await.unwrap();
        stream.write_all(&[5, 1, 0]).await.unwrap();
        let mut method = [0u8; 2];
        stream.read_exact(&mut method).await.unwrap();
        assert_eq!(method, [5, 0]);

        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        stream.write_all(&request).await.unwrap();

        let mut head = [0u8; 4];
        stream.read_exact(&mut head).await.unwrap();
        let status = head[1];
        // Drain the bound address so the stream is positioned at payload.
        let rest = match head[3] {
            1 => 4 + 2,
            4 => 16 + 2,
            other => panic!("unexpected address type {other}"),
        };
        let mut skip = vec![0u8; rest];
        stream.read_exact(&mut skip).await.unwrap();
        (stream, status)
    }

    /// SOCKS5 CONNECT to an IPv4 literal; returns the reply status.
    async fn socks5_connect_ip(socks: SocketAddr, ip: [u8; 4], port: u16) -> u8 {
        let mut stream = TcpStream::connect(socks).await.unwrap();
        stream.write_all(&[5, 1, 0]).await.unwrap();
        let mut method = [0u8; 2];
        stream.read_exact(&mut method).await.unwrap();
        let mut request = vec![5, 1, 0, 1];
        request.extend_from_slice(&ip);
        request.extend_from_slice(&port.to_be_bytes());
        stream.write_all(&request).await.unwrap();
        let mut head = [0u8; 4];
        stream.read_exact(&mut head).await.unwrap();
        head[1]
    }

    /// SOCKS4a CONNECT to a hostname; returns the stream and the reply code.
    async fn socks4a_connect(socks: SocketAddr, host: &str, port: u16) -> (TcpStream, u8) {
        let mut stream = TcpStream::connect(socks).await.unwrap();
        let mut request = vec![4, 1];
        request.extend_from_slice(&port.to_be_bytes());
        request.extend_from_slice(&[0, 0, 0, 1]); // 0.0.0.x marks SOCKS4a
        request.push(0); // empty user id
        request.extend_from_slice(host.as_bytes());
        request.push(0);
        stream.write_all(&request).await.unwrap();
        let mut reply = [0u8; 8];
        stream.read_exact(&mut reply).await.unwrap();
        (stream, reply[1])
    }

    async fn assert_echo(stream: &mut TcpStream) {
        stream.write_all(b"veil").await.unwrap();
        let mut echoed = [0u8; 4];
        stream.read_exact(&mut echoed).await.unwrap();
        assert_eq!(&echoed, b"veil");
    }

    #[tokio::test]
    async fn listener_is_on_loopback() {
        let harness = start(None).await;
        assert!(harness.socks.ip().is_loopback());
        assert_ne!(harness.socks.port(), 0);
    }

    #[tokio::test]
    async fn socks5_onion_connect_is_relayed() {
        let harness = start(None).await;
        let (mut stream, status) = socks5_connect(harness.socks, ONION, 18081).await;
        assert_eq!(status, 0x00);
        assert_echo(&mut stream).await;
        assert_eq!(
            *harness.connector.last_target.lock().unwrap(),
            Some((ONION.to_owned(), 18081))
        );
    }

    #[tokio::test]
    async fn socks4a_onion_connect_is_relayed() {
        let harness = start(None).await;
        let (mut stream, code) = socks4a_connect(harness.socks, ONION, 18081).await;
        assert_eq!(code, 90, "SOCKS4 request granted");
        assert_echo(&mut stream).await;
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn clearnet_hostname_is_refused_without_connecting() {
        let harness = start(None).await;
        let (_stream, status) = socks5_connect(harness.socks, "example.com", 443).await;
        assert_eq!(status, 0x02, "connection not allowed by ruleset");
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn socks4a_clearnet_hostname_is_refused() {
        let harness = start(None).await;
        let (_stream, code) = socks4a_connect(harness.socks, "example.com", 443).await;
        assert_eq!(code, 91, "SOCKS4 request rejected");
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn ip_literal_is_refused_without_connecting() {
        let harness = start(None).await;
        let status = socks5_connect_ip(harness.socks, [203, 0, 113, 7], 443).await;
        assert_eq!(status, 0x02);
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn bind_command_is_refused() {
        let harness = start(None).await;
        let mut stream = TcpStream::connect(harness.socks).await.unwrap();
        stream.write_all(&[5, 1, 0]).await.unwrap();
        let mut method = [0u8; 2];
        stream.read_exact(&mut method).await.unwrap();
        // BIND (2) is not something Tor supports. Depending on where the
        // handshake rejects it, the client sees an error reply or a close.
        let mut request = vec![5, 2, 0, 3, ONION.len() as u8];
        request.extend_from_slice(ONION.as_bytes());
        request.extend_from_slice(&80u16.to_be_bytes());
        stream.write_all(&request).await.unwrap();
        let mut head = [0u8; 2];
        if stream.read_exact(&mut head).await.is_ok() {
            assert_ne!(head[1], 0x00, "BIND must not succeed");
        }
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn tor_failure_becomes_socks_error() {
        let harness = start(Some(ConnectFailure::HostUnreachable)).await;
        let (_stream, status) = socks5_connect(harness.socks, ONION, 18081).await;
        assert_eq!(status, 0x04, "host unreachable");
    }

    #[tokio::test]
    async fn garbage_handshake_is_dropped() {
        let harness = start(None).await;
        let mut stream = TcpStream::connect(harness.socks).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
        let mut sink = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut sink))
            .await
            .expect("connection must be closed, not left hanging");
        assert_eq!(harness.connector.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn shutdown_stops_accepting() {
        let harness = start(None).await;
        harness._shutdown.send(true).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let attempt = TcpStream::connect(harness.socks).await;
        if let Ok(mut stream) = attempt {
            // The OS may still complete the TCP handshake from the backlog;
            // what matters is that nothing answers.
            let _ = stream.write_all(&[5, 1, 0]).await;
            let mut method = [0u8; 2];
            let answered =
                tokio::time::timeout(Duration::from_millis(500), stream.read_exact(&mut method))
                    .await;
            assert!(!matches!(answered, Ok(Ok(_))), "listener must be stopped");
        }
    }
}
