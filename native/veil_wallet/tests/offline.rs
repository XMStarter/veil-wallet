//! Wallet behaviour that needs the wallet library but no node.

mod common;

use std::process::{Command, Stdio};

use common::{names_under, wallet_library, RingDatabaseBefore, TempDir};
use veil_wallet::{
    Amount, Error, Network, Node, SeedKind, SeedPhrase, SendAmount, Wallet, WalletLibrary,
    MAX_UNUSED_ADDRESSES,
};

const PASSWORD: &str = "correct horse battery staple";

#[test]
fn loading_a_missing_library_fails_cleanly() {
    match WalletLibrary::load("/nonexistent/libmonero_wallet2_api_c.so") {
        Err(Error::Library { .. }) => {}
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("a missing library must not load"),
    }
}

#[test]
fn new_wallet_has_a_sixteen_word_phrase_and_a_subaddress() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("create");
    let (mut wallet, phrase) =
        Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();

    assert_eq!(phrase.kind(), SeedKind::Polyseed);
    assert_eq!(phrase.words().count(), 16);
    assert_eq!(wallet.seed_phrase().unwrap().as_str(), phrase.as_str());

    let address = wallet.receive_address().unwrap();
    assert_eq!(address.index, 1, "the primary address is never handed out");
    assert!(
        address.address.starts_with('8'),
        "mainnet subaddresses start with 8: {}",
        address.address
    );
    assert_eq!(address.address.len(), 95);
    assert!(wallet.is_valid_address(&address.address));
}

#[test]
fn two_new_wallets_differ() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("distinct");
    let (mut a, phrase_a) =
        Wallet::create(&lib, &dir.file("a"), PASSWORD, Network::Mainnet).unwrap();
    let (mut b, phrase_b) =
        Wallet::create(&lib, &dir.file("b"), PASSWORD, Network::Mainnet).unwrap();
    assert_ne!(phrase_a.as_str(), phrase_b.as_str());
    assert_ne!(
        a.receive_address().unwrap().address,
        b.receive_address().unwrap().address
    );
}

#[test]
fn receive_address_stays_the_same_until_it_is_used() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("stable");
    let (mut wallet, _) = Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();
    let first = wallet.receive_address().unwrap();
    let again = wallet.receive_address().unwrap();
    assert_eq!(first, again);
}

#[test]
fn extra_addresses_are_capped() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("cap");
    let (mut wallet, _) = Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();

    let mut seen = std::collections::BTreeSet::new();
    for expected_index in 1..=MAX_UNUSED_ADDRESSES {
        let issued = wallet.new_receive_address().unwrap();
        assert_eq!(issued.index, expected_index);
        assert!(seen.insert(issued.address), "addresses must be distinct");
    }
    match wallet.new_receive_address() {
        Err(Error::TooManyUnusedAddresses { unused }) => assert_eq!(unused, MAX_UNUSED_ADDRESSES),
        other => panic!("expected the cap to apply, got {other:?}"),
    }
    // The latest issued address is the one shown.
    assert_eq!(
        wallet.receive_address().unwrap().index,
        MAX_UNUSED_ADDRESSES
    );
}

#[test]
fn wallet_survives_close_and_reopen() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("reopen");
    let path = dir.file("w");
    let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    let first = wallet.receive_address().unwrap();
    let second = wallet.new_receive_address().unwrap();
    wallet.close().unwrap();

    let mut reopened = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.seed_phrase().unwrap().as_str(), phrase.as_str());
    assert_eq!(reopened.receive_address().unwrap(), second);
    assert_ne!(first, second);
}

