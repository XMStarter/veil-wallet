//! A Monero wallet: create, restore, receive, send.

use std::collections::BTreeSet;
use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::sys::{self, c_string, SecretCString, WalletLibrary};
use crate::{Amount, Error, SeedKind, SeedPhrase};
use zeroize::Zeroizing;

/// How many addresses may be issued in a row without any of them receiving a
/// payment.
///
/// A wallet restored from its phrase only looks a limited distance past the
/// last address it knows was used (200 in the wallet library). Staying far
/// below that keeps every payment findable after a restore.
pub const MAX_UNUSED_ADDRESSES: u32 = 20;

/// Veil keeps everything in the wallet's first account.
const ACCOUNT: u32 = 0;
/// Rounds of the wallet library's key derivation for the file password.
/// One is the library's default and what other wallets use.
const KDF_ROUNDS: u64 = 1;
/// Language of newly generated recovery phrases.
const SEED_LANGUAGE: &str = "English";
/// Zero lets the wallet library choose the ring size the network requires.
const RING_SIZE_AUTO: u32 = 0;
const LIST_SEPARATOR: &str = ",";

/// Which Monero network a wallet belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
    Stagenet,
}

impl Network {
    fn id(self) -> c_int {
        match self {
            Network::Mainnet => sys::NETWORK_MAINNET,
            Network::Testnet => sys::NETWORK_TESTNET,
            Network::Stagenet => sys::NETWORK_STAGENET,
        }
    }
}

/// The Monero node a wallet synchronises with.
///
/// The only node an app build can name is an onion service reached through a
/// local SOCKS proxy, which is Veil's embedded Tor client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Onion {
        /// The node's `.onion` hostname.
        host: String,
        /// The node's RPC port.
        port: u16,
        /// Port of the local SOCKS proxy on `127.0.0.1`.
        socks_port: u16,
    },
    /// A regtest daemon on this machine, reached directly. Exists only for
    /// tests; app builds do not contain it.
    #[cfg(feature = "regtest")]
    LocalRegtest { port: u16 },
}

/// Spendable and total funds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    /// Everything the wallet has received and not spent.
    pub total: Amount,
    /// The part of `total` that can be spent now. Incoming funds unlock after
    /// ten confirmations.
    pub unlocked: Amount,
}

/// How far synchronisation has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncProgress {
    /// Height the wallet has scanned to.
    pub wallet_height: u64,
    /// Height of the node's chain.
    pub node_height: u64,
}

impl SyncProgress {
    pub fn blocks_left(&self) -> u64 {
        self.node_height.saturating_sub(self.wallet_height)
    }
}

/// An address to give to a payer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveAddress {
    /// Position of the address in the wallet, starting at 1.
    pub index: u32,
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

/// One entry of the wallet's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub txid: String,
    pub direction: Direction,
    pub amount: Amount,
    /// Network fee. Zero for incoming transfers.
    pub fee: Amount,
    /// Zero while the transaction is not in a block.
    pub block_height: u64,
    pub confirmations: u64,
    /// Unix time in seconds.
    pub timestamp: u64,
    pub pending: bool,
    pub failed: bool,
    /// Receiving addresses involved, by index. Empty for outgoing transfers.
    pub address_indices: Vec<u32>,
}

/// How much to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendAmount {
    /// Exactly this much. The fee is added on top.
    Exact(Amount),
    /// The whole unlocked balance. The fee is taken out of it.
    Everything,
}

/// An open wallet.
pub struct Wallet {
    lib: WalletLibrary,
    manager: *mut c_void,
    wallet: *mut c_void,
    network: Network,
    path: PathBuf,
    /// The node given to [`Wallet::connect`].
    sync_node: Option<Node>,
    /// The node transactions are broadcast through, when it is not the
    /// sync node. See [`Wallet::set_broadcast_node`].
    broadcast_node: Option<Node>,
    /// False while the library is pointed at the broadcast node, or after a
    /// failed attempt to point it back. Nothing but a broadcast may contact
    /// a node in that state.
    on_sync_node: bool,
    /// The library's record of the last block it scanned, for reading
    /// progress while a refresh runs. Belongs to the library.
    listener: *mut c_void,
    /// See [`Wallet::was_rebuilt_from_keys`].
    rebuilt_from_keys: bool,
    /// The password the wallet was opened with, for [`Wallet::is_password`].
    /// Wiped when the wallet is dropped.
    password: Zeroizing<String>,
    /// The lock that says this wallet is open: see [`mark_in_use`]. Let go
    /// when the wallet is dropped, after it has been closed.
    in_use: Option<std::fs::File>,
}

/// Marks the wallet at `path` as open, for as long as the file returned is
/// kept: a lock on a file of its own next to the wallet, which the system
/// lets go of when this process ends, however it ends.
///
/// Two programs on one wallet's files overwrite each other's view of the
/// chain, and each may spend what the other already has. The library
/// locks the keys file of a wallet it has opened, but not of one it has
/// just made, and a second window of the app is all it takes. So Veil
/// keeps a lock of its own, for every wallet it has open.
fn mark_in_use(path: &Path) -> Result<std::fs::File, Error> {
    let name = in_use_name(path);
    let failed = |error: std::io::Error| Error::NotMarked {
        file: name.clone(),
        reason: error.to_string(),
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&name)
        .map_err(failed)?;
    match lock_now(&file) {
        Ok(true) => Ok(file),
        Ok(false) => Err(Error::WalletInUse),
        Err(error) => Err(failed(error)),
    }
}

/// Takes the lock on `file` if nobody has it, without waiting. `false`
/// means that somebody has. The lock goes with the open file: it is let go
/// when the file is closed or its process ends.
///
/// The standard library has this as `File::try_lock`, but not for every
/// system: on Android it answers "not supported", which kept every wallet
/// there from being opened or made. The call it makes on Linux, `flock`,
/// exists on Android as well, so here it is made directly, on every
/// system that has it. That also means the lock an Android phone uses is
/// the one the tests on Linux exercise.
#[cfg(unix)]
fn lock_now(file: &std::fs::File) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    // SAFETY: the descriptor belongs to `file`, which is open for the
    // whole of the call, and `flock` touches nothing else.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

/// See the other `lock_now`. On Windows the standard library's lock is
/// there and is what the tests run as Windows programs exercise.
#[cfg(not(unix))]
fn lock_now(file: &std::fs::File) -> std::io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

/// The file whose lock says that the wallet at `path` is open.
fn in_use_name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

/// Lets go of a mark made for a wallet that then could not be made, and
/// takes its file away again: nothing is left where no wallet is.
fn unmark(path: &Path, mark: std::fs::File) {
    drop(mark);
    let _ = std::fs::remove_file(in_use_name(path));
}

/// Whether two texts are the same, taking as long for a near miss as for a
/// far one.
fn same_text(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .fold(0u8, |differs, (x, y)| differs | (x ^ y))
            == 0
}

/// What the library's refusal to build a transaction means. It says in
/// English that there is not enough money, in three wordings that all
/// begin the same way.
fn not_built(error: Error) -> Error {
    match error {
        Error::Wallet(message) if message.starts_with("not enough money") => {
            Error::NotEnoughFunds(message)
        }
        other => other,
    }
}

