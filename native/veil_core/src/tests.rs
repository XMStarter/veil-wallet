//! The core against a stand-in for Tor.
//!
//! Nothing here leaves the machine. The tests that open a wallet need the
//! wallet library (`VEIL_WALLET_LIB`) and are skipped without it.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use veil_tor::{AfterCut, CutPoint, Purpose, StandIn, State};
use veil_wallet::{Error, Network, Wallet, WalletLibrary};

use crate::{Core, CoreError, Nodes, OnionNode, SyncAttempt};

use fake_node::FakeNode;

const PASSWORD: &str = "correct horse battery staple";
const SYNC_HOST: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";
const BROADCAST_HOST: &str = "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion";
const SYNC_PORT: u16 = 18081;
const BROADCAST_PORT: u16 = 18089;
/// A third onion name, for a candidate that is neither.
const THIRD_HOST: &str = "vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd.onion";

fn nodes() -> Nodes {
    Nodes::separate(
        OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
        OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
    )
    .unwrap()
}

/// Loads the wallet library named by `VEIL_WALLET_LIB`, or skips the test.
/// `VEIL_REQUIRE_WALLET_LIB=1` turns the skip into a failure.
///
/// A test that waits for a session with a limit calls this first, although
/// the session loads the library itself: the first loading in a process
/// has taken over a minute as a Windows program on the owner's machine,
/// and that is no part of what such a test times. Once loaded, the
/// library stays, and the session's own loading costs nothing. A loading
/// that takes long says so, which shows with a test that fails.
fn wallet_library() -> Option<WalletLibrary> {
    match std::env::var_os("VEIL_WALLET_LIB") {
        Some(path) => {
            let started = std::time::Instant::now();
            let library = WalletLibrary::load(path).expect("wallet library loads");
            let took = started.elapsed();
            if took >= Duration::from_secs(5) {
                eprintln!("the wallet library took {} s to load", took.as_secs());
            }
            Some(library)
        }
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

/// A node can send anything. Whatever it sends, the synchronisation ends,
/// the wallet has what it had, and the program is still there: the test
/// reaching its end is the finding, since the library that reads a node's
/// answers is not Rust and takes the whole program with it when it falls.
#[test]
fn made_up_answers_from_a_node_end_the_synchronisation_and_change_nothing() {
    use std::cell::RefCell;

    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("made-up-node");
    let stand_in = StandIn::start().unwrap();
    let node = fake_node::HostileNode::start();
    stand_in.route(SYNC_HOST, SYNC_PORT, node.port());
    let core = RefCell::new(core_on(&lib, &dir, &stand_in));
    let address = core.borrow_mut().receive_address().unwrap().address;
    // No block is counted as scanned: the wallet's own height moves no
    // further than the block it starts scanning from.
    let start = core.borrow().restore_height();

    let good = fake_node::good_answer(3_000_000);
    let far = fake_node::good_answer(u64::MAX);
    let none = fake_node::good_answer(0);
    let web = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 18\r\n\r\n<html>hello</html>";
    let endless = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 99999999999\r\n\r\n{";
    let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n{}";
    // The start of a binary answer as the library's own format begins.
    let binary = b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 12\r\n\r\n\x01\x11\x01\x01\x01\x01\x02\x01\x01\xff\xff\xff";
    let samples: [&[u8]; 8] = [&good, &far, &none, web, endless, chunked, binary, b""];
    veil_test_guard::fuzz::bytes("node answer", &samples, 150, |bytes| {
        node.answer_with(bytes);
        let mut core = core.borrow_mut();
        // It may even be taken for a node; it has no blocks to give.
        let _ = core.sync();
        let scanned = core.sync_progress().wallet_height;
        assert!(scanned <= start, "{scanned} is past the start, {start}");
        assert!(core.balance().total.is_zero());
        assert_eq!(core.receive_address().unwrap().address, address);
    });
    assert_ne!(node.asked(), 0, "the wallet never asked the node anything");
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync && request.host == SYNC_HOST));
    // And the wallet is one that can be closed and saved.
    core.into_inner().close().unwrap();
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

/// How far past the wallet's own start the fake nodes' chain reaches.
const BLOCKS_AHEAD: u64 = 5000;
/// Where a connection is cut: well past the short answers about version
/// and height, and a quarter of the way into the answer to the wallet's
/// first request for blocks.
const MID_BLOCKS: CutPoint = CutPoint::AfterReplyBytes(fake_node::BLOCKS_REPLY_BYTES / 4);

fn asked_for_blocks(node: &FakeNode) -> bool {
    node.asked().iter().any(|path| path.ends_with(".bin"))
}

#[test]
fn a_connection_lost_mid_sync_is_reported_and_leaves_the_wallet_as_it_was() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("cut");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let node = FakeNode::start(core.restore_height() + BLOCKS_AHEAD);
    stand_in.route(SYNC_HOST, SYNC_PORT, node.port());

    let address = core.receive_address().unwrap();
    let phrase = core.seed_phrase().unwrap();
    let start = core.restore_height();

    // The node is there and reports its chain.
    assert_eq!(core.node_height().unwrap(), start + BLOCKS_AHEAD);

    // The connection goes away while blocks are arriving; the node itself
    // stays reachable.
    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_BLOCKS, AfterCut::NodeReachable);
    let outcome = core.sync();
    assert!(
        matches!(outcome, Err(CoreError::Wallet(Error::NodeUnreachable))),
        "{outcome:?}"
    );
    assert_eq!(stand_in.cuts(), 1, "asked: {:?}", node.asked());

    // No block is counted as scanned (the wallet's own height moves no
    // further than the block it starts scanning from), and the wallet
    // still works.
    let scanned = core.sync_progress().wallet_height;
    assert!(scanned <= start, "{scanned} is past the start, {start}");
    assert!(core.balance().total.is_zero());
    assert_eq!(core.receive_address().unwrap(), address);
    assert_eq!(core.node_height().unwrap(), start + BLOCKS_AHEAD);

    // It closes cleanly and opens as the same wallet.
    core.close().unwrap();
    let wallet = Wallet::open(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();
    let mut core = Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap();
    assert_eq!(core.seed_phrase().unwrap().as_str(), phrase.as_str());
    assert_eq!(core.receive_address().unwrap(), address);
    assert_eq!(core.restore_height(), start);
    assert_eq!(core.sync_progress().wallet_height, scanned);
}

#[test]
fn a_transaction_would_go_to_the_first_node_that_answers() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("broadcast-route");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let sync = OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap();
    let broadcast = OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap();
    let third = OnionNode::new(THIRD_HOST, 18089).unwrap();
    let candidates = [sync.clone(), broadcast.clone(), third.clone()];

    // The order to try them in: the broadcast node in place, the other
    // candidates, and the node synchronised with last. A node the last
    // send went through comes before all of them.
    let names = |core: &Core, preferred: Option<&OnionNode>| -> Vec<OnionNode> {
        core.broadcast_route(&candidates, preferred)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    };
    assert_eq!(
        names(&core, None),
        [broadcast.clone(), third.clone(), sync.clone()]
    );
    assert_eq!(
        names(&core, Some(&third)),
        [third.clone(), broadcast.clone(), sync.clone()]
    );

    // Every node on the way is asked through the broadcast listener, in
    // that order, until one answers.
    let asked = |stand_in: &StandIn| -> Vec<String> {
        let requests = stand_in.requests();
        assert!(
            requests.iter().all(|r| r.purpose == Purpose::Broadcast),
            "{requests:?}"
        );
        let mut hosts: Vec<String> = requests.into_iter().map(|r| r.host).collect();
        hosts.dedup();
        hosts
    };
    let node = FakeNode::start(core.restore_height() + BLOCKS_AHEAD);

    // Nobody is there: no node, and nothing to hand a transaction to.
    stand_in.clear_requests();
    assert_eq!(core.broadcast_node_that_answers(&candidates).unwrap(), None);
    assert_eq!(asked(&stand_in), [BROADCAST_HOST, THIRD_HOST, SYNC_HOST]);

    // The broadcast node is down and the next candidate is not: that one.
    stand_in.route(THIRD_HOST, 18089, node.port());
    stand_in.clear_requests();
    assert_eq!(
        core.broadcast_node_that_answers(&candidates).unwrap(),
        Some(third.clone())
    );
    assert_eq!(asked(&stand_in), [BROADCAST_HOST, THIRD_HOST]);

    // Only the node synchronised with is left: then that one, through the
    // broadcast listener all the same, rather than no node at all.
    stand_in.unroute(THIRD_HOST, 18089);
    stand_in.route(SYNC_HOST, SYNC_PORT, node.port());
    stand_in.clear_requests();
    assert_eq!(
        core.broadcast_node_that_answers(&candidates).unwrap(),
        Some(sync.clone())
    );
    assert_eq!(asked(&stand_in), [BROADCAST_HOST, THIRD_HOST, SYNC_HOST]);

    // The broadcast node is there: nobody else is asked.
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, node.port());
    stand_in.clear_requests();
    assert_eq!(
        core.broadcast_node_that_answers(&candidates).unwrap(),
        Some(broadcast.clone())
    );
    assert_eq!(asked(&stand_in), [BROADCAST_HOST]);

    // Asking leaves the wallet where it was: it synchronises with its
    // sync node, through the sync listener.
    assert_eq!(core.nodes(), &nodes());
    stand_in.clear_requests();
    let height = core.node_height();
    assert!(height.is_ok(), "{height:?}");
    let requests = stand_in.requests();
    assert!(
        requests
            .iter()
            .all(|r| r.purpose == Purpose::Sync && r.host == SYNC_HOST),
        "{requests:?}"
    );

    // And without Tor nobody is asked anything.
    stand_in.set_ready(false);
    stand_in.clear_requests();
    assert!(matches!(
        core.broadcast_node_that_answers(&candidates),
        Err(CoreError::TorNotReady { .. })
    ));
    assert!(stand_in.requests().is_empty());
}