/// A user's folders are named in the user's language. On Windows the
/// wallet library takes a path as bytes while the system wants UTF-16, so
/// this is where a path with anything but English letters could go wrong.
#[test]
fn a_wallet_can_live_in_a_folder_with_any_name() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("names");
    let folder = dir.file("Мой кошелёk 钱包 é");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("w");

    let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    let address = wallet.receive_address().unwrap();
    wallet.close().unwrap();
    assert!(
        folder.join("w.keys").exists(),
        "the wallet's files are where they were asked to be: {:?}",
        std::fs::read_dir(&folder)
            .map(|entries| entries
                .filter_map(|entry| entry.ok().map(|e| e.file_name()))
                .collect::<Vec<_>>())
            .unwrap_or_default()
    );

    // A second wallet may not overwrite it.
    match Wallet::create(&lib, &path, PASSWORD, Network::Mainnet) {
        Err(Error::WalletExists(_)) => {}
        Err(other) => panic!("refused, but not as an existing wallet: {other}"),
        Ok(_) => panic!("a second wallet was created over the first"),
    }

    let mut reopened = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.seed_phrase().unwrap().as_str(), phrase.as_str());
    assert_eq!(reopened.receive_address().unwrap(), address);
    reopened.close().unwrap();

    // Restoring over it is refused as well.
    let restored = Wallet::restore(&lib, &path, PASSWORD, Network::Mainnet, &phrase, 0);
    assert!(matches!(restored, Err(Error::WalletExists(_))));
}

#[test]
fn wrong_password_does_not_open_the_wallet() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("password");
    let path = dir.file("w");
    let (wallet, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    wallet.close().unwrap();

    match Wallet::open(&lib, &path, "not the password", Network::Mainnet) {
        Err(Error::WrongPassword) => {}
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("a wrong password must not open the wallet"),
    }
    assert!(Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).is_ok());
}

#[test]
fn creating_over_an_existing_wallet_is_refused() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("exists");
    let path = dir.file("w");
    let (wallet, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    wallet.close().unwrap();

    match Wallet::create(&lib, &path, PASSWORD, Network::Mainnet) {
        Err(Error::WalletExists(existing)) => assert_eq!(existing, path),
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("an existing wallet must not be overwritten"),
    }
}

#[test]
fn restoring_from_the_sixteen_word_phrase_gives_the_same_wallet() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("restore16");
    let (mut original, phrase) =
        Wallet::create(&lib, &dir.file("original"), PASSWORD, Network::Mainnet).unwrap();
    let typed = SeedPhrase::parse(&phrase.as_str().to_uppercase()).unwrap();

    let mut restored = Wallet::restore(
        &lib,
        &dir.file("restored"),
        "another password",
        Network::Mainnet,
        &typed,
        0,
    )
    .unwrap();

    assert_eq!(
        restored.receive_address().unwrap(),
        original.receive_address().unwrap()
    );
    assert_eq!(restored.seed_phrase().unwrap().as_str(), phrase.as_str());
}

#[test]
fn restoring_from_the_twenty_five_word_phrase_gives_the_same_wallet() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("restore25");
    let (mut original, _) =
        Wallet::create(&lib, &dir.file("original"), PASSWORD, Network::Mainnet).unwrap();
    let legacy = original.legacy_seed_phrase().unwrap();
    assert_eq!(legacy.kind(), SeedKind::Legacy);

    let mut restored = Wallet::restore(
        &lib,
        &dir.file("restored"),
        PASSWORD,
        Network::Mainnet,
        &legacy,
        1_234_567,
    )
    .unwrap();

    assert_eq!(
        restored.receive_address().unwrap(),
        original.receive_address().unwrap()
    );
    assert_eq!(restored.restore_height(), 1_234_567);
    assert_eq!(restored.seed_phrase().unwrap().kind(), SeedKind::Legacy);
}

#[test]
fn a_phrase_with_a_wrong_word_is_refused() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("badphrase");
    let (_, phrase) = Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();

    let mut words: Vec<&str> = phrase.words().collect();
    words[3] = if words[3] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    let damaged = SeedPhrase::parse(&words.join(" ")).unwrap();

    let result = Wallet::restore(
        &lib,
        &dir.file("restored"),
        PASSWORD,
        Network::Mainnet,
        &damaged,
        0,
    );
    assert!(
        matches!(result, Err(Error::Wallet(_))),
        "the checksum must catch a wrong word"
    );
}

