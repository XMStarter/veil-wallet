//! A stand-in for the Tor network, for tests.
//!
//! [`StandIn`] runs the same loopback SOCKS listeners as [`TorService`],
//! with the same rules, but carries a connection to a port on this machine
//! instead of into Tor. A test maps an onion name to a local port, points
//! the code under test at the listeners, and afterwards reads which targets
//! were asked for and through which listener.
//!
//! It exists only with the `stand-in` feature, which no app build enables.
//!
//! [`TorService`]: crate::TorService

use std::collections::HashMap;
use std::io;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    type Stream = TcpStream;

    async fn connect(&self, onion_host: &str, port: u16) -> Result<TcpStream, ConnectFailure> {
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
        TcpStream::connect((Ipv4Addr::LOCALHOST, local_port))
            .await
            .map_err(|_| ConnectFailure::HostUnreachable)
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
