//! A stand-in for the Tor network, for tests.
//!
//! [`StandIn`] runs the same loopback SOCKS listeners as [`TorService`],
//! with the same rules, but carries a connection to a port on this machine
//! instead of into Tor. A test maps an onion name to a local port, points
//! the code under test at the listeners, and afterwards reads which targets
//! were asked for and through which listener.
//!
//! It can also lose a connection on purpose, at a chosen point of the
//! exchange ([`StandIn::cut_once`]), so that what the wallet does when Tor or
//! a node drops away in the middle of its work can be tested without a
//! network.
//!
//! It exists only with the `stand-in` feature, which no app build enables.
//!
//! [`TorService`]: crate::TorService

use std::collections::HashMap;
use std::io;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::net::TcpStream;
use tokio::sync::watch;

use crate::service::{Purpose, State, Status};
use crate::socks::{self, ConnectFailure, Connector};

/// One connection that was asked of the stand-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The listener the request arrived on.
    pub purpose: Purpose,
    pub host: String,
    pub port: u16,
}

/// What the wallet finds when it tries again after a connection was cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterCut {
    /// Only that connection was lost. A new one gets through.
    NodeReachable,
    /// The onion service is gone: its route is removed, as if by
    /// [`StandIn::unroute`].
    NodeGone,
    /// Tor itself has lost its connection: the stand-in stops being ready,
    /// as if by [`StandIn::set_ready`].
    TorNotReady,
}

/// Where a connection is cut. Bytes are counted from the moment the cut is
/// armed, over every connection to its node, and are whatever travels on
/// the connection: the wallet library encrypts its exchange with a real
/// node, so a cut cannot be told to wait for a particular request by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutPoint {
    /// In the middle of what the node sends: this many bytes still come
    /// back, and then the connection is closed. With a number larger than
    /// the small answers that open an exchange and smaller than a download
    /// of blocks, this is a synchronisation losing its connection part way.
    AfterReplyBytes(usize),
    /// After the wallet has sent this many bytes. What it sends reaches the
    /// node, the rest of its request included; nothing more comes back, and
    /// a moment later the connection is closed. With a number larger than
    /// the opening of an exchange and smaller than a transaction, this is a
    /// transaction that reached the node while its sender never learned so.
    AfterRequestBytes(usize),
}

/// A cut that has been asked for and has not happened yet.
struct Cut {
    host: String,
    port: u16,
    at: CutPoint,
    after: AfterCut,
    /// Bytes seen since the cut was armed, in the direction it counts.
    seen: usize,
}

/// Loopback SOCKS listeners that lead to local ports instead of into Tor.
///
/// A clone is a second handle to the same listeners, so a test can hand one
/// to the code under test and keep one to steer and inspect. The listeners
/// close when the last handle is dropped.
#[derive(Clone)]
pub struct StandIn {
    shared: Arc<Shared>,
    sync_port: u16,
    broadcast_port: u16,
    runner: Arc<Runner>,
}

/// Owns the threads the listeners run on.
struct Runner {
    shutdown: watch::Sender<bool>,
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

impl Drop for Runner {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(runtime) = lock(&self.runtime).take() {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

struct Shared {
    ready: AtomicBool,
    routes: Mutex<HashMap<(String, u16), u16>>,
    requests: Mutex<Vec<Request>>,
    cut: Mutex<Option<Cut>>,
    cuts: AtomicU32,
}

impl Shared {
    /// Counts `count` bytes on their way to `host:port`. Returns the armed
    /// cut if this is where it happens.
    fn sent_to_node(&self, host: &str, port: u16, count: usize) -> Option<Cut> {
        let mut armed = lock(&self.cut);
        let cut = armed.as_mut()?;
        let CutPoint::AfterRequestBytes(limit) = cut.at else {
            return None;
        };
        if cut.host != host || cut.port != port {
            return None;
        }
        cut.seen += count;
        if cut.seen >= limit {
            armed.take()
        } else {
            None
        }
    }

    /// Counts `count` bytes that came back from `host:port`. Returns the
    /// armed cut, and how many of these bytes may still pass, if this is
    /// where it happens.
    fn sent_by_node(&self, host: &str, port: u16, count: usize) -> Option<(Cut, usize)> {
        let mut armed = lock(&self.cut);
        let cut = armed.as_mut()?;
        let CutPoint::AfterReplyBytes(limit) = cut.at else {
            return None;
        };
        if cut.host != host || cut.port != port {
            return None;
        }
        let before = cut.seen;
        cut.seen += count;
        if cut.seen >= limit {
            armed.take().map(|cut| (cut, limit - before))
        } else {
            None
        }
    }

