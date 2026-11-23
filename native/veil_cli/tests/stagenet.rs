//! The tool on the real stagenet, through the real Tor network.
//!
//! Ignored by default and refused on a CI runner. Needs the wallet library
//! (`VEIL_WALLET_LIB`). Run it with `../check.sh stagenet`.
//!
//! A new wallet starts scanning about a month back, so this is also a
//! measure of what a first sync costs over Tor. Sending needs stagenet
//! coins and is tried by hand; see the README.

mod common;

use std::time::Instant;

use common::{address_in, have_wallet_library, veil, TempDir};

const PASSWORD: &str = "not a real wallet";

#[test]
#[ignore = "needs the Tor network and a public stagenet node"]
fn a_new_wallet_synchronises_on_stagenet_through_tor() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("stagenet");
    let wallet = dir.path("w");

    let created = veil(&wallet, &["create"], &[PASSWORD, PASSWORD]);
    assert!(created.ok(), "{}", created.stderr);
    let address = address_in(&created);

    let started = Instant::now();
    let synced = veil(&wallet, &["sync"], &[PASSWORD]);
    println!("--- veil sync, {:.0?}", started.elapsed());
    print!("{}", synced.stderr);
    print!("{}", synced.stdout);
    assert!(synced.ok(), "sync failed: {}", synced.stderr);
    assert!(synced.stderr.contains("Tor is connected"));
    assert!(synced.stderr.contains("Synchronised to block"));
    assert!(
        synced.stdout.contains("Balance:   0.00 XMR"),
        "{}",
        synced.stdout
    );
    assert!(
        wallet.join("tor").exists(),
        "Tor keeps its state in the data directory"
    );

    // What was synchronised was saved: the balance command agrees on the
    // height without going online, and the address is still the first one.
    let height_line = |text: &str| {
        text.lines()
            .find(|line| line.starts_with("At block:"))
            .map(str::to_owned)
    };
    let balance = veil(&wallet, &["balance"], &[PASSWORD]);
    assert!(balance.ok(), "{}", balance.stderr);
    assert_eq!(height_line(&balance.stdout), height_line(&synced.stdout));
    assert_eq!(
        veil(&wallet, &["address"], &[PASSWORD]).stdout.trim(),
        address
    );

    // A second sync has little left to do.
    let started = Instant::now();
    let again = veil(&wallet, &["sync"], &[PASSWORD]);
    println!("--- veil sync again, {:.0?}", started.elapsed());
    print!("{}", again.stderr);
    assert!(again.ok(), "second sync failed: {}", again.stderr);
}
