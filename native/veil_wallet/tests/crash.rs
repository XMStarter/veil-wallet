//! What a kill at the wrong moment leaves behind.
//!
//! A phone runs out of battery, a laptop lid closes, a task manager ends
//! the process. None of them waits for a save to finish. The wallet's files
//! must then hold either what was there before the save or what the save
//! wrote, never something in between that no longer opens.
//!
//! Needs the wallet library (`VEIL_WALLET_LIB`) and no network.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{wallet_library, TempDir};
use veil_wallet::{Network, Wallet, WalletLibrary};

const PASSWORD: &str = "correct horse battery staple";
/// Tells a run of this test program that it is the process to be killed,
/// and where its wallet is.
const VICTIM_WALLET: &str = "VEIL_TEST_VICTIM_WALLET";
/// What the victim prints once it has opened the wallet and starts saving.
const SAVING: &str = "saving";
/// How many times the wallet is killed. Each round opens the wallet twice,
/// which is slow on purpose (the password is stretched), so this is a
/// balance between coverage and minutes. Where a Windows program is started
/// from Linux for the checks, starting it is the slow part, and there are
/// fewer rounds.
const ROUNDS: u32 = if cfg!(windows) { 8 } else { 40 };

/// The body of the process that gets killed. Run on its own it does nothing
/// and passes.
#[test]
fn victim_saves_until_killed() {
    let Some(path) = std::env::var_os(VICTIM_WALLET) else {
        return;
    };
    let library = std::env::var_os("VEIL_WALLET_LIB").expect("the victim is given the library");
    let lib = WalletLibrary::load(library).expect("wallet library loads");
    let mut wallet = Wallet::open(&lib, path.as_ref(), PASSWORD, Network::Mainnet)
        .expect("the victim opens the wallet");
    println!("{SAVING}");
    // Change something every time, so that every save writes a file that
    // differs from the one before.
    let mut height = wallet.restore_height();
    loop {
        height += 1;
        wallet.set_restore_height(height);
        wallet.save().expect("the victim saves");
    }
}

/// Starts the victim and returns once it is in its saving loop.
fn start_victim(wallet: &std::path::Path) -> std::process::Child {
    let mut victim = Command::new(std::env::current_exe().expect("the test program has a path"))
        .args([
            "--exact",
            "victim_saves_until_killed",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(VICTIM_WALLET, wallet)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the victim starts");
    let output = BufReader::new(victim.stdout.take().expect("the victim's output"));
    for line in output.lines() {
        match line {
            // The test harness prints the test's name on the same line
            // first, so the word comes at the end of a line, not alone.
            Ok(line) if line.trim_end().ends_with(SAVING) => return victim,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let _ = victim.kill();
    let status = victim.wait();
    panic!("the victim ended before it began to save: {status:?}");
}

#[test]
fn a_wallet_survives_being_killed_while_it_saves() {
    let Some(lib) = wallet_library() else { return };
    let dir = TempDir::new("crash");
    let path = dir.file("w");

    let (mut wallet, phrase) = Wallet::create(&lib, &path, PASSWORD, Network::Mainnet).unwrap();
    let address = wallet.receive_address().unwrap();
    let mut last_height = wallet.restore_height();
    wallet.close().unwrap();

    for round in 0..ROUNDS {
        let mut victim = start_victim(&path);
        // A different moment each round, spread over the first tens of
        // milliseconds of saving.
        std::thread::sleep(Duration::from_millis(u64::from(round * 37 % 60)));
        victim.kill().expect("the victim can be killed");
        victim.wait().expect("the victim is gone");

        let mut wallet =
            Wallet::open(&lib, &path, PASSWORD, Network::Mainnet).unwrap_or_else(|error| {
                panic!("after kill {round} the wallet no longer opens: {error}")
            });
        assert_eq!(
            wallet.seed_phrase().unwrap().as_str(),
            phrase.as_str(),
            "after kill {round} it is another wallet"
        );
        assert_eq!(
            wallet.receive_address().unwrap(),
            address,
            "after kill {round} the address changed"
        );
        // What the wallet holds is something the victim saved: never older
        // than the round before.
        let height = wallet.restore_height();
        assert!(
            height >= last_height,
            "after kill {round} the wallet went back: {height} < {last_height}"
        );
        last_height = height;
        wallet.close().unwrap();
    }

    // Whatever the kills left lying around, the directory holds one wallet.
    let leftovers: Vec<String> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    println!("files after {ROUNDS} kills: {leftovers:?}");
    assert!(leftovers.iter().any(|name| name == "w.keys"));
}