    /// Records that `cut` has happened and leaves behind what it asked for.
    fn after(&self, cut: &Cut) {
        match cut.after {
            AfterCut::NodeReachable => {}
            AfterCut::NodeGone => {
                lock(&self.routes).remove(&(cut.host.clone(), cut.port));
            }
            AfterCut::TorNotReady => self.ready.store(false, Ordering::SeqCst),
        }
        self.cuts.fetch_add(1, Ordering::SeqCst);
    }
}

impl StandIn {
    /// Binds the listeners. The stand-in starts out ready, like Tor after
    /// its bootstrap; see [`StandIn::set_ready`].
    pub fn start() -> io::Result<StandIn> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("veil-tor-stand-in")
            .enable_all()
            .build()?;
        let shared = Arc::new(Shared {
            ready: AtomicBool::new(true),
            routes: Mutex::new(HashMap::new()),
            requests: Mutex::new(Vec::new()),
            cut: Mutex::new(None),
            cuts: AtomicU32::new(0),
        });
        let (shutdown, shutdown_rx) = watch::channel(false);

        let (sync, broadcast) = runtime.block_on(async {
            let sync = socks::bind_loopback().await?;
            let broadcast = socks::bind_loopback().await?;
            Ok::<_, io::Error>((sync, broadcast))
        })?;
        let (sync_listener, sync_addr) = sync;
        let (broadcast_listener, broadcast_addr) = broadcast;

        for (listener, purpose) in [
            (sync_listener, Purpose::Sync),
            (broadcast_listener, Purpose::Broadcast),
        ] {
            runtime.spawn(socks::serve(
                listener,
                Arc::new(LocalConnector {
                    shared: Arc::clone(&shared),
                    purpose,
                }),
                shutdown_rx.clone(),
            ));
        }

        Ok(StandIn {
            shared,
            sync_port: sync_addr.port(),
            broadcast_port: broadcast_addr.port(),
            runner: Arc::new(Runner {
                shutdown,
                runtime: Mutex::new(Some(runtime)),
            }),
        })
    }

    /// Makes `onion_host:onion_port` lead to `local_port` on `127.0.0.1`.
    /// A target without a route is answered as an unreachable onion service.
    pub fn route(&self, onion_host: &str, onion_port: u16, local_port: u16) {
        lock(&self.shared.routes).insert((onion_host.to_ascii_lowercase(), onion_port), local_port);
    }

    /// Removes a route: the onion service stops being reachable.
    /// Connections that are already open stay open.
    pub fn unroute(&self, onion_host: &str, onion_port: u16) {
        lock(&self.shared.routes).remove(&(onion_host.to_ascii_lowercase(), onion_port));
    }

    /// Arms one cut, replacing an earlier one that has not happened.
    ///
    /// The connection to `onion_host:onion_port` that reaches `at` is
    /// closed there, whether it was open already or is opened later, and
    /// `after` is what the wallet finds when it tries again.
    /// [`StandIn::cuts`] tells whether it has happened.
    pub fn cut_once(&self, onion_host: &str, onion_port: u16, at: CutPoint, after: AfterCut) {
        *lock(&self.shared.cut) = Some(Cut {
            host: onion_host.to_ascii_lowercase(),
            port: onion_port,
            at,
            after,
            seen: 0,
        });
    }

    /// How many connections have been cut so far.
    pub fn cuts(&self) -> u32 {
        self.shared.cuts.load(Ordering::SeqCst)
    }

    /// Switches between Tor being ready and Tor still bootstrapping. While
    /// not ready, every request is refused at once, as [`TorService`] does.
    ///
    /// [`TorService`]: crate::TorService
    pub fn set_ready(&self, ready: bool) {
        self.shared.ready.store(ready, Ordering::SeqCst);
    }

    /// Every onion target asked for so far, in order. Requests the listener
    /// itself refuses, such as a target that is not an onion name, never get
    /// this far and are not listed.
    pub fn requests(&self) -> Vec<Request> {
        lock(&self.shared.requests).clone()
    }

    /// Forgets the requests seen so far.
    pub fn clear_requests(&self) {
        lock(&self.shared.requests).clear();
    }

    /// The loopback port of the SOCKS listener for `purpose`.
    pub fn socks_port(&self, purpose: Purpose) -> u16 {
        match purpose {
            Purpose::Sync => self.sync_port,
            Purpose::Broadcast => self.broadcast_port,
        }
    }