#[test]
fn a_network_lost_mid_sync_is_tors_fault_from_the_next_connection_on() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("tor-drops");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let node = FakeNode::start(core.restore_height() + BLOCKS_AHEAD);
    stand_in.route(SYNC_HOST, SYNC_PORT, node.port());

    // The connection breaks while blocks arrive, and the network is gone
    // from under Tor. A connection that breaks does not say why, and Tor
    // learns of the loss only from a connection it is asked for. The
    // wallet library does not ask for one before it gives up, so this
    // call still ends as the node's failure.
    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_BLOCKS, AfterCut::NetworkLost);
    let outcome = core.sync();
    assert!(
        matches!(
            outcome,
            Err(CoreError::Wallet(Error::NodeUnreachable) | CoreError::TorLost { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(stand_in.cuts(), 1, "asked: {:?}", node.asked());

    // The next call asks for a connection. From then on Tor says that it
    // has lost its network, and every call ends with that, not with a
    // complaint about the node. The node goes on being asked for, through
    // Tor: nothing else would show that the network is back.
    stand_in.clear_requests();
    let outcome = core.sync();
    assert!(
        matches!(outcome, Err(CoreError::TorLost { .. })),
        "{outcome:?}"
    );
    assert_eq!(core.tor_status().state, State::Lost);
    assert!(matches!(core.node_height(), Err(CoreError::TorLost { .. })));
    let requests = stand_in.requests();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|r| r.purpose == Purpose::Sync && r.host == SYNC_HOST),
        "{requests:?}"
    );

    // A transaction that no node took was not sent because of Tor. One
    // whose outcome is unknown stays that: it may have gone out before
    // the network did.
    assert!(matches!(
        core.commit_failure(Error::NoNodeToSendThrough { tried: 2 }),
        CoreError::TorLost { .. }
    ));
    assert!(matches!(
        core.commit_failure(Error::SendOutcomeUnknown),
        CoreError::Wallet(Error::SendOutcomeUnknown)
    ));

    // The network comes back. Nobody is told; the next call gets through,
    // and with it Tor is ready again.
    stand_in.set_network(true);
    assert_eq!(core.tor_status().state, State::Lost);
    assert!(core.node_height().is_ok());
    assert_eq!(core.tor_status().state, State::Ready);
    assert!(matches!(
        core.commit_failure(Error::NoNodeToSendThrough { tried: 2 }),
        CoreError::Wallet(Error::NoNodeToSendThrough { tried: 2 })
    ));
}

#[test]
fn a_node_that_drops_mid_sync_is_left_for_the_next_one() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("next-node");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let height = core.restore_height() + BLOCKS_AHEAD;
    let first = FakeNode::start(height);
    let second = FakeNode::start(height);
    stand_in.route(SYNC_HOST, SYNC_PORT, first.port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, second.port());
    let candidates = [
        OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
        OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
    ];

    // The first node disappears while it is sending blocks. The second is
    // asked for them next, through the sync listener like the first. It is
    // a fake as well and has no blocks to give, so in the end no node has
    // answered; a synchronisation that finishes on the second node is
    // tested on a private chain.
    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_BLOCKS, AfterCut::NodeGone);
    let outcome = core.sync_with_any(&candidates, &mut |_| {});
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
    assert_eq!(stand_in.cuts(), 1);
    assert!(asked_for_blocks(&first), "{:?}", first.asked());
    assert!(asked_for_blocks(&second), "{:?}", second.asked());
    assert_eq!(core.nodes().sync(), &candidates[1]);
    assert_eq!(core.nodes().broadcast(), &candidates[0]);
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync));
}