#[test]
fn addresses_are_validated_per_network() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("addresses");
    let (mut mainnet, _) =
        Wallet::create(&lib, &dir.file("main"), PASSWORD, Network::Mainnet).unwrap();
    let (mut stagenet, _) =
        Wallet::create(&lib, &dir.file("stage"), PASSWORD, Network::Stagenet).unwrap();

    let main_address = mainnet.receive_address().unwrap().address;
    let stage_address = stagenet.receive_address().unwrap().address;

    assert!(mainnet.is_valid_address(&main_address));
    assert!(mainnet.is_valid_address(&format!("  {main_address}\n")));
    assert!(!mainnet.is_valid_address(&stage_address));
    assert!(stagenet.is_valid_address(&stage_address));
    assert!(!stagenet.is_valid_address(&main_address));

    let mut typo = main_address.clone().into_bytes();
    let last = typo.len() - 1;
    typo[last] = if typo[last] == b'A' { b'B' } else { b'A' };
    assert!(!mainnet.is_valid_address(&String::from_utf8(typo).unwrap()));
    for garbage in [
        "",
        "8",
        "not an address",
        "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
    ] {
        assert!(!mainnet.is_valid_address(garbage), "{garbage:?}");
    }
}

#[test]
fn sending_is_refused_before_anything_is_built() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("send");
    let (mut wallet, _) = Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();
    let own = wallet.receive_address().unwrap().address;
    let one = SendAmount::Exact("1".parse::<Amount>().unwrap());

    assert!(matches!(
        wallet.prepare_send("not an address", one).err(),
        Some(Error::InvalidAddress)
    ));
    assert!(matches!(
        wallet
            .prepare_send(&own, SendAmount::Exact(Amount::ZERO))
            .err(),
        Some(Error::ZeroAmount)
    ));
}

#[test]
fn only_onion_nodes_are_accepted() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("node");
    let (mut wallet, _) = Wallet::create(&lib, &dir.file("w"), PASSWORD, Network::Mainnet).unwrap();

    for host in [
        "node.example.com",
        "203.0.113.7",
        "127.0.0.1",
        "localhost",
        "",
    ] {
        let node = Node::Onion {
            host: host.into(),
            port: 18081,
            socks_port: 9050,
        };
        assert!(
            matches!(wallet.connect(&node), Err(Error::NodeNotOnion)),
            "{host:?} must be refused"
        );
    }

    let onion = Node::Onion {
        host: "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion".into(),
        port: 18081,
        socks_port: 9050,
    };
    wallet.connect(&onion).unwrap();
}

/// Lists a wallet folder's file names, sorted.
fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_wallet_that_is_open_already_is_not_opened_twice_and_not_touched() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("open-twice");
    let path = dir.file("w");
    let (mut first, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    let address = first.receive_address().unwrap();
    first.save().unwrap();
    let files_before = files_in(&dir.0);

    // A second window of the app, say. The failure is told apart from a
    // wallet that cannot be read, with the right password and the wrong.
    for password in [PASSWORD, "not the password"] {
        match Wallet::open(&lib, &path, password, Network::Mainnet) {
            Err(Error::WalletInUse) => {}
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("a wallet that is open must not be opened again"),
        }
    }
    // Nothing was set aside or renamed under the wallet that is running,
    assert_eq!(files_in(&dir.0), files_before);
    // which goes on saving, and opens as it was once it has been closed.
    first.save().unwrap();
    first.close().unwrap();
    let mut again = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert!(!again.was_rebuilt_from_keys());
    assert_eq!(again.receive_address().unwrap(), address);
}

