//! Receiving and sending on a private chain.
//!
//! Needs the wallet library (`VEIL_WALLET_LIB`) and a Monero daemon
//! (`VEIL_MONEROD`). The daemon runs in regtest mode: offline, on a chain of
//! its own, with blocks produced on request. No real funds are involved.

#![cfg(feature = "regtest")]

mod common;

use std::path::Path;

use common::{wallet_library, TempDir};
use veil_test_guard::RegtestDaemon;
use veil_wallet::{
    Amount, Direction, Error, Network, Node, SendAmount, Wallet, WalletLibrary,
    MAX_UNUSED_ADDRESSES,
};

const PASSWORD: &str = "regtest";
/// The first outputs on a new chain unlock after 60 blocks; the extra blocks
/// give the wallet enough outputs on the chain to build rings from.
const INITIAL_BLOCKS: u64 = 130;
/// Received funds unlock after 10 confirmations.
const UNLOCK_BLOCKS: u64 = 10;

fn node(daemon: &RegtestDaemon) -> Node {
    Node::LocalRegtest {
        port: daemon.rpc_port(),
    }
}

fn xmr(text: &str) -> Amount {
    text.parse().expect("valid amount")
}

/// A new wallet connected to the regtest daemon and scanning from genesis.
fn new_wallet(lib: &WalletLibrary, path: &Path, daemon: &RegtestDaemon) -> Wallet {
    let (mut wallet, _) = Wallet::create(lib, path, PASSWORD, Network::Mainnet).unwrap();
    // A new wallet starts scanning near today's public-chain height, which
    // is far above this private chain.
    wallet.set_restore_height(0);
    wallet.connect(&node(daemon)).unwrap();
    wallet
}

/// Brings a wallet up to date with the daemon.
///
/// The wallet library remembers the node's height for 30 seconds. On a real
/// network that is harmless; here blocks appear on request, so the node is
/// set again first, which drops the remembered height.
fn sync(wallet: &mut Wallet, daemon: &RegtestDaemon) {
    wallet.connect(&node(daemon)).unwrap();
    let progress = wallet.refresh().unwrap();
    assert_eq!(progress.blocks_left(), 0);
}

fn send(from: &mut Wallet, to: &str, amount: SendAmount) -> (Amount, Amount, Vec<String>) {
    let pending = from.prepare_send(to, amount).expect("transaction builds");
    let sent = pending.amount();
    let fee = pending.fee();
    let ids = pending.commit().expect("transaction is accepted");
    (sent, fee, ids)
}

