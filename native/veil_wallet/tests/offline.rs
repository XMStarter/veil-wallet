//! Wallet behaviour that needs the wallet library but no node.

mod common;

use common::{wallet_library, TempDir};
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
    assert!(matches!(
        Wallet::create(&lib, &path, PASSWORD, Network::Mainnet),
        Err(Error::WalletExists(_))
    ));

    let mut reopened = Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    assert_eq!(reopened.seed_phrase().unwrap().as_str(), phrase.as_str());
    assert_eq!(reopened.receive_address().unwrap(), address);
}

#[test]
fn wrong_password_does_not_open_the_wallet() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("password");
    let path = dir.file("w");
    let (wallet, _) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    wallet.close().unwrap();

    match Wallet::open(&lib, &path, "not the password", Network::Mainnet) {
        Err(Error::Wallet(_)) => {}
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
