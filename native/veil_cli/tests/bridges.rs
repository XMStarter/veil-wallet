//! The tool reaching Tor through bridges, on the real network.
//!
//! Ignored by default and refused on a CI runner. Needs the wallet library
//! (`VEIL_WALLET_LIB`) and an installed obfs4 program. Run it with
//! `../check.sh bridges`.

mod common;

use std::time::Instant;

use common::{have_wallet_library, veil, TempDir};

const PASSWORD: &str = "not a real wallet";

/// The obfs4 bridges built into Tor Browser, as served by the Tor Project.
/// The same lines as in `veil_tor`'s bridge test.
const BUILTIN_OBFS4: &str = "\
# Tor Browser's built-in obfs4 bridges
obfs4 37.218.245.14:38224 D9A82D2F9C2F65A18407B1D2B764F130847F8B5D cert=bjRaMrr1BRiAW8IE9U5z27fQaYgOhX1UCmOpg2pFpoMvo6ZgQMzLsaTzzQNTlm7hNcb+Sg iat-mode=0
obfs4 212.83.43.95:443 BFE712113A72899AD685764B211FACD30FF52C31 cert=ayq0XzCwhpdysn5o0EyDUbmSOx3X/oTEbzDMvczHOdBJKlvIdHHLJGkZARtT4dcBFArPPg iat-mode=1
obfs4 51.222.13.177:80 5EDAC3B810E12B01F6FD8050D2FD3E277B289A08 cert=2uplIpLQ0q9+0qMFrK5pkaYRDOe460LL9WHBvatgkuRr/SL31wBOEupaMMJ6koRE6Ld0ew iat-mode=0
obfs4 146.57.248.225:22 10A6CD36A537FCE513A322361547444B393989F0 cert=K1gDtDAIcUfeLqbstggjIw2rtgIKqdIhUlHp82XRqNSq/mtAjp1BIC9vHKJ2FAEpGssTPw iat-mode=0
";

#[test]
#[ignore = "needs the Tor network, an obfs4 program, and a public stagenet node"]
fn a_wallet_synchronises_through_bridges() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("bridges");
    let wallet = dir.path("w");
    let bridge_file = dir.path("bridges.txt");
    std::fs::write(&bridge_file, BUILTIN_OBFS4).unwrap();
    let bridge_file = bridge_file.to_str().unwrap();

    assert!(veil(&wallet, &["create"], &[PASSWORD, PASSWORD]).ok());

    let started = Instant::now();
    let synced = veil(&wallet, &["sync", "--bridges", bridge_file], &[PASSWORD]);
    println!("--- veil sync --bridges, {:.0?}", started.elapsed());
    print!("{}", synced.stderr);
    print!("{}", synced.stdout);
    assert!(
        synced.ok(),
        "sync through bridges failed: {}",
        synced.stderr
    );
    assert!(synced.stderr.contains("Using 4 bridges (obfs4)."));
    assert!(synced.stderr.contains("Tor is connected"));
    assert!(
        synced.stdout.contains("Balance:   0.00 XMR"),
        "{}",
        synced.stdout
    );

    // A bridge file with nothing usable in it stops the command before Tor
    // or the wallet is touched.
    let empty = dir.path("empty.txt");
    std::fs::write(&empty, "# nothing here\n").unwrap();
    let refused = veil(
        &wallet,
        &["sync", "--bridges", empty.to_str().unwrap()],
        &[PASSWORD],
    );
    assert_eq!(refused.code, Some(1));
    assert!(
        refused.stderr.contains("no bridge lines"),
        "{}",
        refused.stderr
    );
}