#[test]
fn receive_on_fresh_addresses_send_and_restore() {
    // Checked again in `RegtestDaemon::start`; here so that a CI run fails
    // even when the library is missing and the test would otherwise be
    // skipped.
    veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
    let Some(lib) = wallet_library() else { return };
    let Some(daemon) = RegtestDaemon::start() else {
        return;
    };
    let dir = TempDir::new("regtest");

    // A funding wallet stands in for whoever pays the user.
    let mut funder = new_wallet(&lib, &dir.file("funder"), &daemon);
    let funder_address = funder.primary_address().unwrap();
    daemon.add_blocks(INITIAL_BLOCKS, &funder_address);
    sync(&mut funder, &daemon);
    assert!(funder.balance().unlocked > xmr("20"));

    let mut alice = new_wallet(&lib, &dir.file("alice"), &daemon);
    sync(&mut alice, &daemon);
    assert_eq!(alice.balance().total, Amount::ZERO);

    // --- First payment arrives on address 1 ---------------------------
    let first = alice.receive_address().unwrap();
    assert_eq!(first.index, 1);
    let (_, _, first_ids) = send(&mut funder, &first.address, SendAmount::Exact(xmr("5")));
    assert_eq!(first_ids.len(), 1);

    // Seen in the mempool: the address counts as used at once, before any
    // confirmation, so it is not handed to a second payer.
    sync(&mut alice, &daemon);
    let second = alice.receive_address().unwrap();
    assert_eq!(second.index, 2, "a paid address is replaced immediately");
    assert_ne!(second.address, first.address);

    daemon.add_blocks(1, &funder_address);
    sync(&mut alice, &daemon);
    assert_eq!(alice.balance().total, xmr("5"));
    assert_eq!(
        alice.balance().unlocked,
        Amount::ZERO,
        "new funds are locked"
    );

    daemon.add_blocks(UNLOCK_BLOCKS, &funder_address);
    sync(&mut alice, &daemon);
    assert_eq!(alice.balance().unlocked, xmr("5"));

    // --- Second payment arrives on address 2 --------------------------
    sync(&mut funder, &daemon);
    send(&mut funder, &second.address, SendAmount::Exact(xmr("3")));
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut alice, &daemon);
    assert_eq!(alice.balance().total, xmr("8"));
    assert_eq!(alice.balance().unlocked, xmr("8"));
    let third = alice.receive_address().unwrap();
    assert_eq!(third.index, 3);

    let incoming: Vec<_> = alice
        .history()
        .unwrap()
        .into_iter()
        .filter(|t| t.direction == Direction::Incoming)
        .collect();
    assert_eq!(incoming.len(), 2);
    assert_eq!(incoming[0].amount, xmr("5"));
    assert_eq!(incoming[0].address_indices, vec![1]);
    assert_eq!(incoming[0].txid, first_ids[0]);
    assert_eq!(incoming[1].amount, xmr("3"));
    assert_eq!(incoming[1].address_indices, vec![2]);
    assert!(incoming.iter().all(|t| !t.pending && !t.failed));
    assert!(
        incoming.iter().all(|t| t.fee.is_zero()),
        "the sender's fee is not shown on what was received"
    );

    // --- Alice pays Bob ------------------------------------------------
    let mut bob = new_wallet(&lib, &dir.file("bob"), &daemon);
    let bob_address = bob.receive_address().unwrap();

    // More than she has: refused when building, for that reason, and
    // nothing is sent.
    assert!(matches!(
        alice
            .prepare_send(&bob_address.address, SendAmount::Exact(xmr("1000")))
            .err(),
        Some(Error::NotEnoughFunds(_))
    ));

    let (sent, fee, payment_ids) = send(
        &mut alice,
        &bob_address.address,
        SendAmount::Exact(xmr("1")),
    );
    assert_eq!(sent, xmr("1"));
    assert!(!fee.is_zero(), "a transaction pays a fee");
    assert!(fee < xmr("0.01"), "the fee is small: {fee}");

    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut alice, &daemon);
    sync(&mut bob, &daemon);

    let expected = xmr("8")
        .checked_sub(xmr("1"))
        .and_then(|rest| rest.checked_sub(fee))
        .unwrap();
    assert_eq!(alice.balance().total, expected);
    assert_eq!(bob.balance().total, xmr("1"));
    assert_eq!(bob.balance().unlocked, xmr("1"));

    let outgoing: Vec<_> = alice
        .history()
        .unwrap()
        .into_iter()
        .filter(|t| t.direction == Direction::Outgoing)
        .collect();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].amount, xmr("1"));
    assert_eq!(outgoing[0].fee, fee);
    assert_eq!(outgoing[0].txid, payment_ids[0]);
    // Paying someone does not use up a receiving address.
    assert_eq!(alice.receive_address().unwrap().index, 3);

    // --- Bob sends everything back ------------------------------------
    let back_to = alice.receive_address().unwrap();
    let (swept, sweep_fee, _) = send(&mut bob, &back_to.address, SendAmount::Everything);
    assert_eq!(
        swept.checked_add(sweep_fee),
        Some(xmr("1")),
        "sending everything leaves nothing behind"
    );
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut bob, &daemon);
    sync(&mut alice, &daemon);
    assert_eq!(bob.balance().total, Amount::ZERO);
    let alice_total = expected.checked_add(swept).unwrap();
    assert_eq!(alice.balance().total, alice_total);
    assert_eq!(alice.receive_address().unwrap().index, 4);

    // --- The balance is on disk, not only in memory --------------------
    let alice_phrase = alice.seed_phrase().unwrap();
    alice.close().unwrap();
    let mut reopened = Wallet::open(&lib, &dir.file("alice"), PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.balance().total, alice_total);
    assert_eq!(reopened.receive_address().unwrap().index, 4);
    reopened.close().unwrap();

    // --- Restoring from the phrase finds every payment ----------------
    let mut restored = Wallet::restore(
        &lib,
        &dir.file("alice-restored"),
        PASSWORD,
        Network::Mainnet,
        &alice_phrase,
        0,
    )
    .unwrap();
    restored.set_restore_height(0);
    sync(&mut restored, &daemon);
    assert_eq!(restored.balance().total, alice_total);
    assert_eq!(
        restored.receive_address().unwrap().index,
        4,
        "a restored wallet continues after the last paid address"
    );
    assert_eq!(restored.receive_address().unwrap().address, {
        let mut again = Wallet::open(&lib, &dir.file("alice"), PASSWORD, Network::Mainnet).unwrap();
        again.receive_address().unwrap().address
    });
}

