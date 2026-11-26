//! The core on the real Tor network, against a public Monero node.
//!
//! Ignored by default and refused on a CI runner: it starts a Tor client,
//! reaches a node that is an onion service, and synchronises a new, empty
//! mainnet wallet with it. No funds are involved. Needs the wallet library
//! (`VEIL_WALLET_LIB`). Run it with `../check.sh tor`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use veil_core::{Core, Nodes, OnionNode};
use veil_tor::{Bridges, State, TorConfig, TorService};
use veil_wallet::{Amount, Network, Wallet, WalletLibrary};

/// A block comes about every two minutes.
const BLOCKS_A_DAY: u64 = 720;

/// How far behind the others a node may be, in blocks, before it is said
/// to be on a chain of its own or not keeping up. A block comes every two
/// minutes, and asking the nodes one after the other takes a while.
const BLOCKS_APART: u64 = 10;

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(300);
/// How many of the newest blocks the test wallet scans.
const BLOCKS_TO_SCAN: u64 = 10;

/// The nodes the app goes to for a wallet on the main network
/// (`veil_core::public_nodes`), in the order they are written, so that
/// this is a test of the list an app is built with. `VEIL_TEST_NODES`
/// (`host.onion:port`, separated by commas) names others, for trying a
/// node before it is put on the list.
fn candidates() -> Vec<OnionNode> {
    match std::env::var("VEIL_TEST_NODES") {
        Ok(value) => value
            .split(',')
            .map(|entry| entry.trim().parse().expect("host.onion:port"))
            .collect(),
        Err(_) => veil_core::public_nodes(Network::Mainnet),
    }
}

/// A new, empty wallet on the main network, joined to a Tor that has
/// connected.
fn core_on_the_main_network(lib: &WalletLibrary, root: &std::path::Path) -> Core {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).unwrap();
    let tor = TorService::start(TorConfig {
        state_dir: root.join("tor-state"),
        cache_dir: root.join("tor-cache"),
        bridges: Bridges::none(),
    })
    .expect("Tor starts");
    let (wallet, _) = Wallet::create(
        lib,
        &root.join("wallet"),
        "not a real wallet",
        Network::Mainnet,
    )
    .unwrap();
    let nodes = candidates();
    assert!(nodes.len() >= 2, "two nodes are needed");
    let core = Core::new(
        tor,
        wallet,
        Nodes::separate(nodes[0].clone(), nodes[1].clone()).unwrap(),
    )
    .unwrap();
    wait_until_ready(&core);
    core
}

/// The wallet library, or none where the test is to be skipped.
fn library() -> Option<WalletLibrary> {
    let Some(library) = std::env::var_os("VEIL_WALLET_LIB") else {
        assert!(
            std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
            "VEIL_WALLET_LIB is not set but the wallet library is required"
        );
        eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
        return None;
    };
    Some(WalletLibrary::load(library).expect("wallet library loads"))
}