/// The address indices in the library's list of them, which it writes as
/// one set in braces per transaction: `{{1,4},{4}}`. Each index once, in
/// ascending order.
fn address_indices(list: &str) -> Vec<u32> {
    let mut indices: Vec<u32> = list
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|number| number.parse().ok())
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// A pointer the watching thread of [`Wallet::refresh_until`] may hold.
struct ListenerPtr(*mut c_void);

// SAFETY: the listener is a plain record that the library writes during a
// refresh and that is made to be read from another thread meanwhile; the
// thread holding this never outlives the refresh it watches.
unsafe impl Send for ListenerPtr {}

/// The wallet, for the one call the watching thread of
/// [`Wallet::refresh_until`] makes on it: asking the refresh to stop.
struct StopPtr(*mut c_void);

// SAFETY: stopping only sets flags the library reads between two batches of
// blocks, and is the library's own way to end a refresh from another
// thread; the thread holding this never outlives the refresh it watches.
unsafe impl Send for StopPtr {}

// SAFETY: the wallet library's objects are not tied to the thread that
// created them. `Wallet` is not `Sync`, so only one thread uses it at a time.
unsafe impl Send for Wallet {}

impl Wallet {
    /// Creates a new wallet and returns it with its recovery phrase.
    ///
    /// The phrase is a 16-word Polyseed. It is the only backup of the wallet
    /// and must be shown to the user.
    pub fn create(
        lib: &WalletLibrary,
        path: &Path,
        password: &str,
        network: Network,
    ) -> Result<(Wallet, SeedPhrase), Error> {
        let language = c_string(SEED_LANGUAGE, "seed language")?;
        // SAFETY: valid NUL-terminated argument; the result is a string the
        // library allocated for us.
        let words = unsafe {
            let raw = lib.api.MONERO_Wallet_createPolyseed(language.as_ptr());
            lib.take_secret(raw)
        };
        if words.is_empty() {
            return Err(Error::Wallet(
                "the wallet library did not produce a recovery phrase".into(),
            ));
        }
        let phrase = SeedPhrase::parse(&words)?;
        let wallet = Self::from_polyseed(lib, path, password, network, &phrase, true)?;
        Ok((wallet, phrase))
    }

    /// Restores a wallet from its recovery phrase.
    ///
    /// `restore_height` is where scanning starts for a 25-word phrase; zero
    /// scans the whole chain. A 16-word phrase carries its own creation date
    /// and ignores it.
    pub fn restore(
        lib: &WalletLibrary,
        path: &Path,
        password: &str,
        network: Network,
        phrase: &SeedPhrase,
        restore_height: u64,
    ) -> Result<Wallet, Error> {
        match phrase.kind() {
            SeedKind::Polyseed => Self::from_polyseed(lib, path, password, network, phrase, false),
            SeedKind::Legacy => {
                let (manager, c_path, mark) = Self::prepare_new(lib, path)?;
                let kept = Zeroizing::new(password.to_owned());
                let password = SecretCString::new(password, "password")?;
                let words = SecretCString::new(phrase.as_str(), "recovery phrase")?;
                let offset = c_string("", "seed offset")?;
                // SAFETY: every pointer is a valid NUL-terminated string that
                // outlives the call; `manager` comes from the library.
                let wallet = unsafe {
                    lib.api.MONERO_WalletManager_recoveryWallet(
                        manager,
                        c_path.as_ptr(),
                        password.as_ptr(),
                        words.as_ptr(),
                        network.id(),
                        restore_height,
                        KDF_ROUNDS,
                        offset.as_ptr(),
                    )
                };
                Self::made(
                    Self::adopt(lib, manager, wallet, network, path, kept),
                    path,
                    mark,
                )
            }
        }
    }

    /// Opens an existing wallet.
    ///
    /// A wallet is two files. The keys file holds the keys and is written
    /// once. The other, at `path`, holds what was learned from the chain
    /// and is rewritten on every save. If that second file can no longer
    /// be read (a power cut, a failing disk), the wallet is opened from the
    /// keys file alone and the unreadable file is kept next to it under
    /// another name. [`Wallet::was_rebuilt_from_keys`] then returns true:
    /// the funds are found again by the next synchronisation, which starts
    /// from the wallet's start height.
    ///
    /// Opening is where the library, left to itself, makes a "shared ring
    /// database" outside the wallet's folder. Veil's build of it does not:
    /// a wallet's files are those next to `path` and no others, however
    /// the wallet was made or opened. The README says what that gives up.
    pub fn open(
        lib: &WalletLibrary,
        path: &Path,
        password: &str,
        network: Network,
    ) -> Result<Wallet, Error> {
        // Marked as open before the library is asked for anything, if
        // there is a wallet to mark: a place without one gets no file of
        // ours, and the library says what is missing.
        let mut keys_file = path.as_os_str().to_owned();
        keys_file.push(".keys");
        let mark = if Path::new(&keys_file).is_file() {
            Some(mark_in_use(path)?)
        } else {
            None
        };
        let first = match Self::open_files(lib, path, password, network) {
            Ok(mut wallet) => {
                wallet.in_use = mark;
                return Ok(wallet);
            }
            Err(error) => error,
        };
        // A wrong password is by far the usual reason, and no reason to
        // touch any file.
        if matches!(&first, Error::Wallet(message) if message.contains("invalid password")) {
            return Err(Error::WrongPassword);
        }
        // Open in another wallet program, which does not know of Veil's
        // own mark: the library holds a lock on the keys file of a wallet
        // it has opened and says so here. That is no sign of a damaged
        // data file, and setting the file aside to find out would take it
        // from under a wallet that is running.
        if matches!(&first, Error::Wallet(message) if message.contains("opened by another wallet program"))
        {
            return Err(Error::WalletInUse);
        }
        if !path.is_file() || !Path::new(&keys_file).is_file() {
            return Err(first);
        }
        // Try without the file that changes. If the wallet opens then, that
        // file was the reason; if not, it goes back where it was and the
        // first error stands.
        let aside = set_aside_name(path);
        if std::fs::rename(path, &aside).is_err() {
            return Err(first);
        }
        match Self::open_files(lib, path, password, network) {
            Ok(mut wallet) => {
                wallet.rebuilt_from_keys = true;
                wallet.in_use = mark;
                Ok(wallet)
            }
            Err(_) => {
                let _ = std::fs::rename(&aside, path);
                Err(first)
            }
        }
    }

    /// Whether this wallet was opened from its keys alone because its other
    /// file could not be read. See [`Wallet::open`].
    ///
    /// The keys, the recovery phrase and the start height are as they were.
    /// The balance and the history are empty until the next
    /// synchronisation has scanned the chain again. Addresses that were
    /// handed out and never paid are forgotten and will be handed out
    /// again; payments to them are still found.
    pub fn was_rebuilt_from_keys(&self) -> bool {
        self.rebuilt_from_keys
    }

