//! Lifecycle of the embedded Tor client.
//!
//! [`TorService::start`] returns immediately. Tor bootstraps in the
//! background on the service's own threads, and the caller polls
//! [`TorService::status`]. The SOCKS listeners are bound before bootstrap
//! finishes, so their ports are known at once; until Tor is ready they
//! answer every request with a SOCKS error.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arti_client::config::TorClientConfigBuilder;
use arti_client::{
    BootstrapBehavior, DataStream, ErrorKind, HasKind, StreamPrefs, TorClient, TorClientConfig,
};
use futures::StreamExt;
use tokio::sync::watch;
use tor_rtcompat::PreferredRuntime;

use crate::bridges::Bridges;
use crate::socks::{self, ConnectFailure, Connector};

/// Where Tor keeps its files. Both directories must be private to the app.
#[derive(Debug, Clone)]
pub struct TorConfig {
    /// Long-lived state: guard relays and keys.
    pub state_dir: PathBuf,
    /// Cached directory information. Safe to delete; Tor downloads it again.
    pub cache_dir: PathBuf,
    /// Bridges to connect through, for a network that blocks Tor.
    /// [`Bridges::none`] connects directly.
    pub bridges: Bridges,
}

/// What a SOCKS listener is used for. Each purpose has its own listener and
/// its own Tor circuits, so activity on one cannot be linked to the other by
/// a shared circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    /// Wallet synchronisation with a Monero node.
    Sync = 0,
    /// Broadcasting a transaction. Every connection gets a new circuit.
    Broadcast = 1,
}

/// Coarse lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    /// Tor is connecting to the network.
    Bootstrapping = 0,
    /// Tor can carry traffic.
    Ready = 1,
    /// Tor could not start. See [`Status::message`].
    Failed = 2,
    /// Tor was ready and can no longer reach its own network: the last
    /// connection it was asked for failed on the way to the first relay,
    /// which is what a device without a network looks like. Tor is still
    /// running and takes requests; the first that gets into the Tor
    /// network makes the state [`State::Ready`] again.
    ///
    /// Tor does not report this by itself. It is read from the connections
    /// the wallet asks for, so the state changes only when one is asked
    /// for, and a loss that shows as nothing but timeouts is not told
    /// apart from a node that does not answer.
    Lost = 3,
}

/// A snapshot of the service for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    /// Bootstrap progress, 0 to 100.
    pub percent: u8,
    /// Why Tor is stuck, failed or lost, when known. While bootstrapping, a
    /// message here usually means the network blocks Tor and bridges are
    /// needed.
    pub message: Option<String>,
}

/// Errors from starting the service.
#[derive(Debug, thiserror::Error)]
pub enum TorError {
    #[error("cannot prepare Tor directory {path}: {source}")]
    Directory { path: PathBuf, source: io::Error },
    #[error("cannot start the Tor runtime: {0}")]
    Runtime(io::Error),
    #[error("invalid Tor configuration: {0}")]
    Config(String),
    #[error("cannot create the Tor client: {0}")]
    Client(String),
    #[error("cannot open a local listener: {0}")]
    Listener(io::Error),
    /// A bridge line cannot be read. The line itself is not repeated here.
    #[error("bridge line {number} is not a bridge: {reason}")]
    Bridge { number: usize, reason: String },
    #[error("there is no program for the {transport} transport that a bridge needs")]
    TransportMissing { transport: String },
    #[error("the transport program {path} does not exist")]
    TransportProgram { path: PathBuf },
    #[error("{name} is not the name of a transport")]
    TransportName { name: String },
}

/// A running embedded Tor client with its loopback SOCKS listeners.
///
/// Dropping the service stops Tor and closes the listeners.
pub struct TorService {
    shared: Arc<Shared>,
    sync_port: u16,
    broadcast_port: u16,
    shutdown: watch::Sender<bool>,
    runtime: Option<tokio::runtime::Runtime>,
}

struct Shared {
    state: AtomicU8,
    percent: AtomicU8,
    message: Mutex<Option<String>>,
    /// See [`TorService::last_failure`].
    last_failure: Mutex<Option<String>>,
}