    /// The state a [`TorService`] would report.
    ///
    /// [`TorService`]: crate::TorService
    pub fn status(&self) -> Status {
        if self.shared.ready.load(Ordering::SeqCst) {
            Status {
                state: State::Ready,
                percent: 100,
                message: None,
            }
        } else {
            Status {
                state: State::Bootstrapping,
                percent: 0,
                message: None,
            }
        }
    }

    /// Closes the listeners while the stand-in keeps reporting whatever
    /// state it was in, like a Tor client that has died without anyone
    /// noticing yet. Connections to the ports are refused from then on.
    pub fn close_listeners(&self) {
        let _ = self.runner.shutdown.send(true);
        if let Some(runtime) = lock(&self.runner.runtime).take() {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens local connections where the Tor client would open onion streams.
struct LocalConnector {
    shared: Arc<Shared>,
    purpose: Purpose,
}

impl Connector for LocalConnector {
    type Stream = DuplexStream;

    async fn connect(&self, onion_host: &str, port: u16) -> Result<DuplexStream, ConnectFailure> {
        lock(&self.shared.requests).push(Request {
            purpose: self.purpose,
            host: onion_host.to_owned(),
            port,
        });
        if !self.shared.ready.load(Ordering::SeqCst) {
            return Err(ConnectFailure::General);
        }
        let local_port = lock(&self.shared.routes)
            .get(&(onion_host.to_owned(), port))
            .copied()
            .ok_or(ConnectFailure::HostUnreachable)?;
        let node = TcpStream::connect((Ipv4Addr::LOCALHOST, local_port))
            .await
            .map_err(|_| ConnectFailure::HostUnreachable)?;
        let (near, far) = tokio::io::duplex(RELAY_BUFFER);
        tokio::spawn(relay(
            far,
            node,
            Arc::clone(&self.shared),
            onion_host.to_owned(),
            port,
        ));
        Ok(near)
    }
}

const RELAY_BUFFER: usize = 64 * 1024;
/// How long a connection cut by [`CutPoint::AfterRequestBytes`] goes on
/// carrying the wallet's request to the node before it is closed.
const TIME_TO_FINISH_A_REQUEST: Duration = Duration::from_millis(500);

/// Carries one connection between the wallet's side and the local port,
/// and cuts it when an armed cut says so.
async fn relay(
    wallet: DuplexStream,
    mut node: TcpStream,
    shared: Arc<Shared>,
    host: String,
    port: u16,
) {
    let (mut from_wallet, mut to_wallet) = tokio::io::split(wallet);
    let (mut from_node, mut to_node) = node.split();
    let mut request = vec![0u8; RELAY_BUFFER];
    let mut reply = vec![0u8; RELAY_BUFFER];
    let mut wallet_open = true;
    // Set once the wallet has sent as much as a cut waits for: from then
    // on nothing comes back, and at the deadline the connection is closed.
    let mut silenced: Option<(Cut, tokio::time::Instant)> = None;

    loop {
        let deadline = silenced.as_ref().map(|(_, deadline)| *deadline);
        tokio::select! {
            read = from_wallet.read(&mut request), if wallet_open => {
                let count = match read {
                    Ok(count) if count > 0 => count,
                    _ => {
                        wallet_open = false;
                        let _ = to_node.shutdown().await;
                        continue;
                    }
                };
                if to_node.write_all(&request[..count]).await.is_err() {
                    break;
                }
                if silenced.is_none() {
                    if let Some(cut) = shared.sent_to_node(&host, port, count) {
                        let deadline = tokio::time::Instant::now() + TIME_TO_FINISH_A_REQUEST;
                        silenced = Some((cut, deadline));
                    }
                }
            }
            read = from_node.read(&mut reply) => {
                let count = match read {
                    Ok(count) if count > 0 => count,
                    _ => break,
                };
                if silenced.is_some() {
                    continue;
                }
                if let Some((cut, pass)) = shared.sent_by_node(&host, port, count) {
                    let _ = to_wallet.write_all(&reply[..pass]).await;
                    // Dropping both ends closes the connection in the
                    // middle of whatever was being said.
                    shared.after(&cut);
                    return;
                }
                if to_wallet.write_all(&reply[..count]).await.is_err() {
                    break;
                }
            }
            () = tokio::time::sleep_until(deadline.unwrap_or_else(tokio::time::Instant::now)),
                if deadline.is_some() =>
            {
                break;
            }
        }
    }
    if let Some((cut, _)) = &silenced {
        shared.after(cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream as StdStream};

    const ONION: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";

    /// A local server that answers one connection with `reply`.
    fn serve_once(reply: &'static [u8]) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.write_all(reply);
            }
        });
        port
    }

    /// SOCKS4a CONNECT, the dialect the wallet library speaks. Returns the
    /// stream and the reply code: 90 is granted, 91 refused.
    fn socks4a(socks_port: u16, host: &str, port: u16) -> (StdStream, u8) {
        let mut stream = StdStream::connect(("127.0.0.1", socks_port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = vec![4, 1];
        request.extend_from_slice(&port.to_be_bytes());
        request.extend_from_slice(&[0, 0, 0, 1, 0]);
        request.extend_from_slice(host.as_bytes());
        request.push(0);
        stream.write_all(&request).unwrap();
        let mut reply = [0u8; 8];
        stream.read_exact(&mut reply).unwrap();
        (stream, reply[1])
    }

    /// A local server that answers every request line, on any number of
    /// connections, with ten bytes.
    fn serve_digits() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                std::thread::spawn(move || {
                    let mut request = [0u8; 64];
                    while let Ok(count @ 1..) = stream.read(&mut request) {
                        let lines = request[..count].iter().filter(|b| **b == b'\n').count();
                        for _ in 0..lines {
                            if stream.write_all(b"0123456789").is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        });
        port
    }

    /// Reads until the other side closes.
    fn read_to_close(stream: &mut StdStream) -> Vec<u8> {
        let mut all = Vec::new();
        let _ = stream.read_to_end(&mut all);
        all
    }

    #[test]
    fn a_cut_closes_the_connection_part_way_through_the_reply() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        stand_in.cut_once(
            ONION,
            18081,
            CutPoint::AfterReplyBytes(14),
            AfterCut::NodeReachable,
        );
        let socks = stand_in.socks_port(Purpose::Sync);

        // The first answer is shorter than what may pass and arrives whole.
        let (mut stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 90);
        stream.write_all(b"GET /height\n").unwrap();
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"0123456789");
        assert_eq!(stand_in.cuts(), 0);

        // Four bytes of the second arrive, and no more.
        stream.write_all(b"GET /blocks\n").unwrap();
        assert_eq!(read_to_close(&mut stream), b"0123");
        assert_eq!(stand_in.cuts(), 1);

        // It was armed once: the next connection is left alone.
        let (mut stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 90);
        stream.write_all(b"GET /blocks\n").unwrap();
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"0123456789");
        assert_eq!(stand_in.cuts(), 1);
    }

    #[test]
    fn bytes_are_counted_over_every_connection_to_the_node() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        stand_in.cut_once(
            ONION,
            18081,
            CutPoint::AfterReplyBytes(14),
            AfterCut::NodeReachable,
        );
        let socks = stand_in.socks_port(Purpose::Sync);

        let (mut first, _) = socks4a(socks, ONION, 18081);
        first.write_all(b"GET /height\n").unwrap();
        let mut reply = [0u8; 10];
        first.read_exact(&mut reply).unwrap();

        let (mut second, _) = socks4a(socks, ONION, 18081);
        second.write_all(b"GET /blocks\n").unwrap();
        assert_eq!(read_to_close(&mut second), b"0123");
        assert_eq!(stand_in.cuts(), 1);

        // The connection that was not cut still works.
        first.write_all(b"GET /height\n").unwrap();
        first.read_exact(&mut reply).unwrap();
    }