#[test]
fn a_wallet_whose_data_file_is_damaged_opens_from_its_keys() {
    let Some(lib) = wallet_library() else { return };
    type Damage = fn(&[u8]) -> Vec<u8>;
    let damages: [(&str, Damage); 3] = [
        ("emptied", |_| Vec::new()),
        ("cut in half", |bytes| bytes[..bytes.len() / 2].to_vec()),
        ("overwritten", |bytes| vec![0x5a; bytes.len()]),
    ];
    for (what, damage) in damages {
        let dir = TempDir::new("damaged");
        let path = dir.file("w");
        let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
        let address = wallet.receive_address().unwrap();
        let start = wallet.restore_height();
        assert!(!wallet.was_rebuilt_from_keys());
        wallet.close().unwrap();

        let intact = std::fs::read(&path).unwrap();
        let damaged = damage(&intact);
        std::fs::write(&path, &damaged).unwrap();

        // A wrong password still opens nothing, and moves nothing.
        assert!(Wallet::open(&lib, &path, "not the password", Network::Mainnet).is_err());
        // The third is the mark that says a wallet is open; it stays where
        // the wallet is.
        assert_eq!(
            files_in(path.parent().unwrap()),
            ["w", "w.keys", "w.lock"],
            "{what}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), damaged, "{what}");

        // The right one opens the same wallet, and says how.
        let mut wallet = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet)
            .unwrap_or_else(|error| panic!("{what}: the wallet does not open: {error}"));
        assert!(wallet.was_rebuilt_from_keys(), "{what}");
        assert_eq!(wallet.seed_phrase().unwrap().as_str(), phrase.as_str());
        assert_eq!(wallet.restore_height(), start, "{what}");
        assert_eq!(wallet.receive_address().unwrap(), address, "{what}");
        // The file that could not be read is kept, not deleted.
        let aside = path.with_file_name("w.unreadable-1");
        assert_eq!(std::fs::read(&aside).unwrap(), damaged, "{what}");
        wallet.close().unwrap();

        // From then on it is an ordinary wallet again. Nothing else was
        // set aside; the library may have left a copy of its own of the
        // file it could not read, ending in `.unportable`.
        let wallet = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
        assert!(!wallet.was_rebuilt_from_keys(), "{what}");
        let files: Vec<String> = files_in(path.parent().unwrap())
            .into_iter()
            .filter(|name| name != "w.unportable")
            .collect();
        assert_eq!(files, ["w", "w.keys", "w.lock", "w.unreadable-1"], "{what}");
    }
}

#[test]
fn a_wallet_without_its_keys_file_does_not_open_and_nothing_is_moved() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("no-keys");
    let path = dir.file("w");
    let (wallet, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    wallet.close().unwrap();
    std::fs::remove_file(path.with_file_name("w.keys")).unwrap();

    assert!(Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).is_err());
    assert_eq!(files_in(path.parent().unwrap()), ["w", "w.lock"]);
}

/// Where scanning starts decides whether old payments are found, so it has
/// to survive the wallet being closed.
#[test]
fn the_start_height_given_at_creation_or_restore_is_kept_on_disk() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("start-height");
    let reopened_height = |name: &str| {
        Wallet::open(&lib, &dir.file(name), PASSWORD, Network::Mainnet)
            .unwrap()
            .restore_height()
    };

    // A new wallet: about a month before today.
    let (new, phrase) = Wallet::create(&lib, &dir.file("new"), PASSWORD, Network::Mainnet).unwrap();
    let created_at = new.restore_height();
    assert!(created_at > 3_000_000, "{created_at}");
    let legacy = new.legacy_seed_phrase().unwrap();
    new.close().unwrap();
    assert_eq!(reopened_height("new"), created_at);

    // Restored from the sixteen words: the date inside the phrase.
    let from_date = Wallet::restore(
        &lib,
        &dir.file("16"),
        PASSWORD,
        Network::Mainnet,
        &phrase,
        0,
    )
    .unwrap();
    let dated = from_date.restore_height();
    assert!(dated > 3_000_000 && dated <= created_at, "{dated}");
    from_date.close().unwrap();
    assert_eq!(reopened_height("16"), dated);

    // Restored with a height the user chose. It applies to the twenty-five
    // words; the sixteen keep their own date.
    for (name, phrase, expected) in [
        ("16-chosen", &phrase, dated),
        ("25-chosen", &legacy, 1_234_567),
    ] {
        let chosen = Wallet::restore(
            &lib,
            &dir.file(name),
            PASSWORD,
            Network::Mainnet,
            phrase,
            1_234_567,
        )
        .unwrap();
        assert_eq!(chosen.restore_height(), expected, "{name}");
        chosen.close().unwrap();
        assert_eq!(reopened_height(name), expected, "{name}");
    }

    // Changed afterwards: for as long as the wallet is open, and no longer.
    // The library keeps this height in the keys file, which it writes once.
    let mut changed = Wallet::open(&lib, &dir.file("new"), PASSWORD, Network::Mainnet).unwrap();
    changed.set_restore_height(2_000_000);
    assert_eq!(changed.restore_height(), 2_000_000);
    changed.close().unwrap();
    assert_eq!(reopened_height("new"), created_at);
}

