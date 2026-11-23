//! What each command does.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use veil_core::{Core, Nodes, OnionNode};
use veil_tor::{State, TorConfig, TorService};
use veil_wallet::{
    Amount, Direction, Network, SeedPhrase, SendAmount, SyncProgress, Wallet, WalletLibrary,
};
use zeroize::Zeroizing;

use crate::args::{Command, Options, USAGE};
use crate::{nodes, prompt};

/// The only network this tool works on. Stagenet coins have no value.
const NETWORK: Network = Network::Stagenet;
/// How long Tor gets to connect before the tool gives up.
const TOR_TIMEOUT: Duration = Duration::from_secs(600);

/// A message for the user, in place of a result.
#[derive(Debug)]
pub struct Fail(pub String);

impl From<String> for Fail {
    fn from(message: String) -> Fail {
        Fail(message)
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Fail {
        Fail(message.to_owned())
    }
}

macro_rules! fail_from {
    ($($error:ty),*) => {$(
        impl From<$error> for Fail {
            fn from(error: $error) -> Fail {
                Fail(error.to_string())
            }
        }
    )*};
}
fail_from!(
    io::Error,
    veil_wallet::Error,
    veil_core::CoreError,
    veil_tor::TorError
);

pub fn run(options: Options) -> Result<(), Fail> {
    if options.command == Command::Help {
        print!("{USAGE}");
        return Ok(());
    }
    let session = Session::new(options.data_dir, options.wallet_lib)?;
    match options.command {
        Command::Help => Ok(()),
        Command::Create => session.create(),
        Command::Restore { height } => session.restore(height),
        Command::Address { new } => session.address(new),
        Command::Seed => session.seed(),
        Command::Balance => session.balance(),
        Command::History => session.history(),
        Command::Sync { nodes } => session.sync(nodes),
        Command::Send { to, amount, nodes } => session.send(&to, amount, nodes),
    }
}

/// The wallet library and the directory everything is kept in.
struct Session {
    lib: WalletLibrary,
    data_dir: PathBuf,
}

impl Session {
    fn new(data_dir: Option<PathBuf>, wallet_lib: Option<PathBuf>) -> Result<Session, Fail> {
        let data_dir = match data_dir {
            Some(dir) => dir,
            None => std::env::home_dir()
                .ok_or("cannot tell where the home directory is; give --data-dir")?
                .join(".veil")
                .join("stagenet"),
        };
        make_private_dir(&data_dir)?;
        let lib = WalletLibrary::load(find_wallet_library(wallet_lib)?)?;
        Ok(Session { lib, data_dir })
    }

    fn wallet_path(&self) -> PathBuf {
        self.data_dir.join("wallet")
    }

    fn wallet_exists(&self) -> bool {
        self.data_dir.join("wallet.keys").exists()
    }

    fn refuse_to_overwrite(&self) -> Result<(), Fail> {
        if self.wallet_exists() {
            return Err(format!(
                "there already is a wallet in {}; choose another --data-dir",
                self.data_dir.display()
            )
            .into());
        }
        Ok(())
    }

    fn open(&self) -> Result<Wallet, Fail> {
        if !self.wallet_exists() {
            return Err(format!(
                "there is no wallet in {}; run `veil create` or `veil restore` first",
                self.data_dir.display()
            )
            .into());
        }
        let password = prompt::secret("Password: ")?;
        Ok(Wallet::open(
            &self.lib,
            &self.wallet_path(),
            &password,
            NETWORK,
        )?)
    }

    fn create(&self) -> Result<(), Fail> {
        self.refuse_to_overwrite()?;
        let password = new_password()?;
        let (mut wallet, phrase) =
            Wallet::create(&self.lib, &self.wallet_path(), &password, NETWORK)?;
        let address = wallet.receive_address()?;
        wallet.close()?;

        println!(
            "A new stagenet wallet was created in {}.",
            self.data_dir.display()
        );
        println!();
        println!("Recovery phrase. Write it down; it is the only backup:");
        println!();
        println!("  {}", phrase.as_str());
        println!();
        println!("Address to receive on:");
        println!("  {}", address.address);
        Ok(())
    }