    #[test]
    fn a_cut_waits_for_its_own_node() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        stand_in.route(ONION, 18089, serve_digits());
        stand_in.cut_once(
            ONION,
            18089,
            CutPoint::AfterReplyBytes(0),
            AfterCut::NodeReachable,
        );

        let (mut stream, _) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        stream.write_all(b"GET /blocks\n").unwrap();
        let mut reply = [0u8; 10];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(stand_in.cuts(), 0);
    }

    /// A local server that reads twenty bytes, tells `received` so, and
    /// answers with ten.
    fn serve_after_twenty_bytes(received: std::sync::mpsc::Sender<Vec<u8>>) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 20];
            if stream.read_exact(&mut request).is_ok() {
                let _ = received.send(request.to_vec());
                let _ = stream.write_all(b"0123456789");
            }
        });
        port
    }

    #[test]
    fn a_request_can_reach_the_node_while_its_answer_is_lost() {
        let (received, node_received) = std::sync::mpsc::channel();
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_after_twenty_bytes(received));
        stand_in.cut_once(
            ONION,
            18081,
            CutPoint::AfterRequestBytes(8),
            AfterCut::NodeReachable,
        );

        let (mut stream, _) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        // The request crosses the mark in its first half; the second half
        // still gets through.
        stream.write_all(b"0123456789").unwrap();
        stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        stream.write_all(b"abcdefghij").unwrap();

        assert_eq!(
            node_received.recv_timeout(Duration::from_secs(10)).unwrap(),
            b"0123456789abcdefghij"
        );
        assert_eq!(read_to_close(&mut stream), b"", "no answer comes back");
        assert_eq!(stand_in.cuts(), 1);
    }

    #[test]
    fn after_a_cut_the_node_can_be_gone() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        stand_in.cut_once(
            ONION,
            18081,
            CutPoint::AfterReplyBytes(3),
            AfterCut::NodeGone,
        );
        let socks = stand_in.socks_port(Purpose::Sync);

        let (mut stream, _) = socks4a(socks, ONION, 18081);
        stream.write_all(b"GET /blocks\n").unwrap();
        assert_eq!(read_to_close(&mut stream), b"012");

        let (_stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 91, "the route is gone");
        assert_eq!(stand_in.status().state, State::Ready);
    }

    #[test]
    fn after_a_cut_tor_can_be_not_ready() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        stand_in.cut_once(
            ONION,
            18081,
            CutPoint::AfterReplyBytes(3),
            AfterCut::TorNotReady,
        );
        let socks = stand_in.socks_port(Purpose::Sync);

        let (mut stream, _) = socks4a(socks, ONION, 18081);
        stream.write_all(b"GET /blocks\n").unwrap();
        assert_eq!(read_to_close(&mut stream), b"012");

        assert_eq!(stand_in.status().state, State::Bootstrapping);
        let (_stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 91);
    }

    #[test]
    fn an_unrouted_node_stops_answering() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_digits());
        let socks = stand_in.socks_port(Purpose::Sync);
        let (_stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 90);
        stand_in.unroute(ONION, 18081);
        let (_stream, code) = socks4a(socks, ONION, 18081);
        assert_eq!(code, 91);
    }

    #[test]
    fn a_routed_onion_target_reaches_the_local_port() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_once(b"node"));

        let (mut stream, code) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        assert_eq!(code, 90);
        let mut reply = [0u8; 4];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"node");
        assert_eq!(
            stand_in.requests(),
            vec![Request {
                purpose: Purpose::Sync,
                host: ONION.to_owned(),
                port: 18081,
            }]
        );
    }

    #[test]
    fn each_listener_reports_its_own_purpose() {
        let stand_in = StandIn::start().unwrap();
        let sync = stand_in.socks_port(Purpose::Sync);
        let broadcast = stand_in.socks_port(Purpose::Broadcast);
        assert_ne!(sync, broadcast);

        socks4a(broadcast, ONION, 18089);
        socks4a(sync, ONION, 18081);
        let purposes: Vec<Purpose> = stand_in.requests().iter().map(|r| r.purpose).collect();
        assert_eq!(purposes, vec![Purpose::Broadcast, Purpose::Sync]);

        stand_in.clear_requests();
        assert!(stand_in.requests().is_empty());
    }

    #[test]
    fn an_unrouted_target_is_refused() {
        let stand_in = StandIn::start().unwrap();
        let (_stream, code) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        assert_eq!(code, 91);
        assert_eq!(stand_in.requests().len(), 1);
    }

    #[test]
    fn nothing_is_carried_while_not_ready() {
        let stand_in = StandIn::start().unwrap();
        stand_in.route(ONION, 18081, serve_once(b"node"));
        stand_in.set_ready(false);
        assert_eq!(stand_in.status().state, State::Bootstrapping);

        let (_stream, code) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        assert_eq!(code, 91);

        stand_in.set_ready(true);
        assert_eq!(stand_in.status().state, State::Ready);
        let (_stream, code) = socks4a(stand_in.socks_port(Purpose::Sync), ONION, 18081);
        assert_eq!(code, 90);
    }

    #[test]
    fn closed_listeners_refuse_connections() {
        let stand_in = StandIn::start().unwrap();
        let port = stand_in.socks_port(Purpose::Sync);
        stand_in.clone().close_listeners();
        assert!(StdStream::connect(("127.0.0.1", port)).is_err());
        assert_eq!(stand_in.status().state, State::Ready);
    }

    #[test]
    fn the_onion_only_rule_still_applies() {
        let stand_in = StandIn::start().unwrap();
        let (_stream, code) = socks4a(stand_in.socks_port(Purpose::Sync), "example.com", 80);
        assert_eq!(code, 91);
        assert!(
            stand_in.requests().is_empty(),
            "a clearnet name must be refused before it reaches the connector"
        );
    }
}