    fn open_files(
        lib: &WalletLibrary,
        path: &Path,
        password: &str,
        network: Network,
    ) -> Result<Wallet, Error> {
        let manager = Self::manager(lib)?;
        let c_path = c_string(path_text(path)?, "wallet path")?;
        let kept = Zeroizing::new(password.to_owned());
        let password = SecretCString::new(password, "password")?;
        // SAFETY: valid NUL-terminated strings; `manager` comes from the
        // library.
        let wallet = unsafe {
            lib.api.MONERO_WalletManager_openWallet(
                manager,
                c_path.as_ptr(),
                password.as_ptr(),
                network.id(),
            )
        };
        Self::adopt(lib, manager, wallet, network, path, kept)
    }

    fn from_polyseed(
        lib: &WalletLibrary,
        path: &Path,
        password: &str,
        network: Network,
        phrase: &SeedPhrase,
        new_wallet: bool,
    ) -> Result<Wallet, Error> {
        let (manager, c_path, mark) = Self::prepare_new(lib, path)?;
        let kept = Zeroizing::new(password.to_owned());
        let password = SecretCString::new(password, "password")?;
        let words = SecretCString::new(phrase.as_str(), "recovery phrase")?;
        let passphrase = c_string("", "seed passphrase")?;
        // SAFETY: every pointer is a valid NUL-terminated string that
        // outlives the call; `manager` comes from the library.
        let wallet = unsafe {
            lib.api.MONERO_WalletManager_createWalletFromPolyseed(
                manager,
                c_path.as_ptr(),
                password.as_ptr(),
                network.id(),
                words.as_ptr(),
                passphrase.as_ptr(),
                new_wallet,
                0,
                KDF_ROUNDS,
            )
        };
        Self::made(
            Self::adopt(lib, manager, wallet, network, path, kept),
            path,
            mark,
        )
    }

    /// Gives a wallet that was just made the mark taken for it, or takes
    /// the mark away again if the wallet could not be made.
    fn made(
        wallet: Result<Wallet, Error>,
        path: &Path,
        mark: std::fs::File,
    ) -> Result<Wallet, Error> {
        match wallet {
            Ok(mut wallet) => {
                wallet.in_use = Some(mark);
                Ok(wallet)
            }
            Err(error) => {
                unmark(path, mark);
                Err(error)
            }
        }
    }

    fn manager(lib: &WalletLibrary) -> Result<*mut c_void, Error> {
        // SAFETY: no arguments; returns the library's wallet manager.
        let manager = unsafe { lib.api.MONERO_WalletManagerFactory_getWalletManager() };
        if manager.is_null() {
            return Err(Error::Wallet(
                "the wallet library has no wallet manager".into(),
            ));
        }
        Ok(manager)
    }

    /// Common start of creating a wallet file: refuses to overwrite one,
    /// and marks the place as in use for the wallet about to be made.
    fn prepare_new(
        lib: &WalletLibrary,
        path: &Path,
    ) -> Result<(*mut c_void, std::ffi::CString, std::fs::File), Error> {
        let manager = Self::manager(lib)?;
        let c_path = c_string(path_text(path)?, "wallet path")?;
        // Asked twice, of the system and of the library, and either answer
        // is enough to refuse. The library alone has been wrong: on Windows
        // it did not see a wallet in a folder whose name was not ASCII, and
        // creating a wallet there again would have replaced the first.
        let mut keys_file = path.as_os_str().to_owned();
        keys_file.push(".keys");
        if path.exists() || Path::new(&keys_file).exists() {
            return Err(Error::WalletExists(path.to_path_buf()));
        }
        // SAFETY: valid manager and NUL-terminated path.
        let exists = unsafe {
            lib.api
                .MONERO_WalletManager_walletExists(manager, c_path.as_ptr())
        };
        if exists {
            return Err(Error::WalletExists(path.to_path_buf()));
        }
        let mark = mark_in_use(path)?;
        Ok((manager, c_path, mark))
    }

    /// Turns a pointer returned by the wallet manager into a `Wallet`, or
    /// into the library's error.
    fn adopt(
        lib: &WalletLibrary,
        manager: *mut c_void,
        wallet: *mut c_void,
        network: Network,
        path: &Path,
        password: Zeroizing<String>,
    ) -> Result<Wallet, Error> {
        if wallet.is_null() {
            // SAFETY: valid manager; the result is a library-owned string.
            let message =
                unsafe { lib.take_string(lib.api.MONERO_WalletManager_errorString(manager)) };
            return Err(Error::Wallet(non_empty(
                message,
                "the wallet could not be opened",
            )));
        }
        let wallet = Wallet {
            lib: lib.clone(),
            manager,
            wallet,
            network,
            path: path.to_path_buf(),
            sync_node: None,
            broadcast_node: None,
            on_sync_node: true,
            listener: std::ptr::null_mut(),
            rebuilt_from_keys: false,
            password,
            in_use: None,
        };
        // The manager returns an object even when opening failed; the error
        // is on the object. Dropping `wallet` on the error path closes it.
        wallet.check()?;
        // The library has a thread that refreshes the wallet on a timer, and
        // building or sending a transaction switches it on. Veil decides
        // itself when a node is contacted and which one: left on, that
        // thread would synchronise through the broadcast node while a
        // transaction is being sent through it.
        // SAFETY: live wallet object.
        unsafe {
            wallet
                .lib
                .api
                .MONERO_Wallet_setBackgroundRefresh(wallet.wallet, false)
        };
        let mut wallet = wallet;
        // SAFETY: live wallet object. The library creates the listener and
        // attaches it to the wallet; it only records what happens.
        wallet.listener = unsafe { wallet.lib.api.MONERO_cw_getWalletListener(wallet.wallet) };
        Ok(wallet)
    }