/// Issues addresses until no more are allowed and returns the last one.
fn last_address_allowed(wallet: &mut Wallet) -> veil_wallet::ReceiveAddress {
    let mut last = wallet.receive_address().unwrap();
    loop {
        match wallet.new_receive_address() {
            Ok(next) => last = next,
            Err(Error::TooManyUnusedAddresses { unused }) => {
                assert_eq!(unused, MAX_UNUSED_ADDRESSES);
                return last;
            }
            Err(other) => panic!("issuing an address failed: {other}"),
        }
    }
}

/// A restore finds payments however far apart their addresses are.
///
/// The user hands out as many addresses as the wallet allows without any
/// being paid, and only the last one is paid; then does the same again. A
/// wallet restored from the phrase knows none of these addresses in
/// advance. It has to find both payments, and continue after them.
#[test]
fn a_restore_finds_payments_past_long_runs_of_unused_addresses() {
    veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
    let Some(lib) = wallet_library() else { return };
    let Some(daemon) = RegtestDaemon::start() else {
        return;
    };
    let dir = TempDir::new("regtest-gap");

    let mut funder = new_wallet(&lib, &dir.file("funder"), &daemon);
    let funder_address = funder.primary_address().unwrap();
    daemon.add_blocks(INITIAL_BLOCKS, &funder_address);
    sync(&mut funder, &daemon);

    let mut user = new_wallet(&lib, &dir.file("user"), &daemon);
    sync(&mut user, &daemon);

    // The first run: addresses 1 to 20, the last one paid.
    let first = last_address_allowed(&mut user);
    assert_eq!(first.index, MAX_UNUSED_ADDRESSES);
    send(&mut funder, &first.address, SendAmount::Exact(xmr("2")));
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut user, &daemon);
    assert_eq!(user.balance().total, xmr("2"));

    // The second run starts after the paid address: 21 to 40.
    let second = last_address_allowed(&mut user);
    assert_eq!(second.index, 2 * MAX_UNUSED_ADDRESSES);
    sync(&mut funder, &daemon);
    send(&mut funder, &second.address, SendAmount::Exact(xmr("3")));
    daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
    sync(&mut user, &daemon);
    assert_eq!(user.balance().total, xmr("5"));
    let next = user.receive_address().unwrap();
    assert_eq!(next.index, 2 * MAX_UNUSED_ADDRESSES + 1);

    let phrase = user.seed_phrase().unwrap();
    user.close().unwrap();

    let mut restored = Wallet::restore(
        &lib,
        &dir.file("user-restored"),
        PASSWORD,
        Network::Mainnet,
        &phrase,
        0,
    )
    .unwrap();
    restored.set_restore_height(0);
    sync(&mut restored, &daemon);
    assert_eq!(restored.balance().total, xmr("5"));
    assert_eq!(restored.balance().unlocked, xmr("5"));
    let incoming: Vec<(u32, Amount)> = restored
        .history()
        .unwrap()
        .into_iter()
        .filter(|t| t.direction == Direction::Incoming)
        .flat_map(|t| t.address_indices.into_iter().map(move |i| (i, t.amount)))
        .collect();
    assert_eq!(
        incoming,
        vec![(first.index, xmr("2")), (second.index, xmr("3"))],
        "each payment is found at the address it was sent to"
    );
    // It goes on after the last paid address and never hands out an
    // address that was already given to someone.
    assert_eq!(restored.receive_address().unwrap(), next);

    // The original wallet's data file is damaged. The wallet opens from
    // its keys, empty, and a synchronisation finds everything again.
    let data_file = dir.file("user");
    let intact = std::fs::read(&data_file).unwrap();
    std::fs::write(&data_file, &intact[..intact.len() / 2]).unwrap();
    let mut rebuilt = Wallet::open(&lib, &data_file, PASSWORD, Network::Mainnet).unwrap();
    assert!(rebuilt.was_rebuilt_from_keys());
    assert_eq!(rebuilt.balance().total, Amount::ZERO);
    // As for every wallet on this private chain: scan from its first block.
    rebuilt.set_restore_height(0);
    sync(&mut rebuilt, &daemon);
    assert_eq!(rebuilt.balance().total, xmr("5"));
    assert_eq!(rebuilt.receive_address().unwrap(), next);
}