#[test]
fn every_node_is_asked_when_the_network_is_lost_and_tor_is_what_is_named() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("tor-drops-any");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let height = core.restore_height() + BLOCKS_AHEAD;
    let first = FakeNode::start(height);
    let second = FakeNode::start(height);
    stand_in.route(SYNC_HOST, SYNC_PORT, first.port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, second.port());
    let candidates = [
        OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
        OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
    ];

    // The network goes while the first node sends blocks. The second is
    // asked for all the same, through Tor, and nothing reaches it. What
    // comes out is that Tor has lost its network, not that two nodes did
    // not answer.
    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_BLOCKS, AfterCut::NetworkLost);
    let outcome = core.sync_with_any(&candidates, &mut |_| {});
    assert!(
        matches!(outcome, Err(CoreError::TorLost { .. })),
        "{outcome:?}"
    );
    assert!(second.asked().is_empty(), "{:?}", second.asked());
    assert!(stand_in
        .requests()
        .iter()
        .any(|request| request.host == BROADCAST_HOST));
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.purpose == Purpose::Sync));

    // The network is back, and the first node is not: asking for it says
    // nothing about the network, so Tor still counts as lost when the
    // second node is asked. That one answers, and that ends the loss: a
    // node that is down cannot keep the wallet from finding out. (The
    // second node is a fake without blocks, so in the end no node has
    // carried the synchronisation, and that is what is said now.)
    stand_in.set_network(true);
    stand_in.unroute(SYNC_HOST, SYNC_PORT);
    let outcome = core.sync_with_any(&candidates, &mut |_| {});
    assert!(
        matches!(outcome, Err(CoreError::NoNodeAnswered { tried: 2, .. })),
        "{outcome:?}"
    );
    assert!(asked_for_blocks(&second), "{:?}", second.asked());
    assert_eq!(core.tor_status().state, State::Ready);
}