    /// Returns the library's error for the last operation, if there is one.
    fn check(&self) -> Result<(), Error> {
        // SAFETY: `self.wallet` is a live wallet object.
        let status = unsafe { self.lib.api.MONERO_Wallet_status(self.wallet) };
        if status == sys::STATUS_OK {
            return Ok(());
        }
        // SAFETY: live wallet object; the result is a library-owned string.
        let message = unsafe {
            self.lib
                .take_string(self.lib.api.MONERO_Wallet_errorString(self.wallet))
        };
        if lost_connection(&message) {
            return Err(Error::NodeUnreachable);
        }
        Err(Error::Wallet(non_empty(
            message,
            "the wallet library reported an error",
        )))
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `password` is the wallet's password.
    ///
    /// It is compared with the one the wallet was opened with, which is
    /// kept in memory for this while the wallet is open; the wallet's
    /// keys are there as well. The keys file is not read: the library
    /// keeps it locked, and on Windows a locked file cannot be read at
    /// all. Nor is the library asked: the copy it keeps is empty for a
    /// wallet that was created or restored in this session.
    pub fn is_password(&self, password: &str) -> bool {
        same_text(self.password.as_bytes(), password.as_bytes())
    }

    /// Gives the wallet a new password. The caller has checked the old one
    /// with [`Wallet::is_password`].
    ///
    /// The library writes the keys file again under the new password, as a
    /// new file that then takes the old one's place, so a kill in the
    /// middle leaves a wallet that opens with one password or the other.
    /// The data file does not depend on the password.
    pub fn change_password(&mut self, new: &str) -> Result<(), Error> {
        let kept = Zeroizing::new(new.to_owned());
        let new = SecretCString::new(new, "password")?;
        // SAFETY: live wallet object and a valid string.
        let changed = unsafe {
            self.lib
                .api
                .MONERO_Wallet_setPassword(self.wallet, new.as_ptr())
        };
        if !changed {
            self.check()?;
            return Err(Error::Wallet("the password could not be changed".into()));
        }
        self.password = kept;
        Ok(())
    }

    /// A short text the wallet keeps under `key` in its data file,
    /// encrypted like the rest of it; empty if there is none. For what
    /// Veil has to remember about this wallet that is nobody else's
    /// business.
    pub fn note(&self, key: &str) -> Result<String, Error> {
        let key = c_string(key, "note key")?;
        // SAFETY: live wallet object and a valid string; the result is a
        // library-owned string.
        Ok(unsafe {
            self.lib.take_string(
                self.lib
                    .api
                    .MONERO_Wallet_getCacheAttribute(self.wallet, key.as_ptr()),
            )
        })
    }

    /// Sets the text kept under `key`; empty clears it. It reaches the
    /// disk with the next [`Wallet::save`], and is lost with the data file
    /// if the wallet ever has to be rebuilt from its keys.
    pub fn set_note(&mut self, key: &str, value: &str) -> Result<(), Error> {
        let key = c_string(key, "note key")?;
        let value = c_string(value, "note")?;
        // SAFETY: live wallet object and valid strings.
        let set = unsafe {
            self.lib
                .api
                .MONERO_Wallet_setCacheAttribute(self.wallet, key.as_ptr(), value.as_ptr())
        };
        if !set {
            self.check()?;
            return Err(Error::Wallet("the note could not be kept".into()));
        }
        Ok(())
    }

    /// The wallet's recovery phrase, in the format it was created with.
    pub fn seed_phrase(&self) -> Result<SeedPhrase, Error> {
        let empty = c_string("", "seed passphrase")?;
        // SAFETY: live wallet object and valid strings; results are
        // library-owned strings.
        let (polyseed, legacy) = unsafe {
            let polyseed = self.lib.take_secret(
                self.lib
                    .api
                    .MONERO_Wallet_getPolyseed(self.wallet, empty.as_ptr()),
            );
            let legacy = self
                .lib
                .take_secret(self.lib.api.MONERO_Wallet_seed(self.wallet, empty.as_ptr()));
            (polyseed, legacy)
        };
        if !polyseed.trim().is_empty() {
            return SeedPhrase::parse(&polyseed);
        }
        if !legacy.trim().is_empty() {
            return SeedPhrase::parse(&legacy);
        }
        Err(Error::Wallet(
            "this wallet has no recovery phrase".to_string(),
        ))
    }

    /// The wallet's keys as a classic 25-word Monero phrase.
    ///
    /// Every Monero wallet can restore from this format, including ones that
    /// do not know Polyseed. Unlike the 16-word phrase it does not carry the
    /// creation date, so restoring from it needs a restore height.
    pub fn legacy_seed_phrase(&self) -> Result<SeedPhrase, Error> {
        let empty = c_string("", "seed offset")?;
        // SAFETY: live wallet object and valid string; the result is a
        // library-owned string.
        let legacy = unsafe {
            self.lib
                .take_secret(self.lib.api.MONERO_Wallet_seed(self.wallet, empty.as_ptr()))
        };
        self.check()?;
        SeedPhrase::parse(&legacy)
    }

    /// The wallet's primary address. Veil never shows it; tests use it to
    /// fund a private chain, whose daemon cannot credit a subaddress.
    #[cfg(feature = "regtest")]
    pub fn primary_address(&self) -> Result<String, Error> {
        self.address_at(0).map(|a| a.address)
    }

    /// Whether `address` is a valid address on this wallet's network.
    pub fn is_valid_address(&self, address: &str) -> bool {
        let Ok(address) = c_string(address.trim(), "address") else {
            return false;
        };
        // SAFETY: valid NUL-terminated string.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_addressValid(address.as_ptr(), self.network.id())
        }
    }

    /// Points the wallet at the node it synchronises with. Does not contact
    /// it yet.
    pub fn connect(&mut self, node: &Node) -> Result<(), Error> {
        self.point_at(node)?;
        self.sync_node = Some(node.clone());
        self.on_sync_node = true;
        Ok(())
    }

    /// Chooses the node transactions are broadcast through. Does not contact
    /// it.
    ///
    /// With a broadcast node set, [`PendingSend::commit`] points the wallet
    /// at it for the one request that hands over the transaction and then
    /// back at the sync node. The node that sees the wallet synchronise then
    /// never sees it spend. `None` broadcasts through the sync node.
    pub fn set_broadcast_node(&mut self, node: Option<Node>) -> Result<(), Error> {
        if let Some(node) = &node {
            node_addresses(node)?;
        }
        self.broadcast_node = node;
        Ok(())
    }