#[test]
fn the_password_can_be_checked_and_changed() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("change-password");
    let path = dir.file("w");
    let new = "a new and longer password";
    let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    let address = wallet.receive_address().unwrap();

    // The open wallet knows its password from any other, however close.
    assert!(wallet.is_password(PASSWORD));
    assert!(!wallet.is_password(""));
    assert!(!wallet.is_password(&PASSWORD[..PASSWORD.len() - 1]));
    assert!(!wallet.is_password(&format!("{PASSWORD} ")));
    assert!(!wallet.is_password(new));

    wallet.change_password(new).unwrap();
    assert!(wallet.is_password(new));
    assert!(!wallet.is_password(PASSWORD));
    wallet.close().unwrap();

    // From now on only the new one opens it, and it is the same wallet.
    match Wallet::open(&lib, &path, PASSWORD, Network::Mainnet) {
        Err(Error::WrongPassword) => {}
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("the old password must not open the wallet any more"),
    }
    let mut reopened = Wallet::open(&lib, &path, new, Network::Mainnet).unwrap();
    assert!(!reopened.was_rebuilt_from_keys());
    assert!(reopened.is_password(new));
    assert_eq!(reopened.seed_phrase().unwrap().as_str(), phrase.as_str());
    assert_eq!(reopened.receive_address().unwrap(), address);
}

#[test]
fn a_restored_wallet_knows_its_password_too() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("restored-password");
    let (wallet, phrase) =
        Wallet::create(&lib, &dir.file("first"), PASSWORD, Network::Mainnet).unwrap();
    wallet.close().unwrap();
    let restored = Wallet::restore(
        &lib,
        &dir.file("second"),
        "another password",
        Network::Mainnet,
        &phrase,
        0,
    )
    .unwrap();
    assert!(restored.is_password("another password"));
    assert!(!restored.is_password(PASSWORD));
}

#[test]
fn a_note_is_kept_with_the_wallet_and_can_be_cleared() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("note");
    let path = dir.file("w");
    let (mut wallet, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(wallet.note("veil.test").unwrap(), "");
    wallet.set_note("veil.test", "1795400000 abc,def").unwrap();
    assert_eq!(wallet.note("veil.test").unwrap(), "1795400000 abc,def");
    wallet.close().unwrap();

    // It is in the data file, which is encrypted: not readable from outside.
    let on_disk = std::fs::read(&path).unwrap();
    assert!(!on_disk.windows(7).any(|window| window == b"abc,def"));

    let mut reopened = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.note("veil.test").unwrap(), "1795400000 abc,def");
    assert_eq!(reopened.note("veil.other").unwrap(), "");
    reopened.set_note("veil.test", "").unwrap();
    reopened.close().unwrap();
    let reopened = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.note("veil.test").unwrap(), "");
}

/// Tells a run of this test program that it is the process whose wallets
/// `a_wallet_leaves_nothing_outside_its_folder` looks at afterwards, and
/// in which folder to keep them.
const TRACES_WALLETS: &str = "VEIL_TEST_TRACES_WALLETS";
const NEW_PASSWORD: &str = "a new and longer password";

/// What an open wallet is asked to do, short of anything that needs a
/// node: addresses, a note, saving, a node that is not there, and a
/// payment that cannot be built.
fn use_a_wallet(wallet: &mut Wallet, socks_port: u16) {
    let own = wallet.receive_address().unwrap().address;
    wallet.new_receive_address().unwrap();
    wallet.set_note("veil.test", "a note").unwrap();
    wallet.save().unwrap();
    let node = Node::Onion {
        host: "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion".into(),
        port: 18081,
        socks_port,
    };
    wallet.connect(&node).unwrap();
    match wallet.refresh() {
        Err(Error::NodeUnreachable) => {}
        other => panic!("nothing listens for this wallet, and yet: {other:?}"),
    }
    let one = SendAmount::Exact("1".parse::<Amount>().unwrap());
    assert!(wallet.prepare_send(&own, one).is_err());
    wallet.save().unwrap();
}

