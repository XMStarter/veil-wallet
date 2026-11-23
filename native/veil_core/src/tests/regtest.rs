//! The core on a private chain, behind the stand-in for Tor.
//!
//! Needs the wallet library (`VEIL_WALLET_LIB`) and a Monero daemon
//! (`VEIL_MONEROD`), so it runs on the owner's machine and never in hosted
//! CI. The wallet is set up exactly as in the app: its nodes are onion
//! names behind the loopback SOCKS listeners. Only what is behind the
//! listeners differs: the stand-in leads both names to the regtest daemon.

use std::time::Duration;

use veil_test_guard::RegtestDaemon;
use veil_tor::{Purpose, Request, StandIn};
use veil_wallet::{Amount, Direction, Network, Node, SendAmount, Wallet};

use super::{
    nodes, wallet_library, TempDir, BROADCAST_HOST, BROADCAST_PORT, PASSWORD, SYNC_HOST, SYNC_PORT,
};
use crate::Core;

/// The first outputs on a new chain unlock after 60 blocks; the extra blocks
/// give the wallet enough outputs on the chain to build rings from.
const INITIAL_BLOCKS: u64 = 130;
/// Received funds unlock after 10 confirmations.
const UNLOCK_BLOCKS: u64 = 10;
/// The wallet library's own refresh thread wakes every ten seconds.
const LONGER_THAN_THE_REFRESH_TIMER: Duration = Duration::from_secs(13);

fn sync(core: &mut Core) {
    core.forget_node_state().unwrap();
    let progress = core.sync().unwrap();
    assert_eq!(progress.blocks_left(), 0);
}

fn through(requests: &[Request], purpose: Purpose) -> Vec<&Request> {
    requests.iter().filter(|r| r.purpose == purpose).collect()
}

#[test]
fn synchronises_through_one_listener_and_sends_through_the_other() {
    // Checked again in `RegtestDaemon::start`; here so that a CI run fails
    // even when the library is missing and the test would otherwise be
    // skipped.
    veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
    let Some(lib) = wallet_library() else { return };
    let Some(daemon) = RegtestDaemon::start() else {
        return;
    };
    let dir = TempDir::new("regtest");

    let stand_in = StandIn::start().unwrap();
    stand_in.route(SYNC_HOST, SYNC_PORT, daemon.rpc_port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, daemon.rpc_port());

    // The user's wallet, run by the core. It scans from the first block of
    // the private chain and accepts that chain's rules.
    let (mut wallet, _) =
        Wallet::create(&lib, &dir.file("user"), PASSWORD, Network::Mainnet).unwrap();
    wallet.set_restore_height(0);
    wallet.accept_regtest_chain();
    // A regtest daemon can only credit a primary address.
    let mining_address = wallet.primary_address().unwrap();
    let mut core = Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap();

    daemon.add_blocks(INITIAL_BLOCKS, &mining_address);
    sync(&mut core);
    let funded = core.balance();
    assert!(!funded.unlocked.is_zero(), "mined funds are spendable");

    // Synchronising used the sync listener and the sync node's name only.
    let requests = stand_in.requests();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|r| r.purpose == Purpose::Sync && r.host == SYNC_HOST && r.port == SYNC_PORT),
        "{requests:?}"
    );

    // Someone to pay: a wallet of its own, reaching the daemon directly.
    let payee_node = Node::LocalRegtest {
        port: daemon.rpc_port(),
    };
    let (mut payee, _) =
        Wallet::create(&lib, &dir.file("payee"), PASSWORD, Network::Mainnet).unwrap();
    payee.set_restore_height(0);
    payee.connect(&payee_node).unwrap();
    let payee_address = payee.receive_address().unwrap().address;

    // Build the transaction: still the sync node, still the sync listener.
    stand_in.clear_requests();
    let amount: Amount = "1.5".parse().unwrap();
    let pending = core
        .prepare_send(&payee_address, SendAmount::Exact(amount))
        .expect("transaction builds");
    let fee = pending.fee();
    let requests = stand_in.requests();
    assert!(
        through(&requests, Purpose::Broadcast).is_empty(),
        "building a transaction must not touch the broadcast listener: {requests:?}"
    );

    // Send it: the broadcast node, through the broadcast listener.
    stand_in.clear_requests();
    let txids = pending.commit().expect("transaction is accepted");
    assert!(!txids.is_empty());
    let requests = stand_in.requests();
    let broadcast = through(&requests, Purpose::Broadcast);
    assert!(
        !broadcast.is_empty(),
        "the transaction did not go through the broadcast listener: {requests:?}"
    );
    assert!(
        broadcast
            .iter()
            .all(|r| r.host == BROADCAST_HOST && r.port == BROADCAST_PORT),
        "{requests:?}"
    );
    assert!(
        through(&requests, Purpose::Sync).is_empty(),
        "sending must not contact the sync node: {requests:?}"
    );

    // The wallet library switches its refresh thread on when a transaction
    // is built or sent. Left alone, that thread would now synchronise on
    // its timer, and during a send it would do so through the broadcast
    // node. It must stay silent.
    stand_in.clear_requests();
    std::thread::sleep(LONGER_THAN_THE_REFRESH_TIMER);
    let requests = stand_in.requests();
    assert!(
        requests.is_empty(),
        "the wallet contacted a node on its own: {requests:?}"
    );

    // Confirm the transaction. Synchronising is back on the sync node.
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &mining_address);
    sync(&mut core);
    let requests = stand_in.requests();
    assert!(
        requests
            .iter()
            .all(|r| r.purpose == Purpose::Sync && r.host == SYNC_HOST),
        "{requests:?}"
    );

    let sent = core
        .history()
        .unwrap()
        .into_iter()
        .find(|t| txids.contains(&t.txid))
        .expect("the sent transaction is in the history");
    assert_eq!(sent.direction, Direction::Outgoing);
    assert_eq!(sent.amount, amount);
    assert_eq!(sent.fee, fee);
    assert!(!sent.pending && !sent.failed);

    payee.connect(&payee_node).unwrap();
    payee.refresh().unwrap();
    assert_eq!(payee.balance().total, amount);

    // With Tor gone, a transaction can be neither built nor sent.
    stand_in.set_ready(false);
    assert!(core
        .prepare_send(&payee_address, SendAmount::Exact(amount))
        .is_err());
}