    /// Makes the wallet accept a private test chain, whichever way its node
    /// is reached. [`Node::LocalRegtest`] does this by itself; a test that
    /// reaches a regtest daemon through a stand-in proxy calls this.
    #[cfg(feature = "regtest")]
    pub fn accept_regtest_chain(&mut self) {
        // SAFETY: live wallet object.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_setAllowMismatchedDaemonVersion(self.wallet, true)
        };
    }

    /// Points the wallet library at `node`.
    fn point_at(&mut self, node: &Node) -> Result<(), Error> {
        let (daemon, proxy) = node_addresses(node)?;

        // When a node is set, the wallet library moves the scan start of a
        // wallet it considers new to the node's current height. It considers
        // a wallet new whenever nothing has been scanned yet, which is also
        // true of a restored wallet that was closed before its first sync,
        // and of a new wallet whose address was paid before it first went
        // online. Both would then skip the blocks that hold their funds.
        // Marking the wallet as recovering keeps the stored start height: the
        // creation date for a new wallet, the phrase's date or the chosen
        // height for a restored one.
        // SAFETY: live wallet object.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_setRecoveringFromSeed(self.wallet, true)
        };

        let daemon = c_string(&daemon, "node address")?;
        let proxy = c_string(&proxy, "proxy address")?;
        let empty = c_string("", "node login")?;
        // SAFETY: live wallet object and valid strings.
        let accepted = unsafe {
            self.lib.api.MONERO_Wallet_init(
                self.wallet,
                daemon.as_ptr(),
                0,
                empty.as_ptr(),
                empty.as_ptr(),
                // No TLS: an onion connection is already encrypted and
                // authenticated end to end, and the test daemon is local.
                false,
                false,
                proxy.as_ptr(),
            )
        };
        if !accepted {
            self.check()?;
            return Err(Error::Wallet("the node address was not accepted".into()));
        }
        // A remote node is never trusted: the wallet verifies what it is told
        // and does not ask the node anything that reveals its outputs.
        // SAFETY: live wallet object.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_setTrustedDaemon(self.wallet, false)
        };

        #[cfg(feature = "regtest")]
        if matches!(node, Node::LocalRegtest { .. }) {
            // A regtest chain activates every protocol version from its
            // first block, which a wallet on a public network rejects.
            // SAFETY: live wallet object.
            unsafe {
                self.lib
                    .api
                    .MONERO_Wallet_setAllowMismatchedDaemonVersion(self.wallet, true)
            };
        }
        Ok(())
    }

    /// The height scanning starts from. A new wallet starts about a month
    /// before its creation, to be safe against clock and block-time drift.
    pub fn restore_height(&self) -> u64 {
        // SAFETY: live wallet object.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_getRefreshFromBlockHeight(self.wallet)
        }
    }

    /// Changes where scanning starts, until the wallet is closed.
    ///
    /// The change is not written to disk: the library keeps this height in
    /// the keys file, which it writes once. A wallet opened again starts
    /// from the height it was created or restored with. A start height
    /// that has to last is given to [`Wallet::restore`].
    pub fn set_restore_height(&mut self, height: u64) {
        // SAFETY: live wallet object.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_setRefreshFromBlockHeight(self.wallet, height)
        }
    }

    /// Downloads and scans new blocks. Blocks until the wallet has caught up
    /// with the node or an error occurs.
    ///
    /// Fails if the node cannot be reached or is itself still synchronising.
    /// The wallet library would skip the scan in both cases and report
    /// success, which would show a stale balance as if it were current.
    ///
    /// A connection lost part way fails with [`Error::NodeUnreachable`] as
    /// well. The blocks scanned until then stay scanned, and the next
    /// refresh, through this node or another, continues after them.
    pub fn refresh(&mut self) -> Result<SyncProgress, Error> {
        self.refresh_with_progress(&mut |_| {})
    }

    /// Like [`Wallet::refresh`], and calls `report` about once a second
    /// with how far scanning has come, from the first block of this
    /// refresh on. A report therefore also says that this refresh has
    /// scanned something.
    ///
    /// `report` runs on another thread while this one waits for the
    /// library. The first sync of a wallet can take a long time over Tor;
    /// this is what lets an interface show that it is moving.
    pub fn refresh_with_progress(
        &mut self,
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, Error> {
        self.refresh_until(&|| false, report)
    }

    /// Like [`Wallet::refresh_with_progress`], and gives up as soon as
    /// `stop` says so, with [`Error::SyncStopped`].
    ///
    /// `stop` is asked about twenty times a second, on another thread. Once
    /// it answers yes, the library finishes the batch of blocks it is
    /// working on, which it cannot leave half done, and scans no further.
    /// What was scanned stays in the open wallet; [`Wallet::save`] writes
    /// it to disk, and the next refresh continues after it. If that batch
    /// was the last one, the refresh has finished and says so; if the
    /// refresh failed in the meantime, it fails with that error as it
    /// would have without the stop.
    ///
    /// A stop does not cut short a request the node has not answered yet.
    /// The refresh ends when the answer comes, or when the library gives up
    /// waiting for it, which takes minutes.
    pub fn refresh_until(
        &mut self,
        stop: &(dyn Fn() -> bool + Sync),
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, Error> {
        const REPORT_EVERY: Duration = Duration::from_secs(1);
        const CHECK_EVERY: Duration = Duration::from_millis(50);

        if stop() {
            return Err(Error::SyncStopped);
        }
        let node_height = self.node_height()?;

        let before = self.sync_progress().wallet_height;
        let finished = AtomicBool::new(false);
        let stopped = AtomicBool::new(false);
        let listener = ListenerPtr(self.listener);
        // The library's record still shows the last block of the refresh
        // before this one. Nothing is reported until it moves.
        let scanned_earlier = if self.listener.is_null() {
            0
        } else {
            // SAFETY: the listener lives as long as the wallet.
            unsafe { self.lib.api.MONERO_cw_WalletListener_height(self.listener) }
        };
        let to_stop = StopPtr(self.wallet);
        let api = &self.lib.api;
        let wallet = self.wallet;
        let refreshed = std::thread::scope(|scope| {
            let finished = &finished;
            let stopped = &stopped;
            scope.spawn(move || {
                let listener = listener;
                let to_stop = to_stop;
                let mut waited = Duration::ZERO;
                while !finished.load(Ordering::SeqCst) {
                    std::thread::sleep(CHECK_EVERY);
                    if stop() {
                        stopped.store(true, Ordering::SeqCst);
                        // Asked again on every round: the library forgets a
                        // stop that reaches it before its loop has started.
                        // SAFETY: the wallet outlives this scope; see
                        // `StopPtr`.
                        unsafe { api.MONERO_Wallet_stop(to_stop.0) };
                        continue;
                    }
                    waited += CHECK_EVERY;
                    if waited < REPORT_EVERY || listener.0.is_null() {
                        continue;
                    }
                    waited = Duration::ZERO;
                    // SAFETY: the listener lives as long as the wallet,
                    // which outlives this scope.
                    let scanned = unsafe { api.MONERO_cw_WalletListener_height(listener.0) };
                    // Unchanged until the first block of this refresh
                    // arrives.
                    if scanned > 0 && scanned != scanned_earlier {
                        report(SyncProgress {
                            wallet_height: scanned.min(node_height),
                            node_height,
                        });
                    }
                }
            });
            // SAFETY: live wallet object.
            let refreshed = unsafe { api.MONERO_Wallet_refresh(wallet) };
            finished.store(true, Ordering::SeqCst);
            refreshed
        });
        // A failure is reported as what it is, stopped or not: the caller
        // has to know that this node did not carry the refresh.
        self.check()?;
        if !refreshed {
            return Err(Error::Wallet("synchronisation failed".into()));
        }
        let progress = self.sync_progress();
        if progress.blocks_left() > 0 {
            if stopped.load(Ordering::SeqCst) {
                return Err(Error::SyncStopped);
            }
            // Success with blocks still missing and none scanned means the
            // library skipped the scan. Never report that as up to date.
            if progress.wallet_height <= before {
                return Err(Error::Wallet(
                    "synchronisation made no progress".to_string(),
                ));
            }
        }
        Ok(progress)
    }

    /// Asks the sync node for the height of its chain. Scans nothing.
    ///
    /// Fails if the node cannot be reached, runs an incompatible version, or
    /// is itself still synchronising.
    pub fn node_height(&mut self) -> Result<u64, Error> {
        self.return_to_sync_node()?;
        // SAFETY: live wallet object.
        let connection = unsafe { self.lib.api.MONERO_Wallet_connected(self.wallet) };
        match connection {
            sys::CONNECTION_CONNECTED => {}
            sys::CONNECTION_DISCONNECTED => return Err(Error::NodeUnreachable),
            _ => return Err(Error::NodeIncompatible),
        }
        // SAFETY: live wallet object.
        let (height, target) = unsafe {
            (
                self.lib
                    .api
                    .MONERO_Wallet_daemonBlockChainHeight(self.wallet),
                self.lib
                    .api
                    .MONERO_Wallet_daemonBlockChainTargetHeight(self.wallet),
            )
        };
        self.check()?;
        // A height of one is a chain with nothing but its first block, which
        // the library also treats as not ready.
        if height < target || height <= 1 {
            return Err(Error::NodeSyncing { height, target });
        }
        Ok(height)
    }

    /// Points the library at the first of `nodes` that answers when asked
    /// whether it is there, and returns its position; `None` if none
    /// does. Nothing of the wallet is said to any of them.
    ///
    /// The wallet is left off its sync node, on the node that answered or
    /// on the last one asked: the caller returns it with
    /// [`Wallet::return_to_sync_node`].
    fn point_at_first_that_answers(&mut self, nodes: &[Node]) -> Result<Option<usize>, Error> {
        self.on_sync_node = false;
        for (position, node) in nodes.iter().enumerate() {
            self.point_at(node)?;
            // SAFETY: live wallet object.
            let connection = unsafe { self.lib.api.MONERO_Wallet_connected(self.wallet) };
            if connection == sys::CONNECTION_CONNECTED {
                return Ok(Some(position));
            }
        }
        Ok(None)
    }

    /// The position of the first of `nodes` that answers when asked
    /// whether it is there; `None` if none does. The wallet is pointed
    /// back at its sync node afterwards.
    ///
    /// This is the question [`PendingSend::commit_through`] asks before it
    /// hands a transaction over, on its own for a caller that wants to
    /// know beforehand.
    pub fn first_that_answers(&mut self, nodes: &[Node]) -> Result<Option<usize>, Error> {
        for node in nodes {
            node_addresses(node)?;
        }
        let found = self.point_at_first_that_answers(nodes);
        let back = self.return_to_sync_node();
        let found = found?;
        back?;
        Ok(found)
    }

    /// Makes sure the library is pointed at the sync node before anything
    /// but a broadcast contacts a node.
    fn return_to_sync_node(&mut self) -> Result<(), Error> {
        if self.on_sync_node {
            return Ok(());
        }
        let node = self.sync_node.clone().ok_or(Error::NodeUnreachable)?;
        self.point_at(&node)?;
        self.on_sync_node = true;
        Ok(())
    }

    pub fn sync_progress(&self) -> SyncProgress {
        // SAFETY: live wallet object.
        unsafe {
            SyncProgress {
                wallet_height: self.lib.api.MONERO_Wallet_blockChainHeight(self.wallet),
                node_height: self
                    .lib
                    .api
                    .MONERO_Wallet_daemonBlockChainHeight(self.wallet),
            }
        }
    }

    pub fn balance(&self) -> Balance {
        // SAFETY: live wallet object.
        unsafe {
            Balance {
                total: Amount::from_atomic(
                    self.lib.api.MONERO_Wallet_balance(self.wallet, ACCOUNT),
                ),
                unlocked: Amount::from_atomic(
                    self.lib
                        .api
                        .MONERO_Wallet_unlockedBalance(self.wallet, ACCOUNT),
                ),
            }
        }
    }

    /// The address to show on the Receive screen.
    ///
    /// This is the most recently issued address, as long as nothing has been
    /// received on it. Once it has been paid, a new one is issued. The
    /// wallet's primary address is never returned.
    pub fn receive_address(&mut self) -> Result<ReceiveAddress, Error> {
        let highest_used = self.highest_used_address()?;
        let latest = self.address_count() - 1;
        if latest >= 1 && highest_used.is_none_or(|used| latest > used) {
            return self.address_at(latest);
        }
        self.issue_address()
    }

    /// Issues an additional address although the current one is unused, for
    /// a user who wants to give different payers different addresses.
    ///
    /// Refused once [`MAX_UNUSED_ADDRESSES`] addresses in a row are unused.
    pub fn new_receive_address(&mut self) -> Result<ReceiveAddress, Error> {
        let highest_used = self.highest_used_address()?.unwrap_or(0);
        let latest = self.address_count() - 1;
        let unused = latest.saturating_sub(highest_used);
        if unused >= MAX_UNUSED_ADDRESSES {
            return Err(Error::TooManyUnusedAddresses { unused });
        }
        self.issue_address()
    }

    /// Number of addresses in the account, counting the primary address at
    /// index zero. Always at least one.
    fn address_count(&self) -> u32 {
        // SAFETY: live wallet object.
        let count = unsafe {
            self.lib
                .api
                .MONERO_Wallet_numSubaddresses(self.wallet, ACCOUNT)
        };
        u32::try_from(count).unwrap_or(u32::MAX).max(1)
    }

    fn issue_address(&mut self) -> Result<ReceiveAddress, Error> {
        let label = c_string("", "address label")?;
        let index = self.address_count();
        // SAFETY: live wallet object and valid string.
        unsafe {
            self.lib
                .api
                .MONERO_Wallet_addSubaddress(self.wallet, ACCOUNT, label.as_ptr())
        };
        self.check()?;
        if self.address_count() != index + 1 {
            return Err(Error::Wallet("a new address could not be created".into()));
        }
        self.address_at(index)
    }

    fn address_at(&self, index: u32) -> Result<ReceiveAddress, Error> {
        // SAFETY: live wallet object; the result is a library-owned string.
        let address = unsafe {
            self.lib.take_string(self.lib.api.MONERO_Wallet_address(
                self.wallet,
                u64::from(ACCOUNT),
                u64::from(index),
            ))
        };
        if address.is_empty() {
            return Err(Error::Wallet("the address could not be read".into()));
        }
        Ok(ReceiveAddress { index, address })
    }

    /// The highest address index that has received anything, confirmed or
    /// not.
    fn highest_used_address(&mut self) -> Result<Option<u32>, Error> {
        let used: BTreeSet<u32> = self
            .history()?
            .into_iter()
            .filter(|t| t.direction == Direction::Incoming && !t.failed)
            .flat_map(|t| t.address_indices)
            .collect();
        Ok(used.into_iter().next_back())
    }

    /// The wallet's transfers, oldest first.
    pub fn history(&mut self) -> Result<Vec<Transfer>, Error> {
        let separator = c_string(LIST_SEPARATOR, "separator")?;
        // SAFETY: live wallet object. The history object and the entries it
        // hands out belong to the wallet and stay valid until the next
        // history refresh, which happens only here.
        unsafe {
            let api = &self.lib.api;
            let history = api.MONERO_Wallet_history(self.wallet);
            if history.is_null() {
                return Err(Error::Wallet("the history could not be read".into()));
            }
            api.MONERO_TransactionHistory_refresh(history);
            let count = api.MONERO_TransactionHistory_count(history).max(0);

            let mut transfers = Vec::with_capacity(count as usize);
            for position in 0..count {
                let info = api.MONERO_TransactionHistory_transaction(history, position);
                if info.is_null() {
                    continue;
                }
                let direction = if api.MONERO_TransactionInfo_direction(info) == sys::DIRECTION_IN {
                    Direction::Incoming
                } else {
                    Direction::Outgoing
                };
                let in_account = api.MONERO_TransactionInfo_subaddrAccount(info) == ACCOUNT;
                let indices = self
                    .lib
                    .take_string(api.MONERO_TransactionInfo_subaddrIndex(info, separator.as_ptr()));
                let address_indices = if direction == Direction::Incoming && in_account {
                    indices
                        .split(LIST_SEPARATOR)
                        .filter_map(|part| part.trim().parse::<u32>().ok())
                        .collect()
                } else {
                    Vec::new()
                };
                transfers.push(Transfer {
                    txid: self.lib.take_string(api.MONERO_TransactionInfo_hash(info)),
                    direction,
                    amount: Amount::from_atomic(api.MONERO_TransactionInfo_amount(info)),
                    // The library reports the fee of an incoming transfer
                    // too. The sender paid it, not this wallet.
                    fee: match direction {
                        Direction::Incoming => Amount::ZERO,
                        Direction::Outgoing => {
                            Amount::from_atomic(api.MONERO_TransactionInfo_fee(info))
                        }
                    },
                    block_height: api.MONERO_TransactionInfo_blockHeight(info),
                    confirmations: api.MONERO_TransactionInfo_confirmations(info),
                    timestamp: api.MONERO_TransactionInfo_timestamp(info),
                    pending: api.MONERO_TransactionInfo_isPending(info),
                    failed: api.MONERO_TransactionInfo_isFailed(info),
                    address_indices,
                });
            }
            transfers.sort_by_key(|t| (t.block_height == 0, t.block_height, t.timestamp));
            Ok(transfers)
        }
    }

    /// Builds and signs a transaction without sending it, so the user can
    /// review the fee first.
    pub fn prepare_send(&mut self, to: &str, amount: SendAmount) -> Result<PendingSend<'_>, Error> {
        let to = to.trim();
        if !self.is_valid_address(to) {
            return Err(Error::InvalidAddress);
        }
        self.return_to_sync_node()?;
        // The wallet library reads an amount of zero as "send everything".
        // That meaning is only ever used on purpose.
        let units = match amount {
            SendAmount::Exact(amount) if amount.is_zero() => return Err(Error::ZeroAmount),
            SendAmount::Exact(amount) => amount.as_atomic(),
            SendAmount::Everything => 0,
        };

        let address = c_string(to, "address")?;
        let empty = c_string("", "payment id")?;
        let separator = c_string(LIST_SEPARATOR, "separator")?;
        // SAFETY: live wallet object and valid strings.
        let pending = unsafe {
            self.lib.api.MONERO_Wallet_createTransaction(
                self.wallet,
                address.as_ptr(),
                empty.as_ptr(),
                units,
                RING_SIZE_AUTO,
                sys::PRIORITY_DEFAULT,
                ACCOUNT,
                empty.as_ptr(),
                separator.as_ptr(),
            )
        };
        if pending.is_null() {
            self.check().map_err(not_built)?;
            return Err(Error::Wallet("the transaction could not be built".into()));
        }

        // SAFETY: `pending` is a live pending-transaction object.
        unsafe {
            let api = &self.lib.api;
            if api.MONERO_PendingTransaction_status(pending) != sys::STATUS_OK {
                let message = self
                    .lib
                    .take_string(api.MONERO_PendingTransaction_errorString(pending));
                return Err(not_built(Error::Wallet(non_empty(
                    message,
                    "the transaction could not be built",
                ))));
            }
            // Read the ids now: the library forgets them once the
            // transaction has been broadcast.
            let ids = self
                .lib
                .take_string(api.MONERO_PendingTransaction_txid(pending, separator.as_ptr()));
            let txids: Vec<String> = ids
                .split(LIST_SEPARATOR)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect();
            if txids.is_empty() {
                return Err(Error::Wallet("the transaction has no id".into()));
            }
            // A wallet with very many small coins needs several
            // transactions for a large payment. The library hands them to
            // the node one by one and stops at the first that fails, so
            // such a payment can go out in part, and what is reported
            // then is true of one transaction only. Nothing was sent yet;
            // what was built is dropped.
            if txids.len() > 1 {
                return Err(Error::SeveralTransactions {
                    transactions: txids.len(),
                });
            }
            let sources = self.lib.take_string(
                api.MONERO_PendingTransaction_subaddrIndices(pending, separator.as_ptr()),
            );
            Ok(PendingSend {
                source_addresses: address_indices(&sources),
                amount: Amount::from_atomic(api.MONERO_PendingTransaction_amount(pending)),
                fee: Amount::from_atomic(api.MONERO_PendingTransaction_fee(pending)),
                txids,
                pending,
                wallet: self,
            })
        }
    }

    /// Writes the wallet to disk. The wallet library replaces the file
    /// atomically.
    pub fn save(&mut self) -> Result<(), Error> {
        let same_path = c_string("", "wallet path")?;
        // SAFETY: live wallet object; an empty path means "where it is".
        let stored = unsafe {
            self.lib
                .api
                .MONERO_Wallet_store(self.wallet, same_path.as_ptr())
        };
        if !stored {
            self.check()?;
            return Err(Error::Wallet("the wallet could not be saved".into()));
        }
        Ok(())
    }

    /// Saves and closes the wallet. Prefer this to dropping the wallet, which
    /// also saves but cannot report a failure.
    pub fn close(mut self) -> Result<(), Error> {
        self.save()?;
        self.release(false);
        Ok(())
    }

    fn release(&mut self, store: bool) {
        if self.wallet.is_null() {
            return;
        }
        // SAFETY: live manager and wallet; the wallet pointer is cleared so
        // it is never used again.
        unsafe {
            self.lib
                .api
                .MONERO_WalletManager_closeWallet(self.manager, self.wallet, store);
        }
        self.wallet = std::ptr::null_mut();
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        self.release(true);
    }
}