/// The body of the process that `a_wallet_leaves_nothing_outside_its_folder`
/// starts: every way a wallet is made or opened, on both networks the
/// library keeps apart on disk, and each wallet used. Run on its own it
/// does nothing and passes.
#[test]
fn wallets_are_used_in_a_process_of_their_own() {
    let Some(folder) = std::env::var_os(TRACES_WALLETS) else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let library = std::env::var_os("VEIL_WALLET_LIB").expect("the process is given the library");
    let lib = WalletLibrary::load(library).expect("wallet library loads");
    // A port of this machine that nothing listens on: taken and let go.
    let socks_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    for (name, network) in [("main", Network::Mainnet), ("stage", Network::Stagenet)] {
        let path = folder.join(name);

        // Created.
        let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, network).unwrap();
        use_a_wallet(&mut wallet, socks_port);
        let legacy = wallet.legacy_seed_phrase().unwrap();
        wallet.close().unwrap();

        // Restored, from either kind of phrase.
        for (suffix, phrase, height) in [("16", &phrase, 0), ("25", &legacy, 1_234_567)] {
            let path = folder.join(format!("{name}-{suffix}"));
            let mut restored =
                Wallet::restore(&lib, &path, PASSWORD, network, phrase, height).unwrap();
            use_a_wallet(&mut restored, socks_port);
            restored.close().unwrap();
        }

        // Opened: refused for a wrong password first, which is as far as
        // the library has to get to make a ring database of its own.
        match Wallet::open(&lib, &path, "not the password", network) {
            Err(Error::WrongPassword) => {}
            other => panic!("a wrong password: {:?}", other.err()),
        }
        let mut wallet = Wallet::open(&lib, &path, PASSWORD, network).unwrap();
        use_a_wallet(&mut wallet, socks_port);
        wallet.change_password(NEW_PASSWORD).unwrap();
        wallet.close().unwrap();

        // Opened from its keys alone, the data file being damaged.
        std::fs::write(&path, b"not a wallet's data").unwrap();
        let mut wallet = Wallet::open(&lib, &path, NEW_PASSWORD, network).unwrap();
        assert!(wallet.was_rebuilt_from_keys());
        use_a_wallet(&mut wallet, socks_port);
        wallet.close().unwrap();
    }
}

