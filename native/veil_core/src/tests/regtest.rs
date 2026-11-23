//! The core on a private chain, behind the stand-in for Tor.
//!
//! Needs the wallet library (`VEIL_WALLET_LIB`) and a Monero daemon
//! (`VEIL_MONEROD`), so it runs on the owner's machine and never in hosted
//! CI. The wallet is set up exactly as in the app: its nodes are onion
//! names behind the loopback SOCKS listeners. Only what is behind the
//! listeners differs: the stand-in leads both names to the regtest daemon.

use std::time::Duration;

use veil_test_guard::RegtestDaemon;
use veil_tor::{AfterCut, CutPoint, Purpose, Request, StandIn};
use veil_wallet::{Amount, Direction, Error, Network, Node, SendAmount, Wallet};

use super::{
    nodes, wallet_library, TempDir, BROADCAST_HOST, BROADCAST_PORT, PASSWORD, SYNC_HOST, SYNC_PORT,
};
use crate::{Core, CoreError, OnionNode};

/// The first outputs on a new chain unlock after 60 blocks; the extra blocks
/// give the wallet enough outputs on the chain to build rings from.
const INITIAL_BLOCKS: u64 = 130;
/// Received funds unlock after 10 confirmations.
const UNLOCK_BLOCKS: u64 = 10;
/// The wallet library's own refresh thread wakes every ten seconds.
const LONGER_THAN_THE_REFRESH_TIMER: Duration = Duration::from_secs(13);

/// Enough blocks that downloading them is one long answer from the node:
/// a block of this chain takes well over a hundred bytes.
const LONG_DOWNLOAD_BLOCKS: u64 = 600;
/// Where a download is cut. The answers that open an exchange with a node
/// (its side of the encryption handshake, its version, its height) come to
/// a few thousand bytes; this is past them and well inside the download.
const MID_DOWNLOAD: CutPoint = CutPoint::AfterReplyBytes(24 * 1024);
/// Where a send is cut. Opening an exchange takes the wallet well under two
/// thousand bytes; a transaction takes it over three thousand. The request
/// that crosses this mark is the transaction.
const MID_TRANSACTION: CutPoint = CutPoint::AfterRequestBytes(2500);

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
    // Settling on nodes finds the daemon behind the first name and keeps
    // the second for sending.
    let height = core
        .settle_on_nodes(&[
            OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
            OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
        ])
        .unwrap();
    assert!(
        height > INITIAL_BLOCKS,
        "the node reports its chain: {height}"
    );
    assert_eq!(core.nodes(), &nodes());
    // Progress reports, if any arrive in so short a scan, never run past
    // the node's height.
    let mut reports = Vec::new();
    let progress = core
        .sync_with_progress(&mut |progress| reports.push(progress))
        .unwrap();
    assert_eq!(progress.blocks_left(), 0);
    assert!(reports.iter().all(|r| r.wallet_height <= r.node_height));
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

/// Brings a wallet that reaches the daemon directly up to date.
fn sync_direct(wallet: &mut Wallet, node: &Node) {
    wallet.connect(node).unwrap();
    let progress = wallet.refresh().unwrap();
    assert_eq!(progress.blocks_left(), 0);
}

fn pay(from: &mut Wallet, to: &str, amount: Amount) {
    from.prepare_send(to, SendAmount::Exact(amount))
        .expect("transaction builds")
        .commit()
        .expect("transaction is accepted");
}