fn wait_until_ready(core: &Core) {
    let started = Instant::now();
    let mut last_percent = u8::MAX;
    loop {
        let status = core.tor_status();
        if status.percent != last_percent {
            println!("bootstrap {}% {:?}", status.percent, status.message);
            last_percent = status.percent;
        }
        match status.state {
            State::Ready => return,
            State::Failed => panic!("Tor failed to start: {:?}", status.message),
            State::Lost => panic!("Tor lost its network: {:?}", status.message),
            State::Bootstrapping => {}
        }
        assert!(
            started.elapsed() < BOOTSTRAP_TIMEOUT,
            "Tor did not bootstrap in {BOOTSTRAP_TIMEOUT:?}: {:?}",
            status.message
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
#[ignore = "needs the Tor network and a public Monero node"]
fn a_new_wallet_synchronises_with_a_public_node_over_tor() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let Some(lib) = library() else { return };
    let root: PathBuf = std::env::temp_dir().join(format!("veil-core-net-{}", std::process::id()));
    let mut core = core_on_the_main_network(&lib, &root);
    let nodes = candidates();

    // Go through the nodes until one carries a whole synchronisation. The
    // next candidate serves as the broadcast node; the test sends nothing,
    // so that one is never contacted.
    //
    // A public node can stop answering between one request and the next.
    // That is the node's failure and not the wallet's, as long as the wallet
    // reports it and gets on with another node, which is what this loop
    // lets it show.
    let mut outcome = None;
    let mut made_now = None;
    for (position, node) in nodes.iter().enumerate() {
        let other = nodes[(position + 1) % nodes.len()].clone();
        core.set_nodes(Nodes::separate(node.clone(), other).unwrap())
            .unwrap();
        let asked = Instant::now();
        let height = match core.node_height() {
            Ok(height) => height,
            Err(error) => {
                println!("node {node}: {error} ({:.0?})", asked.elapsed());
                continue;
            }
        };
        println!(
            "node {node} is at height {height} ({:.0?})",
            asked.elapsed()
        );
        assert!(height > 3_000_000, "a mainnet node is far along: {height}");

        // Where a wallet made today begins to scan, which the library
        // reckons from the date and not from any node. It has to lie
        // behind the end of the chain: a wallet that began after it would
        // never look at a payment made today. And not much more than the
        // month it is meant to lie behind, or a first synchronisation
        // would take days.
        if made_now.is_none() {
            let begins = core.restore_height();
            println!(
                "a wallet made now scans from block {begins}, {} blocks before the node's end",
                height.saturating_sub(begins)
            );
            assert!(
                begins < height,
                "a new wallet would begin at {begins}, after the chain's end at {height}"
            );
            assert!(
                height - begins < 100 * BLOCKS_A_DAY,
                "a new wallet would begin {} blocks back",
                height - begins
            );
            made_now = Some(begins);
        }

        // A new wallet starts a month back, which takes hours over Tor. The
        // last few blocks are enough to show that blocks arrive and are
        // scanned.
        core.set_restore_height(height.saturating_sub(BLOCKS_TO_SCAN));
        let started = Instant::now();
        match core.sync() {
            Ok(progress) => {
                println!(
                    "synchronised to height {} in {:.0?}",
                    progress.wallet_height,
                    started.elapsed()
                );
                outcome = Some((height, progress));
                break;
            }
            Err(error) => println!(
                "node {node}: synchronising failed: {error} ({:.0?})",
                started.elapsed()
            ),
        }
    }
    let (height, progress) = outcome.expect("no public node carried a synchronisation over Tor");
    assert_eq!(progress.blocks_left(), 0);
    assert!(progress.wallet_height >= height);
    assert_eq!(core.balance().total, Amount::ZERO);
    assert!(core.history().unwrap().is_empty());

    core.close().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// Every node an app is built with for the main network, asked through
/// Tor where it is. What an app with real money in it depends on is that
/// enough of them are there, that they are the main network, and that
/// none is on a chain of its own: a node that is far behind the others
/// shows a wallet a balance that is no longer true.
///
/// It says what each node answered. It fails when fewer than two answer,
/// since a transaction goes to another node than the one a wallet
/// synchronises with, and when a node that answers is more than a few
/// blocks behind the one that is furthest. A node that does not answer is
/// said and is no failure: public nodes come and go, and the wallet passes
/// over one that is not there.
#[test]
#[ignore = "needs the Tor network and the public Monero nodes"]
fn the_public_nodes_of_the_main_network_answer_and_agree() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let Some(lib) = library() else { return };
    let root: PathBuf =
        std::env::temp_dir().join(format!("veil-core-nodes-{}", std::process::id()));
    let mut core = core_on_the_main_network(&lib, &root);
    let nodes = candidates();

    let mut heights = Vec::new();
    for (position, node) in nodes.iter().enumerate() {
        let other = nodes[(position + 1) % nodes.len()].clone();
        core.set_nodes(Nodes::separate(node.clone(), other).unwrap())
            .unwrap();
        let asked = Instant::now();
        match core.node_height() {
            Ok(height) => {
                println!(
                    "node {node} is at height {height} ({:.0?})",
                    asked.elapsed()
                );
                heights.push((node.clone(), height));
            }
            Err(error) => println!("node {node}: {error} ({:.0?})", asked.elapsed()),
        }
    }
    println!("{} of {} nodes answered", heights.len(), nodes.len());
    core.close().unwrap();
    let _ = std::fs::remove_dir_all(root);

    assert!(
        heights.len() >= 2,
        "fewer than two public nodes answered: a wallet could not send"
    );
    let furthest = heights.iter().map(|(_, height)| *height).max().unwrap();
    assert!(
        furthest > 3_000_000,
        "the main network is far along: {furthest}"
    );
    for (node, height) in &heights {
        assert!(
            furthest - height <= BLOCKS_APART,
            "node {node} is at {height}, {} blocks behind the furthest",
            furthest - height
        );
    }
}