/// A signed transaction waiting for the user's confirmation.
///
/// Nothing has left the device yet. Dropping it abandons the transaction.
pub struct PendingSend<'w> {
    wallet: &'w mut Wallet,
    pending: *mut c_void,
    amount: Amount,
    fee: Amount,
    txids: Vec<String>,
    source_addresses: Vec<u32>,
}

impl PendingSend<'_> {
    /// What the recipient gets.
    pub fn amount(&self) -> Amount {
        self.amount
    }

    /// The network fee, paid on top of the amount.
    pub fn fee(&self) -> Amount {
        self.fee
    }

    /// What leaves the wallet: the amount and the fee. `None` if the two
    /// do not fit into one amount, which no real payment comes near.
    pub fn total(&self) -> Option<Amount> {
        self.amount.checked_add(self.fee)
    }

    /// The wallet's own addresses, by index, whose coins this payment
    /// spends, in ascending order. Index 0 is where change comes back to.
    ///
    /// The library chooses the coins and takes them from one address as
    /// long as that address holds enough. More than one index here means
    /// coins from different addresses are spent together, which can show
    /// whoever paid to those addresses that they belong to one wallet.
    pub fn source_addresses(&self) -> &[u32] {
        &self.source_addresses
    }

    /// The id of the transaction that carries the payment. Always one: a
    /// payment that would need several is refused by
    /// [`Wallet::prepare_send`].
    pub fn txids(&self) -> &[String] {
        &self.txids
    }

    /// Broadcasts the transaction and returns its transaction ids.
    ///
    /// With a broadcast node set, the transaction goes through that node and
    /// the wallet is pointed back at the sync node afterwards, whether the
    /// broadcast worked or not.
    pub fn commit(self) -> Result<Vec<String>, Error> {
        let PendingSend {
            wallet,
            pending,
            txids,
            ..
        } = self;
        let Some(node) = wallet.broadcast_node.clone() else {
            Self::broadcast(wallet, pending)?;
            return Ok(txids);
        };

        // From here until the wallet is back on the sync node, the only
        // request the library may make is the broadcast itself.
        wallet.on_sync_node = false;
        let sent = wallet
            .point_at(&node)
            .and_then(|()| Self::broadcast(wallet, pending));
        // A failure to switch back is not an error of this send: the
        // transaction may be on its way. The wallet stays marked as off the
        // sync node, and the next call that needs a node switches back or
        // reports why it cannot.
        let _ = wallet.return_to_sync_node();
        sent?;
        Ok(txids)
    }

    /// Broadcasts the transaction through the first of `nodes` that
    /// answers, and returns its transaction ids with the position of that
    /// node.
    ///
    /// Each node is first asked whether it is there, and the transaction
    /// is handed to the first that says so. A node that is down is passed
    /// over without having been given anything. If none answers, this
    /// fails with [`Error::NoNodeToSendThrough`]: the transaction has not
    /// left the device, which is a failure that can be said out loud and
    /// tried again. Once a node has answered and the transaction is on
    /// its way to it, a lost connection is
    /// [`Error::SendOutcomeUnknown`] as in [`PendingSend::commit`], and
    /// no further node is tried: a second hand-over could not be told
    /// from a second payment.
    ///
    /// The wallet is pointed back at the sync node afterwards, whatever
    /// the outcome.
    pub fn commit_through(self, nodes: &[Node]) -> Result<(Vec<String>, usize), Error> {
        let PendingSend {
            wallet,
            pending,
            txids,
            ..
        } = self;
        let sent = wallet
            .point_at_first_that_answers(nodes)
            .and_then(|found| match found {
                Some(position) => Self::broadcast(wallet, pending).map(|()| position),
                None => Err(Error::NoNodeToSendThrough { tried: nodes.len() }),
            });
        // As in `commit`: a failure to switch back is not this send's.
        let _ = wallet.return_to_sync_node();
        Ok((txids, sent?))
    }

    fn broadcast(wallet: &Wallet, pending: *mut c_void) -> Result<(), Error> {
        let lib = &wallet.lib;
        let empty = c_string("", "file name")?;
        // SAFETY: live pending-transaction object and valid string. An empty
        // file name means "broadcast" rather than "write to a file".
        unsafe {
            let api = &lib.api;
            let sent = api.MONERO_PendingTransaction_commit(pending, empty.as_ptr(), false);
            if !sent || api.MONERO_PendingTransaction_status(pending) != sys::STATUS_OK {
                let message = lib.take_string(api.MONERO_PendingTransaction_errorString(pending));
                // The library says the same whether the node never heard
                // the request or only its answer was lost. Treat both as
                // the worse case: the transaction may be on its way.
                if lost_connection(&message) {
                    return Err(Error::SendOutcomeUnknown);
                }
                return Err(Error::Wallet(non_empty(
                    message,
                    "the transaction could not be sent",
                )));
            }
        }
        Ok(())
    }
}