/// Removing a wallet removes its folder, so whatever the library keeps
/// anywhere else would stay behind and say that a Monero wallet was used
/// on this machine. Left to itself the library keeps a "shared ring
/// database" there: under the user's home on Linux, and on Windows in a
/// folder that all users of the machine share.
///
/// On Linux the process that uses the wallets gets an empty folder for a
/// home, so anything made there is seen. Windows gives a test no such
/// handle: there the place itself is looked at before and after.
#[test]
fn a_wallet_leaves_nothing_outside_its_folder() {
    let Some(_lib) = wallet_library() else { return };
    let dir = TempDir::new("traces");
    let [home, temporary, working, wallets] =
        ["home", "temporary", "working", "wallets"].map(|name| {
            let folder = dir.file(name);
            std::fs::create_dir_all(&folder).unwrap();
            folder
        });
    let library = std::env::var_os("VEIL_WALLET_LIB").expect("checked by wallet_library");
    let library = std::path::absolute(library).expect("the library's path");
    let ring_database = RingDatabaseBefore::look();

    let user = Command::new(std::env::current_exe().expect("the test program has a path"))
        .args([
            "--exact",
            "wallets_are_used_in_a_process_of_their_own",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(TRACES_WALLETS, &wallets)
        .env("VEIL_WALLET_LIB", &library)
        // Where a program that knows no better puts things: the home, the
        // folder for temporary files, and the folder it was started in.
        .env("HOME", &home)
        .env("TMPDIR", &temporary)
        .env("TMP", &temporary)
        .env("TEMP", &temporary)
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("XDG_STATE_HOME")
        .current_dir(&working)
        .stdin(Stdio::null())
        .output()
        .expect("the process that uses the wallets starts");
    assert!(
        user.status.success(),
        "the process that uses the wallets failed: {}\n{}\n{}",
        user.status,
        String::from_utf8_lossy(&user.stdout),
        String::from_utf8_lossy(&user.stderr)
    );

    // The wallets are where they were asked to be, which also says that
    // the process did its work. Besides its two files a wallet has Veil's
    // mark that it is open; the damaged file is set aside, and the library
    // may keep a copy of its own of it (`.unportable`). The text file with
    // the wallet's address, which the library writes on any network but
    // the main one, is not among them.
    let mut expected = Vec::new();
    for name in ["main", "stage"] {
        for wallet in [name.to_string(), format!("{name}-16"), format!("{name}-25")] {
            expected.extend([
                wallet.clone(),
                format!("{wallet}.keys"),
                format!("{wallet}.lock"),
            ]);
        }
        expected.push(format!("{name}.unreadable-1"));
    }
    expected.sort();
    let found: Vec<String> = names_under(&wallets)
        .into_iter()
        .filter(|name| !name.ends_with(".unportable"))
        .collect();
    assert_eq!(found, expected);

    // And nothing is anywhere else.
    for (what, folder) in [
        ("its home", &home),
        ("the folder for temporary files", &temporary),
        ("the folder it was started in", &working),
    ] {
        assert_eq!(
            names_under(folder),
            Vec::<String>::new(),
            "using a wallet left something in {what}"
        );
    }
    ring_database.assert_untouched();
}

/// On a test network the library writes the primary address of a wallet it
/// makes into a text file next to it. Veil never shows that address, and a
/// wallet's folder should say nothing that the wallet's password does not
/// protect.
#[test]
fn the_wallets_address_is_not_left_as_text_next_to_it() {
    let Some(lib) = wallet_library() else { return };
    for network in [Network::Stagenet, Network::Testnet, Network::Mainnet] {
        let dir = TempDir::new("address-file");
        let path = dir.file("w");
        let address_file = dir.file("w.address.txt");

        // Created, and restored from either kind of phrase.
        let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, network).unwrap();
        let address = wallet.receive_address().unwrap();
        let legacy = wallet.legacy_seed_phrase().unwrap();
        wallet.close().unwrap();
        for (name, phrase) in [("r16", &phrase), ("r25", &legacy)] {
            let restored =
                Wallet::restore(&lib, &dir.file(name), PASSWORD, network, phrase, 0).unwrap();
            restored.close().unwrap();
        }
        assert_eq!(
            files_in(&dir.0),
            [
                "r16", "r16.keys", "r16.lock", "r25", "r25.keys", "r25.lock", "w", "w.keys",
                "w.lock"
            ],
            "{network:?}"
        );

        // A wallet made before Veil took the file away still has it. A
        // wrong password changes nothing; opening the wallet removes it.
        std::fs::write(&address_file, "what the library wrote here").unwrap();
        assert!(Wallet::open(&lib, &path, "not the password", network).is_err());
        assert!(address_file.exists(), "{network:?}");
        let mut wallet = Wallet::open(&lib, &path, PASSWORD, network).unwrap();
        assert!(!address_file.exists(), "{network:?}");

        // The library writes the keys again when the password changes, and
        // the text file with them where there is one. There is none.
        wallet.change_password(NEW_PASSWORD).unwrap();
        wallet.save().unwrap();
        wallet.close().unwrap();
        let mut wallet = Wallet::open(&lib, &path, NEW_PASSWORD, network).unwrap();
        assert_eq!(wallet.receive_address().unwrap(), address);
        wallet.close().unwrap();
        assert!(!address_file.exists(), "{network:?}");
    }
}

/// Whatever is put where an address goes, the library is asked about it
/// and says no, unless it is one of the addresses, whole.
#[test]
fn made_up_addresses_are_judged_and_only_whole_ones_pass() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("made-up-addresses");
    let (mut stagenet, _) =
        Wallet::create(&lib, &dir.file("stage"), PASSWORD, Network::Stagenet).unwrap();
    let (mut mainnet, _) =
        Wallet::create(&lib, &dir.file("main"), PASSWORD, Network::Mainnet).unwrap();
    let first = stagenet.receive_address().unwrap().address;
    let second = stagenet.new_receive_address().unwrap().address;
    let elsewhere = mainnet.receive_address().unwrap().address;
    let untidy = format!("  {first}\n");
    let link = format!("monero:{first}?tx_amount=0.25");
    let samples = [
        first.as_str(),
        second.as_str(),
        elsewhere.as_str(),
        untidy.as_str(),
        link.as_str(),
        "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
    ];
    let one = SendAmount::Exact("1".parse::<Amount>().unwrap());
    veil_test_guard::fuzz::texts("address", &samples, |text| {
        let whole = text.trim();
        if stagenet.is_valid_address(text) {
            // A changed address that still passed would be a payment to
            // somebody else. Four bytes of checksum stand against it.
            assert!(whole == first || whole == second, "{whole:?} passed");
        } else {
            assert!(whole != first && whole != second);
        }
        assert_eq!(mainnet.is_valid_address(text), whole == elsewhere);
    });
    // And nothing is built for what is no address.
    for text in ["", "\0", elsewhere.as_str(), link.as_str()] {
        assert!(matches!(
            stagenet.prepare_send(text, one).err(),
            Some(Error::InvalidAddress)
        ));
    }
}

