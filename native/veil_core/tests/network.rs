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

/// Public mainnet nodes, tried in order until two answer. They come from
/// the public list at monero.fail and are here only so that the test has
/// something to reach. Nobody vetted them and the app does not use this
/// list. Override with `VEIL_TEST_NODES` (`host.onion:port`, separated by
/// commas).
const PUBLIC_NODES: &[&str] = &[
    "plowsof3t5hogddwabaeiyrno25efmzfxyro2vligremt7sxpsclfaid.onion:18089",
    "rucknium757bokwv3ss35ftgc3gzb7hgbvvglbg3hisp7tsj2fkd2nyd.onion:18081",
    "hashvaultsvg2rinvxz7kos77hdfm6zrd5yco3tx2yh2linsmusfwyad.onion:18081",
    "supportxmrcnzyyl4gtmtvql446f4pdqfcrykwaltwsg3trj3ekn3fad.onion:18089",
    "moneronkvv2hu2anvcc5b4qd5y7strnc2ob6khqsrtikmhocyvjpdjyd.onion:18089",
    "xmrnodei64luhuwm4qfi7jga2k3tdpy7l2ovbledihfjgt2xvvw3paad.onion:18089",
];

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(300);
/// How many of the newest blocks the test wallet scans.
const BLOCKS_TO_SCAN: u64 = 10;

fn candidates() -> Vec<OnionNode> {
    let from_env = std::env::var("VEIL_TEST_NODES").ok();
    let list: Vec<&str> = match &from_env {
        Some(value) => value.split(',').collect(),
        None => PUBLIC_NODES.to_vec(),
    };
    list.into_iter()
        .map(|entry| entry.parse().expect("host.onion:port"))
        .collect()
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
    let Some(library) = std::env::var_os("VEIL_WALLET_LIB") else {
        assert!(
            std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
            "VEIL_WALLET_LIB is not set but the wallet library is required"
        );
        eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
        return;
    };
    let lib = WalletLibrary::load(library).expect("wallet library loads");

    let root: PathBuf = std::env::temp_dir().join(format!("veil-core-net-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let tor = TorService::start(TorConfig {
        state_dir: root.join("tor-state"),
        cache_dir: root.join("tor-cache"),
        bridges: Bridges::none(),
    })
    .expect("Tor starts");
    let (wallet, _) = Wallet::create(
        &lib,
        &root.join("wallet"),
        "not a real wallet",
        Network::Mainnet,
    )
    .unwrap();

    let nodes = candidates();
    assert!(nodes.len() >= 2, "two nodes are needed");
    let mut core = Core::new(
        tor,
        wallet,
        Nodes::separate(nodes[0].clone(), nodes[1].clone()).unwrap(),
    )
    .unwrap();

    wait_until_ready(&core);

    // Find a node that answers. The next candidate serves as the broadcast
    // node; the test sends nothing, so that one is never contacted.
    let mut height = None;
    for (position, node) in nodes.iter().enumerate() {
        let other = nodes[(position + 1) % nodes.len()].clone();
        core.set_nodes(Nodes::separate(node.clone(), other).unwrap())
            .unwrap();
        let asked = Instant::now();
        match core.node_height() {
            Ok(found) => {
                println!("node {node} is at height {found} ({:.0?})", asked.elapsed());
                height = Some(found);
                break;
            }
            Err(error) => println!("node {node}: {error} ({:.0?})", asked.elapsed()),
        }
    }
    let height = height.expect("no public node answered over Tor");
    assert!(height > 3_000_000, "a mainnet node is far along: {height}");

    // A new wallet starts a month back, which takes hours over Tor. The
    // last few blocks are enough to show that blocks arrive and are scanned.
    core.set_restore_height(height.saturating_sub(BLOCKS_TO_SCAN));
    let started = Instant::now();
    let progress = core.sync().expect("the wallet synchronises over Tor");
    println!(
        "synchronised to height {} in {:.0?}",
        progress.wallet_height,
        started.elapsed()
    );
    assert_eq!(progress.blocks_left(), 0);
    assert!(progress.wallet_height >= height);
    assert_eq!(core.balance().total, Amount::ZERO);
    assert!(core.history().unwrap().is_empty());

    core.close().unwrap();
    let _ = std::fs::remove_dir_all(root);
}