/// Where a wallet file that cannot be read is kept: next to the wallet,
/// under a name that is not taken.
fn set_aside_name(path: &Path) -> PathBuf {
    let numbered = |number: u32| {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".unreadable-{number}"));
        PathBuf::from(name)
    };
    (1..1000)
        .map(numbered)
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| numbered(1000))
}

fn path_text(path: &Path) -> Result<&str, Error> {
    path.to_str().ok_or(Error::InvalidArgument {
        what: "wallet path",
    })
}

/// Whether the library's error text says that a request to the node got no
/// usable answer: the connection could not be made, broke on the way, or
/// carried something that is not a node's reply.
fn lost_connection(message: &str) -> bool {
    message.contains("no connection to daemon")
}

fn non_empty(message: String, fallback: &str) -> String {
    if message.trim().is_empty() {
        fallback.to_owned()
    } else {
        message
    }
}

/// The node's address and the proxy to reach it through, as the wallet
/// library wants them. An empty proxy means a direct connection.
fn node_addresses(node: &Node) -> Result<(String, String), Error> {
    match node {
        Node::Onion {
            host,
            port,
            socks_port,
        } => {
            let host = host.trim().to_ascii_lowercase();
            if !is_onion_host(&host) {
                return Err(Error::NodeNotOnion);
            }
            Ok((
                format!("http://{host}:{port}"),
                format!("127.0.0.1:{socks_port}"),
            ))
        }
        #[cfg(feature = "regtest")]
        Node::LocalRegtest { port } => Ok((format!("http://127.0.0.1:{port}"), String::new())),
    }
}

