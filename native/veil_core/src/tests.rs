//! The core against a stand-in for Tor.
//!
//! Nothing here leaves the machine. The tests that open a wallet need the
//! wallet library (`VEIL_WALLET_LIB`) and are skipped without it.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use veil_tor::{Purpose, StandIn, State};
use veil_wallet::{Error, Network, Wallet, WalletLibrary};

use crate::{Core, CoreError, Nodes, OnionNode};

const PASSWORD: &str = "correct horse battery staple";
const SYNC_HOST: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";
const BROADCAST_HOST: &str = "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion";
const SYNC_PORT: u16 = 18081;
const BROADCAST_PORT: u16 = 18089;

fn nodes() -> Nodes {
    Nodes::separate(
        OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
        OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
    )
    .unwrap()
}

/// Loads the wallet library named by `VEIL_WALLET_LIB`, or skips the test.
/// `VEIL_REQUIRE_WALLET_LIB=1` turns the skip into a failure.
fn wallet_library() -> Option<WalletLibrary> {
    match std::env::var_os("VEIL_WALLET_LIB") {
        Some(path) => Some(WalletLibrary::load(path).expect("wallet library loads")),
        None => {
            assert!(
                std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
                "VEIL_WALLET_LIB is not set but the wallet library is required"
            );
            eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
            None
        }
    }
}

/// A fresh directory for one test's wallet files, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("veil-core-{name}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A core with a new wallet on top of `stand_in`.
fn core_on(lib: &WalletLibrary, dir: &TempDir, stand_in: &StandIn) -> Core {
    let (wallet, _) = Wallet::create(lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();
    Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap()
}

#[test]
fn nothing_is_asked_of_a_node_while_tor_is_not_ready() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("not-ready");
    let stand_in = StandIn::start().unwrap();
    stand_in.set_ready(false);
    let mut core = core_on(&lib, &dir, &stand_in);

    assert_eq!(core.tor_status().state, State::Bootstrapping);
    assert!(matches!(core.sync(), Err(CoreError::TorNotReady { .. })));
    assert!(matches!(
        core.node_height(),
        Err(CoreError::TorNotReady { .. })
    ));
    let address = core.receive_address().unwrap().address;
    assert!(matches!(
        core.prepare_send(&address, veil_wallet::SendAmount::Everything),
        Err(CoreError::TorNotReady { .. })
    ));
    assert!(
        stand_in.requests().is_empty(),
        "the wallet must not try a node before Tor is ready: {:?}",
        stand_in.requests()
    );
}

#[test]
fn the_wallet_asks_for_its_sync_node_by_name_through_the_sync_listener() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("sync-listener");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);

    // No route: the onion service is unreachable.
    assert!(matches!(
        core.sync(),
        Err(CoreError::Wallet(Error::NodeUnreachable))
    ));

    let requests = stand_in.requests();
    assert!(!requests.is_empty(), "the wallet never used the proxy");
    for request in &requests {
        // The name arrives at the proxy unresolved, on the sync listener,
        // and the broadcast node is not mentioned at all.
        assert_eq!(request.purpose, Purpose::Sync, "{requests:?}");
        assert_eq!(request.host, SYNC_HOST, "{requests:?}");
        assert_eq!(request.port, SYNC_PORT, "{requests:?}");
    }
}

#[test]
fn without_the_listeners_there_is_no_connection_at_all() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("no-listeners");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);

    // Tor has died but still reads as ready. The wallet has only the dead
    // loopback ports to try; it must fail, not find another way out.
    stand_in.close_listeners();
    assert_eq!(core.tor_status().state, State::Ready);
    assert!(matches!(
        core.sync(),
        Err(CoreError::Wallet(Error::NodeUnreachable))
    ));
    assert!(stand_in.requests().is_empty());
}

#[test]
fn something_that_is_not_a_monero_node_is_not_synchronised_with() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("not-a-node");
    let stand_in = StandIn::start().unwrap();

    // A web server where the node should be.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    stand_in.route(SYNC_HOST, SYNC_PORT, listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let body = "<html>hello</html>";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });

    let mut core = core_on(&lib, &dir, &stand_in);
    let before = core.sync_progress().wallet_height;
    assert!(core.sync().is_err());
    assert_eq!(core.sync_progress().wallet_height, before);
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync && request.host == SYNC_HOST));
}

#[test]
fn a_reopened_wallet_is_joined_to_tor_the_same_way() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("reopen");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let address = core.receive_address().unwrap();
    core.close().unwrap();

    let wallet = Wallet::open(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();
    let mut core = Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap();
    assert_eq!(core.receive_address().unwrap(), address);
    stand_in.clear_requests();
    assert!(core.sync().is_err());
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync && request.host == SYNC_HOST));
    assert!(!stand_in.requests().is_empty());
}

#[test]
fn nodes_are_tried_in_order_until_one_answers() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("settle");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    stand_in.clear_requests();

    // Neither candidate is reachable. Both are tried, in the order given,
    // each through the sync listener, and the outcome names the count.
    let first = OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap();
    let second = OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap();
    let outcome = core.settle_on_nodes(&[first.clone(), second.clone()]);
    assert!(
        matches!(
            outcome,
            Err(CoreError::NoNodeAnswered {
                tried: 2,
                last: Error::NodeUnreachable
            })
        ),
        "{outcome:?}"
    );
    let hosts: Vec<String> = stand_in.requests().into_iter().map(|r| r.host).collect();
    let mut order = hosts.clone();
    order.dedup();
    assert_eq!(order, vec![BROADCAST_HOST, SYNC_HOST], "{hosts:?}");
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync));

    // The last candidate tried is the one the core is left on, with the
    // other as its broadcast node.
    assert_eq!(core.nodes().sync(), &second);
    assert_eq!(core.nodes().broadcast(), &first);

    // One candidate serves both purposes.
    let _ = core.settle_on_nodes(std::slice::from_ref(&first));
    assert_eq!(core.nodes().sync(), core.nodes().broadcast());

    // And before Tor is ready no candidate is tried at all.
    stand_in.set_ready(false);
    stand_in.clear_requests();
    assert!(matches!(
        core.settle_on_nodes(&[first, second]),
        Err(CoreError::TorNotReady { .. })
    ));
    assert!(stand_in.requests().is_empty());
}

#[cfg(feature = "regtest")]
mod regtest;