/// Phrases with the right number of words and something wrong in them go
/// to the library, which refuses them and leaves nothing, or reads them
/// as a wallet. It never falls over.
#[test]
fn made_up_phrases_are_refused_by_the_library_or_are_a_wallet() {
    use std::cell::Cell;

    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("made-up-phrases");
    let (mut original, sixteen) =
        Wallet::create(&lib, &dir.file("original"), PASSWORD, Network::Stagenet).unwrap();
    let twenty_five = original.legacy_seed_phrase().unwrap();
    let address = original.receive_address().unwrap().address;
    original.close().unwrap();

    let good = [sixteen.as_str().to_owned(), twenty_five.as_str().to_owned()];
    let samples = [
        "abandon",
        "ability zoo",
        "ABANDON",
        "abandonx",
        "aban",
        "zzzz",
        "1.",
        "слово",
    ];
    let case = Cell::new(0usize);
    let (refused, read) = (Cell::new(0u32), Cell::new(0u32));
    veil_test_guard::fuzz::some_texts("library phrase", &samples, 600, |text| {
        let number = case.replace(case.get() + 1);
        // A good phrase with some of its words given up for made-up ones;
        // now and then left as it is, to see that one read as well.
        let mut words: Vec<String> = good[number % 2].split(' ').map(str::to_owned).collect();
        let made_up: Vec<&str> = text.split_whitespace().collect();
        if number % 25 == 24 {
            words[0] = words[0].to_uppercase();
        } else if made_up.is_empty() {
            let at = number % words.len();
            let other = (at + 1 + number / 7) % words.len();
            words.swap(at, other);
        } else {
            for (offset, word) in made_up.iter().take(3).enumerate() {
                let at = (number / 2 + offset * 5) % words.len();
                words[at] = (*word).to_owned();
            }
        }
        let phrase = SeedPhrase::parse(&words.join(" ")).unwrap();

        let path = dir.file(&format!("restored-{number}"));
        match Wallet::restore(&lib, &path, PASSWORD, Network::Stagenet, &phrase, 0) {
            Ok(mut wallet) => {
                read.set(read.get() + 1);
                // A wallet like any other; the same one if nothing that
                // counts was changed.
                let restored = wallet.receive_address().unwrap().address;
                assert!(wallet.is_valid_address(&restored));
                if phrase.as_str() == good[number % 2] {
                    assert_eq!(restored, address);
                }
                wallet.close().unwrap();
            }
            Err(Error::Wallet(_)) => {
                refused.set(refused.get() + 1);
                let left: Vec<String> = names_under(&dir.0)
                    .into_iter()
                    .filter(|name| {
                        let own = format!("restored-{number}");
                        *name == own || name.starts_with(&format!("{own}."))
                    })
                    .collect();
                assert_eq!(left, Vec::<String>::new(), "a refused phrase left files");
            }
            // A zero byte cannot be handed to the library at all. Nothing
            // an app sends has one: its text ends at the first.
            Err(Error::InvalidArgument { .. }) => assert!(phrase.as_str().contains('\0')),
            Err(other) => panic!("an unexpected refusal: {other}"),
        }
    });
    // Both ways were taken, or this showed less than it says.
    assert!(refused.get() > 0 && read.get() > 0);
    eprintln!(
        "{} phrases refused, {} read as a wallet",
        refused.get(),
        read.get()
    );
}