/// True for a bare `.onion` hostname: no scheme, port, path, or user info.
pub fn is_onion_host(host: &str) -> bool {
    let Some(name) = host.strip_suffix(".onion") else {
        return false;
    };
    !name.is_empty()
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_indices_are_read_from_the_librarys_braces() {
        assert_eq!(address_indices(""), Vec::<u32>::new());
        assert_eq!(address_indices("{{3}}"), [3]);
        assert_eq!(address_indices("{{4,1},{4},{0}}"), [0, 1, 4]);
    }

    #[test]
    fn a_refusal_for_lack_of_money_is_told_apart_from_the_others() {
        for wording in [
            "not enough money to transfer, available only 1.0, sent amount 2.0",
            "not enough money to transfer, overall balance only 1.0, sent amount 2.0",
            "not enough money to transfer, available only 1.0, transaction amount 1.1 = 1.0 + 0.1 (fee)",
        ] {
            assert!(matches!(
                not_built(Error::Wallet(wording.into())),
                Error::NotEnoughFunds(_)
            ));
        }
        assert!(matches!(
            not_built(Error::Wallet(
                "not enough outputs for specified ring size".into()
            )),
            Error::Wallet(_)
        ));
        assert!(matches!(
            not_built(Error::NodeUnreachable),
            Error::NodeUnreachable
        ));
    }

    #[test]
    fn onion_host_must_be_a_bare_onion_name() {
        let v3 = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid";
        assert!(is_onion_host(&format!("{v3}.onion")));
        for host in [
            "",
            ".onion",
            "onion",
            "example.com",
            "node.onion.example.com",
            "127.0.0.1",
            "localhost",
            &format!("http://{v3}.onion"),
            &format!("{v3}.onion:18081"),
            &format!("{v3}.onion/path"),
            &format!("user@{v3}.onion"),
            &format!("{v3}.onion example.com"),
        ] {
            assert!(!is_onion_host(host), "{host:?} must be refused");
        }
    }

    #[test]
    fn sync_progress_counts_blocks_left() {
        let behind = SyncProgress {
            wallet_height: 90,
            node_height: 100,
        };
        assert_eq!(behind.blocks_left(), 10);
        let ahead = SyncProgress {
            wallet_height: 100,
            node_height: 90,
        };
        assert_eq!(ahead.blocks_left(), 0);
    }
}