    fn restore(&self, height: u64) -> Result<(), Fail> {
        self.refuse_to_overwrite()?;
        let words = prompt::secret("Recovery phrase (16 or 25 words): ")?;
        let phrase = SeedPhrase::parse(&words)?;
        let password = new_password()?;
        let mut wallet = Wallet::restore(
            &self.lib,
            &self.wallet_path(),
            &password,
            NETWORK,
            &phrase,
            height,
        )?;
        let start = wallet.restore_height();
        let address = wallet.receive_address()?;
        wallet.close()?;

        println!("The wallet was restored in {}.", self.data_dir.display());
        println!("Scanning will start at block {start}. Run `veil sync` to find its funds.");
        println!("Address to receive on:");
        println!("  {}", address.address);
        Ok(())
    }

    fn address(&self, new: bool) -> Result<(), Fail> {
        let mut wallet = self.open()?;
        let address = if new {
            wallet.new_receive_address()?
        } else {
            wallet.receive_address()?
        };
        wallet.close()?;
        println!("{}", address.address);
        Ok(())
    }

    fn seed(&self) -> Result<(), Fail> {
        let wallet = self.open()?;
        let phrase = wallet.seed_phrase()?;
        println!("{}", phrase.as_str());
        Ok(())
    }

    fn balance(&self) -> Result<(), Fail> {
        let wallet = self.open()?;
        print_balance(wallet.balance(), wallet.sync_progress().wallet_height);
        println!("This is the state at the last sync. Run `veil sync` to update it.");
        Ok(())
    }

    fn history(&self) -> Result<(), Fail> {
        let mut wallet = self.open()?;
        let transfers = wallet.history()?;
        if transfers.is_empty() {
            println!("No transfers as of the last sync.");
        }
        for transfer in transfers {
            let direction = match transfer.direction {
                Direction::Incoming => "in ",
                Direction::Outgoing => "out",
            };
            let state = if transfer.failed {
                "failed".to_owned()
            } else if transfer.pending || transfer.block_height == 0 {
                "pending".to_owned()
            } else {
                format!("block {}", transfer.block_height)
            };
            println!(
                "{direction}  {:>20} XMR  fee {:>16}  {state:<14}  {}",
                transfer.amount.to_string(),
                transfer.fee.to_string(),
                transfer.txid
            );
        }
        Ok(())
    }

    fn sync(&self, named: Vec<OnionNode>) -> Result<(), Fail> {
        let mut core = self.go_online(named)?;
        let progress = synchronise(&mut core)?;
        print_balance(core.balance(), progress.wallet_height);
        core.close()?;
        Ok(())
    }

    fn send(&self, to: &str, amount: SendAmount, named: Vec<OnionNode>) -> Result<(), Fail> {
        let mut core = self.go_online(named)?;
        if !core.is_valid_address(to) {
            return Err("that is not a stagenet address".into());
        }
        synchronise(&mut core)?;

        let sync_node = core.nodes().sync().clone();
        let broadcast_node = core.nodes().broadcast().clone();
        let pending = core.prepare_send(to, amount)?;
        let total = pending
            .amount()
            .checked_add(pending.fee())
            .unwrap_or(Amount::ZERO);
        println!("To:      {to}");
        println!("Amount:  {} XMR", pending.amount());
        println!("Fee:     {} XMR", pending.fee());
        println!("Total:   {total} XMR");
        if broadcast_node == sync_node {
            println!("Through: the node synchronised with, on a separate Tor circuit");
        } else {
            println!("Through: {broadcast_node}");
        }

        let answer = prompt::line("Send it? Type yes to confirm: ")?;
        if answer.trim() != "yes" {
            drop(pending);
            println!("Nothing was sent.");
            core.close()?;
            return Ok(());
        }
        let sent = pending.commit();
        // Save whatever happened: after a broadcast the wallet knows which
        // of its coins are spent.
        let saved = core.close();
        for txid in sent? {
            println!("Sent. Transaction {txid}");
        }
        saved?;
        Ok(())
    }