impl Shared {
    fn set_message(&self, message: Option<String>) {
        *self.message.lock().unwrap_or_else(|e| e.into_inner()) = message;
    }

    fn set_last_failure(&self, kind: Option<String>) {
        *self.last_failure.lock().unwrap_or_else(|e| e.into_inner()) = kind;
    }

    fn status(&self) -> Status {
        let state = match self.state.load(Ordering::SeqCst) {
            s if s == State::Ready as u8 => State::Ready,
            s if s == State::Failed as u8 => State::Failed,
            s if s == State::Lost as u8 => State::Lost,
            _ => State::Bootstrapping,
        };
        let message = self
            .message
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Status {
            state,
            percent: self.percent.load(Ordering::SeqCst),
            message,
        }
    }

    /// A connection failed before it got into the Tor network. Only a Tor
    /// that was ready can lose its network; one that is still starting
    /// says so through its bootstrap.
    fn network_lost(&self, why: String) {
        if self.swap_state(State::Ready, State::Lost) {
            self.set_message(Some(why));
        }
    }

    /// Something came back from inside the Tor network, so the way into
    /// it is there.
    fn network_reached(&self) {
        if self.swap_state(State::Lost, State::Ready) {
            self.set_message(None);
        }
    }

    /// Goes from `from` to `to`, and says whether it did: nothing happens
    /// in any other state.
    fn swap_state(&self, from: State, to: State) -> bool {
        self.state
            .compare_exchange(from as u8, to as u8, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

/// What a connection that was asked of Tor says about Tor's own way into
/// its network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Network {
    /// Tor could not get to the Tor network at all.
    Lost,
    /// An answer came from inside the Tor network, good or bad.
    Reached,
    /// Neither is certain. A timeout is of this kind: it is what a network
    /// that swallows everything looks like, and also a service that is
    /// not there.
    Unknown,
}

/// What a failed connection says about the network, by the kind Arti gives
/// its error.
fn network_after(kind: ErrorKind) -> Network {
    match kind {
        // The connection to the first relay could not be made or broke.
        ErrorKind::TorAccessFailed | ErrorKind::LocalNetworkError => Network::Lost,
        // Arti gives these kinds only after a directory inside the Tor
        // network, or the service itself, has answered.
        ErrorKind::OnionServiceNotFound
        | ErrorKind::OnionServiceNotRunning
        | ErrorKind::RemoteConnectionRefused => Network::Reached,
        _ => Network::Unknown,
    }
}

impl TorService {
    /// Starts Tor in the background and binds the SOCKS listeners.
    pub fn start(config: TorConfig) -> Result<TorService, TorError> {
        // Checked first: a wrong bridge setting stops everything before a
        // directory is made or a thread started.
        let mut builder =
            TorClientConfigBuilder::from_directories(&config.state_dir, &config.cache_dir);
        config.bridges.apply(&mut builder)?;
        let tor_config: TorClientConfig = builder
            .build()
            .map_err(|e| TorError::Config(e.to_string()))?;

        prepare_private_dir(&config.state_dir)?;
        prepare_private_dir(&config.cache_dir)?;

        // Arti requires the application to choose the TLS cryptography
        // provider. Installing it twice is harmless, so the result is ignored.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("veil-tor")
            .enable_all()
            .build()
            .map_err(TorError::Runtime)?;

        let shared = Arc::new(Shared {
            state: AtomicU8::new(State::Bootstrapping as u8),
            percent: AtomicU8::new(0),
            message: Mutex::new(None),
            last_failure: Mutex::new(None),
        });
        let (shutdown, shutdown_rx) = watch::channel(false);

        let (client, sync, broadcast) = runtime.block_on(async {
            let tor_runtime = PreferredRuntime::current()
                .map_err(|e| TorError::Runtime(io::Error::other(e.to_string())))?;
            let client = TorClient::with_runtime(tor_runtime)
                .config(tor_config)
                .bootstrap_behavior(BootstrapBehavior::Manual)
                .create_unbootstrapped()
                .map_err(|e| TorError::Client(e.to_string()))?;
            let sync = socks::bind_loopback().await.map_err(TorError::Listener)?;
            let broadcast = socks::bind_loopback().await.map_err(TorError::Listener)?;
            Ok::<_, TorError>((client, sync, broadcast))
        })?;

        let (sync_listener, sync_addr) = sync;
        let (broadcast_listener, broadcast_addr) = broadcast;

        runtime.spawn(socks::serve(
            sync_listener,
            Arc::new(TorConnector::new(&client, &shared, Purpose::Sync)),
            shutdown_rx.clone(),
        ));
        runtime.spawn(socks::serve(
            broadcast_listener,
            Arc::new(TorConnector::new(&client, &shared, Purpose::Broadcast)),
            shutdown_rx.clone(),
        ));
        runtime.spawn(watch_progress(
            Arc::clone(&client),
            Arc::clone(&shared),
            shutdown_rx.clone(),
        ));
        runtime.spawn(bootstrap(client, Arc::clone(&shared), shutdown_rx));

        Ok(TorService {
            shared,
            sync_port: sync_addr.port(),
            broadcast_port: broadcast_addr.port(),
            shutdown,
            runtime: Some(runtime),
        })
    }

    /// Current state, progress, and any message for the user.
    pub fn status(&self) -> Status {
        self.shared.status()
    }

    /// What kind of failure the last connection ended in, in Arti's words
    /// for the kind, if the last connection asked for failed. For finding
    /// out why a node could not be reached: the kind names no relay and
    /// no node.
    pub fn last_failure(&self) -> Option<String> {
        self.shared
            .last_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The loopback port of the SOCKS listener for `purpose`.
    pub fn socks_port(&self, purpose: Purpose) -> u16 {
        match purpose {
            Purpose::Sync => self.sync_port,
            Purpose::Broadcast => self.broadcast_port,
        }
    }

    /// Stops Tor and closes the listeners.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

impl Drop for TorService {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Runs the bootstrap and records how it ended.
async fn bootstrap(
    client: Arc<TorClient<PreferredRuntime>>,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::select! {
        _ = shutdown.changed() => {}
        result = client.bootstrap() => match result {
            Ok(()) => {
                shared.percent.store(100, Ordering::SeqCst);
                shared.set_message(None);
                shared.state.store(State::Ready as u8, Ordering::SeqCst);
            }
            Err(error) => {
                shared.set_message(Some(error.to_string()));
                shared.state.store(State::Failed as u8, Ordering::SeqCst);
            }
        }
    }
}

/// Mirrors bootstrap progress into `shared` until shutdown.
async fn watch_progress(
    client: Arc<TorClient<PreferredRuntime>>,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut events = client.bootstrap_events();
    loop {
        tokio::select! {
            _ = shutdown.changed() => return,
            event = events.next() => {
                let Some(status) = event else { return };
                let percent = (status.as_frac().clamp(0.0, 1.0) * 100.0).round() as u8;
                shared.percent.store(percent, Ordering::SeqCst);
                if shared.state.load(Ordering::SeqCst) == State::Bootstrapping as u8 {
                    shared.set_message(status.blocked().map(|b| b.message().to_string()));
                }
            }
        }
    }
}

/// Opens onion-service streams through the Tor client.
struct TorConnector {
    client: Arc<TorClient<PreferredRuntime>>,
    shared: Arc<Shared>,
    prefs: StreamPrefs,
}

impl TorConnector {
    fn new(
        client: &Arc<TorClient<PreferredRuntime>>,
        shared: &Arc<Shared>,
        purpose: Purpose,
    ) -> Self {
        // An isolated client never shares circuits with the client it was
        // made from, which keeps the two purposes apart.
        let client = client.isolated_client();
        let mut prefs = StreamPrefs::new();
        if purpose == Purpose::Broadcast {
            prefs.isolate_every_stream();
        }
        TorConnector {
            client,
            shared: Arc::clone(shared),
            prefs,
        }
    }
}

impl Connector for TorConnector {
    type Stream = DataStream;

    async fn connect(&self, onion_host: &str, port: u16) -> Result<DataStream, ConnectFailure> {
        // Until Tor is ready, refuse at once. Arti would otherwise hold the
        // request until bootstrap finishes, which can be never on a network
        // that blocks Tor, and the caller would hang instead of retrying.
        // A Tor that has lost its network is asked all the same: that it
        // has its network back shows in nothing but a connection that gets
        // through.
        let state = self.shared.state.load(Ordering::SeqCst);
        if state != State::Ready as u8 && state != State::Lost as u8 {
            return Err(ConnectFailure::General);
        }
        let error = match self
            .client
            .connect_with_prefs((onion_host, port), &self.prefs)
            .await
        {
            Ok(stream) => {
                self.shared.set_last_failure(None);
                self.shared.network_reached();
                return Ok(stream);
            }
            Err(error) => error,
        };
        let kind = error.kind();
        self.shared.set_last_failure(Some(kind.to_string()));
        match network_after(kind) {
            Network::Lost => {
                // The kind in Arti's words, not the error: the error names
                // the relays that were tried.
                self.shared.network_lost(kind.to_string());
                return Err(ConnectFailure::NetworkUnreachable);
            }
            Network::Reached => self.shared.network_reached(),
            Network::Unknown => {}
        }
        Err(match kind {
            ErrorKind::OnionServiceNotFound
            | ErrorKind::OnionServiceNotRunning
            | ErrorKind::OnionServiceAddressInvalid
            | ErrorKind::OnionServiceConnectionFailed
            | ErrorKind::RemoteHostNotFound
            | ErrorKind::RemoteConnectionRefused => ConnectFailure::HostUnreachable,
            ErrorKind::TorNetworkTimeout | ErrorKind::RemoteNetworkTimeout => {
                ConnectFailure::TimedOut
            }
            ErrorKind::InvalidStreamTarget | ErrorKind::ForbiddenStreamTarget => {
                ConnectFailure::NotAllowed
            }
            _ => ConnectFailure::General,
        })
    }
}

/// Creates `path` if needed and restricts it to the current user.
fn prepare_private_dir(path: &Path) -> Result<(), TorError> {
    let wrap = |source: io::Error| TorError::Directory {
        path: path.to_path_buf(),
        source,
    };
    fs::create_dir_all(path).map_err(wrap)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(wrap)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dirs(name: &str) -> (PathBuf, TorConfig) {
        let root = std::env::temp_dir().join(format!("veil-tor-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let config = TorConfig {
            state_dir: root.join("state"),
            cache_dir: root.join("cache"),
            bridges: Bridges::none(),
        };
        (root, config)
    }

    fn shared_in(state: State) -> Shared {
        Shared {
            state: AtomicU8::new(state as u8),
            percent: AtomicU8::new(100),
            message: Mutex::new(None),
            last_failure: Mutex::new(None),
        }
    }

    /// The loss of the network is read from the connections asked for: one
    /// that cannot get to the first relay makes a ready Tor lost, with
    /// words for it, and the first answer from inside the Tor network
    /// makes it ready again.
    #[test]
    fn a_ready_tor_is_lost_and_found_by_what_its_connections_say() {
        let shared = shared_in(State::Ready);

        shared.network_lost("error connecting to Tor".to_owned());
        let status = shared.status();
        assert_eq!(status.state, State::Lost);
        assert_eq!(status.message.as_deref(), Some("error connecting to Tor"));

        // Said again, it stays as it is.
        shared.network_lost("something else".to_owned());
        assert_eq!(
            shared.status().message.as_deref(),
            Some("error connecting to Tor")
        );

        shared.network_reached();
        let status = shared.status();
        assert_eq!(status.state, State::Ready);
        assert_eq!(status.message, None);
    }

    /// Before its bootstrap has ended Tor has no network to lose, and a
    /// Tor that could not start does not come to life through a
    /// connection: both states stay what they are, with their messages.
    #[test]
    fn only_a_tor_that_was_ready_loses_its_network() {
        for state in [State::Bootstrapping, State::Failed] {
            let shared = shared_in(state);
            shared.set_message(Some("why".to_owned()));
            shared.network_lost("error connecting to Tor".to_owned());
            assert_eq!(shared.status().state, state);
            shared.network_reached();
            let status = shared.status();
            assert_eq!(status.state, state);
            assert_eq!(status.message.as_deref(), Some("why"));
        }
    }

    /// Which of Arti's kinds of failure say that the network is gone,
    /// which that it is there, and which say neither. A timeout says
    /// neither: reading it as a loss would blame Tor for every node that
    /// is down, and reading it as an answer would call a network that
    /// swallows everything a working one.
    #[test]
    fn a_failure_is_read_for_what_it_says_about_the_network() {
        for kind in [ErrorKind::TorAccessFailed, ErrorKind::LocalNetworkError] {
            assert_eq!(network_after(kind), Network::Lost, "{kind:?}");
        }
        for kind in [
            ErrorKind::OnionServiceNotFound,
            ErrorKind::OnionServiceNotRunning,
            ErrorKind::RemoteConnectionRefused,
        ] {
            assert_eq!(network_after(kind), Network::Reached, "{kind:?}");
        }
        for kind in [
            ErrorKind::TorNetworkTimeout,
            ErrorKind::RemoteNetworkTimeout,
            ErrorKind::OnionServiceConnectionFailed,
            ErrorKind::TransientFailure,
            ErrorKind::Internal,
        ] {
            assert_eq!(network_after(kind), Network::Unknown, "{kind:?}");
        }
    }

    /// Starting must not wait for the network: the listeners come up at once
    /// and Tor reports that it is still bootstrapping.
    #[test]
    #[ignore = "starts a Tor client, which tries to reach the Tor network"]
    fn starts_without_network_and_reports_bootstrapping() {
        veil_test_guard::refuse_in_hosted_ci("starting a Tor client");
        let (root, config) = temp_dirs("start");
        let service = TorService::start(config).expect("service starts");

        let sync = service.socks_port(Purpose::Sync);
        let broadcast = service.socks_port(Purpose::Broadcast);
        assert_ne!(sync, 0);
        assert_ne!(broadcast, 0);
        assert_ne!(sync, broadcast, "each purpose has its own listener");

        let status = service.status();
        assert_ne!(status.state, State::Ready, "cannot be ready instantly");
        assert!(status.percent < 100);

        service.stop();
        let _ = fs::remove_dir_all(root);
    }

    /// Before Tor is ready, a request must be refused rather than carried
    /// some other way.
    #[test]
    #[ignore = "starts a Tor client, which tries to reach the Tor network"]
    fn refuses_onion_connect_before_tor_is_ready() {
        veil_test_guard::refuse_in_hosted_ci("starting a Tor client");
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let (root, config) = temp_dirs("failclosed");
        let service = TorService::start(config).expect("service starts");
        let port = service.socks_port(Purpose::Sync);

        let host = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream.write_all(&[5, 1, 0]).unwrap();
        let mut method = [0u8; 2];
        stream.read_exact(&mut method).unwrap();
        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&18081u16.to_be_bytes());
        stream.write_all(&request).unwrap();
        let mut head = [0u8; 2];
        stream.read_exact(&mut head).unwrap();
        assert_ne!(head[1], 0x00, "must not report success without Tor");

        service.stop();
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "starts a Tor client, which tries to reach the Tor network"]
    fn tor_directories_are_private() {
        veil_test_guard::refuse_in_hosted_ci("starting a Tor client");
        use std::os::unix::fs::PermissionsExt;

        let (root, config) = temp_dirs("perms");
        let service = TorService::start(config.clone()).expect("service starts");
        for dir in [&config.state_dir, &config.cache_dir] {
            let mode = fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} must be private", dir.display());
        }
        service.stop();
        let _ = fs::remove_dir_all(root);
    }
}
