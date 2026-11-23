//! The commands that need no network.
//!
//! All but the first two tests need the wallet library (`VEIL_WALLET_LIB`)
//! and are skipped without it.

mod common;

use common::{address_in, have_wallet_library, phrase_in, veil, TempDir};

const PASSWORD: &str = "correct horse battery staple";

#[test]
fn help_needs_nothing_and_says_what_the_tool_is_for() {
    let dir = TempDir::new("help");
    for arguments in [&[][..], &["--help"], &["help"]] {
        let run = veil(&dir.0, arguments, &[]);
        assert!(run.ok(), "{}", run.stderr);
        assert!(run.stdout.contains("stagenet"));
        assert!(run.stdout.contains("does not work with real funds"));
    }
}

#[test]
fn a_wrong_command_line_is_a_usage_error() {
    let dir = TempDir::new("usage");
    for arguments in [
        &["launch"][..],
        &["send"],
        &["send", "7Address", "much"],
        &["sync", "--node", "node.example.com:38081"],
        &["create", "--verbose"],
    ] {
        let run = veil(&dir.0, arguments, &[]);
        assert_eq!(run.code, Some(2), "{arguments:?}: {}", run.stderr);
        assert!(run.stdout.is_empty());
    }
}

#[test]
fn a_new_wallet_shows_its_phrase_once_and_an_address() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("create");
    let wallet = dir.path("w");

    let created = veil(&wallet, &["create"], &[PASSWORD, PASSWORD]);
    assert!(created.ok(), "{}", created.stderr);
    let phrase = phrase_in(&created);
    let address = address_in(&created);

    // The same phrase and the same address come back later.
    let seed = veil(&wallet, &["seed"], &[PASSWORD]);
    assert!(seed.ok(), "{}", seed.stderr);
    assert_eq!(seed.stdout.trim(), phrase);
    let shown = veil(&wallet, &["address"], &[PASSWORD]);
    assert_eq!(shown.stdout.trim(), address);

    // An extra address is a different one, and it becomes the current one.
    let extra = veil(&wallet, &["address", "--new"], &[PASSWORD]);
    assert!(extra.ok(), "{}", extra.stderr);
    assert_ne!(extra.stdout.trim(), address);
    let current = veil(&wallet, &["address"], &[PASSWORD]);
    assert_eq!(current.stdout.trim(), extra.stdout.trim());

    // Nothing has been received and nothing can have been.
    let balance = veil(&wallet, &["balance"], &[PASSWORD]);
    assert!(balance.ok(), "{}", balance.stderr);
    assert!(
        balance.stdout.contains("Balance:   0.00 XMR"),
        "{}",
        balance.stdout
    );
    let history = veil(&wallet, &["history"], &[PASSWORD]);
    assert!(
        history.stdout.contains("No transfers"),
        "{}",
        history.stdout
    );
}

#[test]
fn a_restored_wallet_has_the_same_addresses() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("restore");
    let created = veil(&dir.path("first"), &["create"], &[PASSWORD, PASSWORD]);
    assert!(created.ok(), "{}", created.stderr);
    let phrase = phrase_in(&created);

    let restored = veil(
        &dir.path("second"),
        &["restore"],
        &[&phrase, "another password", "another password"],
    );
    assert!(restored.ok(), "{}", restored.stderr);
    assert_eq!(address_in(&restored), address_in(&created));
    assert!(
        !restored.stdout.contains(&phrase),
        "restoring must not print the phrase back"
    );

    // The phrase is checked before anything is written.
    let wrong = veil(
        &dir.path("third"),
        &["restore"],
        &["one two three", PASSWORD, PASSWORD],
    );
    assert_eq!(wrong.code, Some(1));
    assert!(!dir.path("third").join("wallet.keys").exists());
}

#[test]
fn the_password_guards_the_wallet() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("password");
    let wallet = dir.path("w");
    assert!(veil(&wallet, &["create"], &[PASSWORD, PASSWORD]).ok());

    for command in ["seed", "address", "balance", "history"] {
        let run = veil(&wallet, &[command], &["not the password"]);
        assert_eq!(run.code, Some(1), "{command}");
        assert!(run.stdout.is_empty(), "{command} printed: {}", run.stdout);
    }
    // No answer at all is not an empty password that happens to work.
    let run = veil(&wallet, &["seed"], &[]);
    assert_eq!(run.code, Some(1));
    assert!(run.stdout.is_empty());
}

#[test]
fn a_wallet_is_never_overwritten_and_a_missing_one_is_said_to_be_missing() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("overwrite");
    let wallet = dir.path("w");
    let created = veil(&wallet, &["create"], &[PASSWORD, PASSWORD]);
    let phrase = phrase_in(&created);

    let again = veil(&wallet, &["create"], &["other", "other"]);
    assert_eq!(again.code, Some(1));
    assert!(again.stderr.contains("already"), "{}", again.stderr);
    let restore = veil(&wallet, &["restore"], &[&phrase, "other", "other"]);
    assert_eq!(restore.code, Some(1));
    // The original still opens with its own password.
    assert_eq!(veil(&wallet, &["seed"], &[PASSWORD]).stdout.trim(), phrase);

    let missing = veil(&dir.path("empty"), &["balance"], &[PASSWORD]);
    assert_eq!(missing.code, Some(1));
    assert!(missing.stderr.contains("no wallet"), "{}", missing.stderr);
}

#[test]
fn a_new_password_must_be_typed_twice_the_same_and_not_be_empty() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("new-password");
    for answers in [&["one", "two"][..], &["", ""], &["only once"]] {
        let wallet = dir.path("w");
        let run = veil(&wallet, &["create"], answers);
        assert_eq!(run.code, Some(1), "{answers:?}");
        assert!(run.stdout.is_empty());
        assert!(!wallet.join("wallet.keys").exists(), "{answers:?}");
    }
}

#[test]
fn commands_that_need_no_network_do_not_start_tor() {
    if !have_wallet_library() {
        return;
    }
    let dir = TempDir::new("no-tor");
    let wallet = dir.path("w");
    assert!(veil(&wallet, &["create"], &[PASSWORD, PASSWORD]).ok());
    for command in ["seed", "address", "balance", "history"] {
        assert!(veil(&wallet, &[command], &[PASSWORD]).ok(), "{command}");
    }
    // Tor keeps its files in the data directory from the moment it starts.
    assert!(!wallet.join("tor").exists());
}