    /// Opens the wallet, starts Tor, waits for it, and finds a node.
    fn go_online(&self, named: Vec<OnionNode>) -> Result<Core, Fail> {
        let wallet = self.open()?;
        let candidates = nodes::candidates(named);
        let first = candidates.first().ok_or("there is no node to try")?.clone();

        let tor_dir = self.data_dir.join("tor");
        let tor = TorService::start(TorConfig {
            state_dir: tor_dir.join("state"),
            cache_dir: tor_dir.join("cache"),
        })?;
        let mut core = Core::new(tor, wallet, Nodes::single(first))?;
        wait_for_tor(&core)?;

        eprintln!("Looking for a node...");
        let asked = Instant::now();
        let height = core.settle_on_nodes(&candidates)?;
        eprintln!(
            "Node {} is at block {height} ({:.0?}).",
            core.nodes().sync(),
            asked.elapsed()
        );
        Ok(core)
    }
}

fn wait_for_tor(core: &Core) -> Result<(), Fail> {
    let started = Instant::now();
    let mut shown_percent = None;
    let mut shown_message = None;
    loop {
        let status = core.tor_status();
        match status.state {
            State::Ready => {
                eprintln!("Tor is connected ({:.0?}).", started.elapsed());
                return Ok(());
            }
            State::Failed => {
                return Err(format!(
                    "Tor could not start: {}",
                    status.message.as_deref().unwrap_or("no reason given")
                )
                .into());
            }
            State::Bootstrapping => {}
        }
        if shown_percent != Some(status.percent) {
            eprintln!("Connecting to Tor: {}%", status.percent);
            shown_percent = Some(status.percent);
        }
        if status.message.is_some() && shown_message != status.message {
            eprintln!(
                "Tor reports: {}",
                status.message.as_deref().unwrap_or_default()
            );
            shown_message = status.message.clone();
        }
        if started.elapsed() > TOR_TIMEOUT {
            return Err(format!(
                "Tor did not connect in {} minutes; the network may be blocking it",
                TOR_TIMEOUT.as_secs() / 60
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Synchronises and shows progress on the way.
fn synchronise(core: &mut Core) -> Result<SyncProgress, Fail> {
    let on_terminal = io::stderr().is_terminal();
    let started = Instant::now();
    let mut reports = 0u32;
    eprintln!("Synchronising...");
    let outcome = core.sync_with_progress(&mut |progress: SyncProgress| {
        reports += 1;
        let line = format!(
            "Scanned to block {} of {} ({} left)",
            progress.wallet_height,
            progress.node_height,
            progress.blocks_left()
        );
        if on_terminal {
            // Redraw one line.
            eprint!("\r{line:<70}");
            let _ = io::stderr().flush();
        } else if reports % 15 == 1 {
            eprintln!("{line}");
        }
    });
    if on_terminal && reports > 0 {
        eprintln!();
    }
    let progress = outcome?;
    eprintln!(
        "Synchronised to block {} ({:.0?}).",
        progress.wallet_height,
        started.elapsed()
    );
    Ok(progress)
}

fn print_balance(balance: veil_wallet::Balance, height: u64) {
    println!("Balance:   {} XMR", balance.total);
    println!("Spendable: {} XMR", balance.unlocked);
    println!("At block:  {height}");
}

/// Asks for a new password twice.
fn new_password() -> Result<Zeroizing<String>, Fail> {
    let password = prompt::secret("New password: ")?;
    if password.is_empty() {
        return Err("the password must not be empty".into());
    }
    let again = prompt::secret("Repeat the password: ")?;
    if *password != *again {
        return Err("the two passwords differ".into());
    }
    Ok(password)
}

/// Creates `dir` if needed and keeps other users of the machine out of it.
fn make_private_dir(dir: &Path) -> Result<(), Fail> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot protect {}: {error}", dir.display()))?;
    }
    Ok(())
}

/// The wallet library: the one named, or `VEIL_WALLET_LIB`, or the one next
/// to this program, or the one `check.sh setup` builds.
fn find_wallet_library(named: Option<PathBuf>) -> Result<PathBuf, Fail> {
    const FILE_NAMES: &[&str] = &[
        "libmonero_wallet2_api_c.so",
        "libmonero_wallet2_api_c.dylib",
        "monero_wallet2_api_c.dll",
    ];
    if let Some(path) = named {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("VEIL_WALLET_LIB") {
        return Ok(PathBuf::from(path));
    }
    let mut places = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        places.push(dir);
    }
    if let Some(home) = std::env::home_dir() {
        places.push(home.join("veil-wallet-lib").join("out"));
    }
    for place in &places {
        for name in FILE_NAMES {
            let candidate = place.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err("cannot find the Monero wallet library; give --wallet-lib or set VEIL_WALLET_LIB".into())
}