/// How long the slow fake node takes over a request for blocks: long
/// enough for something to happen while the wallet waits for them.
const SLOW: Duration = Duration::from_secs(2);

#[test]
fn a_synchronisation_told_to_stop_tries_no_further_node_and_can_be_continued() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("stop");
    let stand_in = StandIn::start().unwrap();
    let mut core = core_on(&lib, &dir, &stand_in);
    let height = core.restore_height() + BLOCKS_AHEAD;
    let first = FakeNode::start_slow(height, SLOW);
    let second = FakeNode::start(height);
    stand_in.route(SYNC_HOST, SYNC_PORT, first.port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, second.port());
    let candidates = [
        OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
        OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
    ];

    // Told to stop before it starts, a synchronisation asks nothing of
    // anyone.
    let outcome = core.sync_with_any_until(&candidates, &|| true, &mut |_| {});
    assert!(
        matches!(outcome, Err(CoreError::Wallet(Error::SyncStopped))),
        "{outcome:?}"
    );
    assert!(stand_in.requests().is_empty(), "{:?}", stand_in.requests());

    // Now the stop comes while the first node is taking its time over the
    // blocks. The wallet cannot leave that request. The node, a fake with
    // no blocks to give, then fails as it would have anyway, and the stop
    // keeps the second node from being asked at all.
    let mut attempt = SyncAttempt::through(&candidates);
    let outcome = core.continue_sync(&mut attempt, &|| asked_for_blocks(&first), &mut |_| {});
    assert!(
        matches!(outcome, Err(CoreError::Wallet(Error::SyncStopped))),
        "{outcome:?}"
    );
    assert!(second.asked().is_empty(), "{:?}", second.asked());
    let asked_of_first = first.asked().len();

    // Continued, the attempt goes on with the second node and does not go
    // back to the one that failed. So it ends, here with no node left; an
    // attempt that is stopped and then finishes is tested on a private
    // chain.
    let outcome = core.continue_sync(&mut attempt, &|| false, &mut |_| {});
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
    assert!(asked_for_blocks(&second), "{:?}", second.asked());
    assert_eq!(first.asked().len(), asked_of_first);
}

mod fake_node;
mod ffi;
#[cfg(feature = "regtest")]
mod regtest;
mod session;