/// What losing a connection costs: nothing that was received, and no wrong
/// answer about what was sent.
#[test]
fn lost_connections_lose_no_funds_and_hide_no_send() {
    veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
    let Some(lib) = wallet_library() else { return };
    let Some(daemon) = RegtestDaemon::start() else {
        return;
    };
    let dir = TempDir::new("regtest-cuts");
    let direct = Node::LocalRegtest {
        port: daemon.rpc_port(),
    };
    let sync_node = OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap();
    let other_node = OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap();

    let stand_in = StandIn::start().unwrap();
    stand_in.route(SYNC_HOST, SYNC_PORT, daemon.rpc_port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, daemon.rpc_port());

    // Whoever pays the user, and the miner of every block: a wallet of its
    // own that reaches the daemon directly.
    let (mut funder, _) =
        Wallet::create(&lib, &dir.file("funder"), PASSWORD, Network::Mainnet).unwrap();
    funder.set_restore_height(0);
    funder.connect(&direct).unwrap();
    let funder_address = funder.primary_address().unwrap();
    daemon.add_blocks(LONG_DOWNLOAD_BLOCKS, &funder_address);
    sync_direct(&mut funder, &direct);

    // The user's wallet, run by the core behind the stand-in.
    let (mut wallet, _) =
        Wallet::create(&lib, &dir.file("user"), PASSWORD, Network::Mainnet).unwrap();
    wallet.set_restore_height(0);
    wallet.accept_regtest_chain();
    let mut core = Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap();

    let first: Amount = "5".parse().unwrap();
    let first_address = core.receive_address().unwrap().address;
    pay(&mut funder, &first_address, first);
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);

    // --- One connection is lost in the middle of the download ------------
    // The node is still there. The wallet library asks again by itself,
    // and the synchronisation finishes as if nothing had happened.
    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_DOWNLOAD, AfterCut::NodeReachable);
    let progress = core.sync().expect("a single lost connection is retried");
    assert_eq!(stand_in.cuts(), 1, "the download was too short to be cut");
    assert_eq!(progress.blocks_left(), 0);
    assert_eq!(core.balance().total, first);
    assert_eq!(core.balance().unlocked, first);

    // --- The node disappears in the middle of the download ----------------
    // A second payment arrives, far enough back that finding it is again a
    // long download.
    let second: Amount = "3".parse().unwrap();
    let second_address = core.receive_address().unwrap().address;
    assert_ne!(second_address, first_address);
    sync_direct(&mut funder, &direct);
    pay(&mut funder, &second_address, second);
    daemon.add_blocks(LONG_DOWNLOAD_BLOCKS, &funder_address);

    stand_in.cut_once(SYNC_HOST, SYNC_PORT, MID_DOWNLOAD, AfterCut::NodeGone);
    core.forget_node_state().unwrap();
    let outcome = core.sync();
    assert!(
        matches!(outcome, Err(CoreError::Wallet(Error::NodeUnreachable))),
        "{outcome:?}"
    );
    assert_eq!(stand_in.cuts(), 2, "the download was too short to be cut");
    // The synchronisation failed and says so. The balance is the one from
    // before, never a guess.
    assert_eq!(core.balance().total, first);

    // The wallet is closed in that state and opened again, as after the
    // user gave up for the day.
    core.close().unwrap();
    let mut wallet = Wallet::open(&lib, &dir.file("user"), PASSWORD, Network::Mainnet).unwrap();
    wallet.accept_regtest_chain();
    let mut core = Core::with_transport(Box::new(stand_in.clone()), wallet, nodes()).unwrap();
    assert_eq!(core.balance().total, first);

    // --- Another node finishes the job -------------------------------------
    // The first node is still gone. The second candidate is asked next and
    // the wallet ends up on it, with the second payment found.
    stand_in.clear_requests();
    let candidates = [sync_node.clone(), other_node.clone()];
    let progress = core
        .sync_with_any(&candidates, &mut |_| {})
        .expect("the second node carries the synchronisation");
    assert_eq!(progress.blocks_left(), 0);
    assert_eq!(core.nodes().sync(), &other_node);
    let both = first.checked_add(second).unwrap();
    assert_eq!(core.balance().total, both);
    assert_eq!(core.balance().unlocked, both);
    let requests = stand_in.requests();
    assert!(
        requests.iter().all(|r| r.purpose == Purpose::Sync),
        "{requests:?}"
    );
    let mut hosts: Vec<&str> = requests.iter().map(|r| r.host.as_str()).collect();
    hosts.dedup();
    assert_eq!(hosts, vec![SYNC_HOST, BROADCAST_HOST], "{requests:?}");

    // --- A send whose answer is lost ----------------------------------------
    // The first node is back, and the wallet returns to its usual pair.
    stand_in.route(SYNC_HOST, SYNC_PORT, daemon.rpc_port());
    core.set_nodes(nodes()).unwrap();
    sync(&mut core);

    let (mut payee, _) =
        Wallet::create(&lib, &dir.file("payee"), PASSWORD, Network::Mainnet).unwrap();
    payee.set_restore_height(0);
    payee.connect(&direct).unwrap();
    let payee_address = payee.receive_address().unwrap().address;

    let amount: Amount = "1.5".parse().unwrap();
    let pending = core
        .prepare_send(&payee_address, SendAmount::Exact(amount))
        .expect("transaction builds");
    let fee = pending.fee();
    let txids = pending.txids().to_vec();
    assert_eq!(txids.len(), 1);

    // The transaction reaches the broadcast node; the node's answer does
    // not come back.
    stand_in.cut_once(
        BROADCAST_HOST,
        BROADCAST_PORT,
        MID_TRANSACTION,
        AfterCut::NodeReachable,
    );
    let outcome = pending.commit();
    assert!(
        matches!(outcome, Err(Error::SendOutcomeUnknown)),
        "a send that may have gone out must not be reported as sent or as failed: {outcome:?}"
    );
    assert_eq!(stand_in.cuts(), 3, "the send was not cut");
    // The wallet has recorded nothing yet.
    assert_eq!(core.balance().total, both);

    // The next synchronisation after a block tells the truth: the
    // transaction went out, once.
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut core);
    let sent = core
        .history()
        .unwrap()
        .into_iter()
        .find(|t| txids.contains(&t.txid))
        .expect("the transaction whose answer was lost is in the history");
    assert_eq!(sent.direction, Direction::Outgoing);
    assert!(!sent.pending && !sent.failed);
    assert_eq!(sent.amount, amount);
    assert_eq!(sent.fee, fee);
    let left = both
        .checked_sub(amount)
        .and_then(|rest| rest.checked_sub(fee))
        .unwrap();
    assert_eq!(core.balance().total, left);
    assert_eq!(core.balance().unlocked, left);

    sync_direct(&mut payee, &direct);
    assert_eq!(payee.balance().total, amount, "the payee was paid once");
}
