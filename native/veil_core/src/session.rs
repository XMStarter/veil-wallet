//! The core on a thread of its own, for an app.
//!
//! A [`Core`] is one thread that waits for a node, sometimes for minutes. An
//! interface cannot wait with it. A [`Session`] runs the core on its own
//! thread: the interface sends it commands, which return at once, and reads
//! a picture of the state, [`SessionState`], whenever it wants to draw.
//!
//! The session adds no way around the core's rules. It opens the wallet,
//! starts Tor, and from then on does only what the core offers.
//!
//! What it decides itself is when things happen: it synchronises when told
//! to, and on a timer if [`SessionConfig::sync_every`] says so; it saves
//! after every synchronisation and every send; and it holds a send between
//! the review and the confirmation.
//!
//! A synchronisation can take an hour, and the session does not make
//! anything wait that long. Every few minutes it stops the synchronisation,
//! saves what was scanned, and goes on, so that a kill or a power cut costs
//! minutes. And any command but another request to synchronise stops it the
//! same way: the session saves, does what was asked, and continues the
//! synchronisation afterwards. Closing is such a command, which is what
//! lets a wallet be locked in the middle of a long scan. A send asked for
//! meanwhile is built once the synchronisation has ended, never from a
//! wallet that has seen only part of the chain.
//!
//! How soon a synchronisation stops is the library's doing: after the batch
//! of blocks it is working on, normally within seconds, but not before a
//! node that has gone silent answers or is given up on.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use veil_tor::{State, Status, TorConfig, TorService, Transport};
use veil_wallet::{
    Amount, Balance, Network, ReceiveAddress, SeedPhrase, SendAmount, SyncProgress, Transfer,
    Wallet, WalletLibrary,
};
use zeroize::Zeroizing;

use crate::{Core, CoreError, Nodes, OnionNode, SyncAttempt};

/// What a session needs to know before it starts.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// The Monero wallet library to load.
    pub wallet_library: PathBuf,
    /// The wallet's data file; its keys file lies next to it.
    pub wallet_path: PathBuf,
    pub network: Network,
    /// Where Tor keeps its files, and its bridges.
    pub tor: TorConfig,
    /// The nodes to use, in the order to try them. The first that carries a
    /// synchronisation to its end is kept; the one after it broadcasts.
    pub nodes: Vec<OnionNode>,
    /// Synchronise by itself: once Tor is ready, and again this long after
    /// the last attempt ended. `None` synchronises only when told to.
    pub sync_every: Option<Duration>,
}

/// Where the session's wallet comes from.
pub enum WalletSource {
    /// A new wallet. Its recovery phrase is read with
    /// [`Session::recovery_phrase`].
    Create,
    /// A wallet restored from its phrase. `restore_height` is where
    /// scanning starts for a 25-word phrase; a 16-word phrase carries its
    /// own date.
    Restore {
        phrase: SeedPhrase,
        restore_height: u64,
    },
    /// The wallet already at [`SessionConfig::wallet_path`].
    Open,
}

/// What went wrong, in a form an interface can put into its own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub kind: ProblemKind,
    /// The error as text, in English, for a log or a details view.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    /// The password does not open the wallet.
    WrongPassword,
    /// A wallet is already there; nothing was overwritten.
    WalletExists,
    /// The wallet is open in another window of the app or another
    /// program. Nothing was touched.
    WalletInUse,
    /// The recovery phrase is not one: a wrong number of words, or words
    /// that do not belong together.
    InvalidPhrase,
    /// Tor is still connecting. Nothing was sent anywhere.
    TorNotReady,
    /// Tor could not start.
    TorFailed,
    /// No node could be used: none answered, or the one in use stopped.
    NoNode,
    /// The destination is not an address of this network.
    InvalidAddress,
    /// The amount is not a number of coins.
    InvalidAmount,
    /// A node is not written as `host.onion:port`.
    InvalidNode,
    /// A bridge line is not one.
    InvalidBridge,
    /// The bridges need a transport that no installed program speaks.
    TransportMissing,
    /// The amount to send is zero.
    ZeroAmount,
    /// The wallet cannot pay the amount and its fee from what can be spent
    /// now.
    NotEnoughFunds,
    /// The payment does not fit into one transaction, and only such
    /// payments are sent. A smaller amount does.
    SeveralTransactions,
    /// Too many addresses were handed out without any being paid.
    TooManyUnusedAddresses,
    /// The connection was lost while a transaction was being sent. It may
    /// have gone out; the next synchronisation shows it if it did.
    SendOutcomeUnknown,
    /// The command does not fit what the session is doing.
    NotNow,
    /// The request could not be read at all: a mistake in the program that
    /// made it, not something a user did.
    BadRequest,
    /// Anything else. `detail` says what.
    Other,
}

impl Problem {
    fn new(kind: ProblemKind, detail: impl Into<String>) -> Problem {
        Problem {
            kind,
            detail: detail.into(),
        }
    }
}

impl From<veil_wallet::Error> for Problem {
    fn from(error: veil_wallet::Error) -> Problem {
        use veil_wallet::Error;
        let kind = match &error {
            Error::WrongPassword => ProblemKind::WrongPassword,
            Error::WalletExists(_) => ProblemKind::WalletExists,
            Error::WalletInUse => ProblemKind::WalletInUse,
            Error::SeedWordCount { .. } => ProblemKind::InvalidPhrase,
            Error::NodeUnreachable
            | Error::NodeIncompatible
            | Error::NodeSyncing { .. }
            | Error::NoNodeToSendThrough { .. } => ProblemKind::NoNode,
            Error::InvalidAddress => ProblemKind::InvalidAddress,
            Error::ZeroAmount => ProblemKind::ZeroAmount,
            Error::NotEnoughFunds(_) => ProblemKind::NotEnoughFunds,
            Error::SeveralTransactions { .. } => ProblemKind::SeveralTransactions,
            Error::TooManyUnusedAddresses { .. } => ProblemKind::TooManyUnusedAddresses,
            Error::SendOutcomeUnknown => ProblemKind::SendOutcomeUnknown,
            _ => ProblemKind::Other,
        };
        Problem::new(kind, error.to_string())
    }
}

impl From<CoreError> for Problem {
    fn from(error: CoreError) -> Problem {
        let kind = match &error {
            CoreError::TorNotReady { .. } => ProblemKind::TorNotReady,
            CoreError::TorFailed { .. } => ProblemKind::TorFailed,
            CoreError::NoNodeAnswered { .. } => ProblemKind::NoNode,
            CoreError::Wallet(_) | CoreError::InvalidNode | CoreError::SameNode => {
                ProblemKind::Other
            }
        };
        match error {
            CoreError::Wallet(wallet) => wallet.into(),
            other => Problem::new(kind, other.to_string()),
        }
    }
}

/// Where the session is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// The wallet is being opened and Tor started.
    Opening,
    /// The wallet is open. Commands are taken.
    Ready,
    /// The wallet was saved and closed.
    Closed,
    /// The wallet could not be opened, or could not be saved on closing.
    Failed(Problem),
}

/// The last synchronisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    /// None since the session started. The balance and the history are
    /// those of the last synchronisation before it.
    NotYet,
    /// Running. `progress` appears once the first blocks have arrived. It
    /// stays `Running` while the session saves along the way or does what a
    /// command asked for, and when the session is closed in the middle.
    Running { progress: Option<SyncProgress> },
    /// Finished: the wallet is level with the node.
    Done { progress: SyncProgress },
    /// Failed. What was scanned is kept; the balance shown is the last one
    /// that was complete.
    Failed(Problem),
}

/// A send, from the form to its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendState {
    /// No send is under way.
    Idle,
    /// The transaction is being built, or waits to be built for the
    /// synchronisation under way to end.
    Preparing,
    /// Built and signed, waiting for the user. Nothing has left the device.
    Review {
        to: String,
        amount: Amount,
        fee: Amount,
        /// The amount and the fee together: what leaves the wallet.
        total: Amount,
        /// The wallet's own addresses, by index, whose coins are spent.
        /// More than one means coins from different addresses are spent
        /// together, which can show that they belong to one wallet.
        source_addresses: Vec<u32>,
    },
    /// Being handed to the node.
    Sending,
    /// Sent.
    Sent { txids: Vec<String> },
    /// The connection was lost while sending, and the transaction may have
    /// gone out or not.
    ///
    /// The session looks for it after every synchronisation, in blocks and
    /// among the transactions the node is still holding, and turns this
    /// into `Sent` when it finds it. Until then it builds no other send. If
    /// a synchronisation that began a good while after the attempt still
    /// does not show it, `settled` becomes true: the transaction most
    /// likely never reached the node, and from then on the session takes a
    /// new send, or a cancel that clears this. It never says by itself
    /// that the send failed.
    OutcomeUnknown { txids: Vec<String>, settled: bool },
    /// Not sent.
    Failed(Problem),
}

/// What an interface draws. A copy; reading it never waits for a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    /// Goes up with every change, so an interface can tell whether it has
    /// to draw again.
    pub revision: u64,
    pub phase: Phase,
    pub tor: Status,
    /// The node synchronised with last.
    pub node: Option<OnionNode>,
    /// The user's own nodes, in the order to try them. Empty while the
    /// nodes the session was started with are used. Kept in the wallet's
    /// data file, encrypted with the rest: which node somebody uses is
    /// nobody else's business.
    pub own_nodes: Vec<OnionNode>,
    pub sync: SyncState,
    /// How many synchronisations have ended since the session started,
    /// whatever their outcome.
    pub syncs_ended: u64,
    /// Funds as of the last synchronisation that finished.
    pub balance: Balance,
    /// The address to show on the Receive screen.
    pub receive_address: Option<ReceiveAddress>,
    /// Transfers as of the last synchronisation, oldest first.
    pub history: Vec<Transfer>,
    pub send: SendState,
    /// The wallet's data file could not be read and the wallet was opened
    /// from its keys. Its funds show again after a synchronisation.
    pub rebuilt_from_keys: bool,
    /// Why the wallet could not be written to disk the last time that was
    /// tried; `None` once a save has worked again. While this is set,
    /// what the wallet has learned since its last good save is in memory
    /// only, a payment whose outcome is unknown included.
    pub not_saved: Option<Problem>,
    /// Why the last command did nothing, when the reason has no place of
    /// its own in the state: a request for one address too many, or a
    /// command that does not fit what the session is doing. Cleared by the
    /// next command.
    pub last_problem: Option<Problem>,
}

/// A wallet running on its own thread.
///
/// Dropping a session closes it: a synchronisation under way is stopped,
/// the wallet is saved and Tor stopped.
pub struct Session {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    /// Taken by whoever ends the session, which can be done through a
    /// shared reference: the C interface ends a session while a call that
    /// waits for its thread may still be under way on another thread.
    thread: Mutex<Option<JoinHandle<()>>>,
}

enum Command {
    Sync,
    NewAddress,
    PrepareSend {
        to: String,
        amount: SendAmount,
    },
    ConfirmSend,
    CancelSend,
    ClearSend,
    Phrase(Sender<Result<SeedPhrase, Problem>>),
    CheckPassword(Zeroizing<String>, Sender<Result<bool, Problem>>),
    ChangePassword {
        old: Zeroizing<String>,
        new: Zeroizing<String>,
        reply: Sender<Result<(), Problem>>,
    },
    SetNodes(Vec<OnionNode>),
    Close,
}

impl Command {
    /// Whether a synchronisation under way stops for this command. It does
    /// for all but the request for a synchronisation itself.
    fn interrupts(&self) -> bool {
        !matches!(self, Command::Sync)
    }
}

/// The state, and a way to wait for it to change.
struct Shared {
    state: Mutex<SessionState>,
    changed: Condvar,
    /// How many commands that interrupt a synchronisation were sent and
    /// not yet taken by the session's thread.
    waiting: AtomicUsize,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut SessionState)) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.clone();
        change(&mut state);
        if *state != before {
            state.revision += 1;
            self.changed.notify_all();
        }
    }

    fn get(&self) -> SessionState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Whether a command is waiting that a synchronisation has to stop for.
    fn is_waiting(&self) -> bool {
        self.waiting.load(Ordering::SeqCst) > 0
    }

    /// Counts a command as taken by the session's thread.
    fn taken(&self, command: &Command) {
        if command.interrupts() {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// What a session is given besides its configuration: how to get a
/// transport, anything to do to the wallet once it is open, and how often
/// to save during a synchronisation. The app starts Tor, does nothing to
/// the wallet, and saves every [`SAVE_EVERY`]; tests put a stand-in in
/// Tor's place, make the wallet accept a private chain, and save sooner.
pub(crate) struct Hooks {
    pub transport: StartTransport,
    pub wallet: Box<dyn FnOnce(&mut Wallet) + Send>,
    pub save_every: Duration,
    /// See [`SEND_SETTLES_AFTER`]; tests wait less.
    pub send_settles_after: Duration,
}

/// Starts whatever carries the wallet's traffic.
pub(crate) type StartTransport =
    Box<dyn FnOnce(TorConfig) -> Result<Box<dyn Transport>, Problem> + Send>;

/// How often an idle session looks at Tor and at its timer.
const TICK: Duration = Duration::from_millis(250);

/// How long a synchronisation scans before the session stops it to save
/// what was scanned. A wallet killed in the middle of a long scan loses no
/// more than this, plus the batch of blocks that was on its way.
pub(crate) const SAVE_EVERY: Duration = Duration::from_secs(120);

/// How long after a send whose outcome is unknown a synchronisation has to
/// begin for its silence to count. A transaction that reached a node shows
/// up at other nodes within seconds as a rule, and within a few minutes
/// when its first hops kept it to themselves, as Monero nodes do on
/// purpose for a short while.
pub(crate) const SEND_SETTLES_AFTER: Duration = Duration::from_secs(300);

impl Session {
    /// Opens the wallet and starts Tor on a new thread, and returns at
    /// once. [`SessionState::phase`] tells when the wallet is open, or why
    /// it is not.
    pub fn start(config: SessionConfig, source: WalletSource, password: &str) -> Session {
        let hooks = Hooks {
            transport: Box::new(|tor| match TorService::start(tor) {
                Ok(service) => Ok(Box::new(service) as Box<dyn Transport>),
                Err(error) => Err(Problem::new(ProblemKind::TorFailed, error.to_string())),
            }),
            wallet: Box::new(|_| {}),
            save_every: SAVE_EVERY,
            send_settles_after: SEND_SETTLES_AFTER,
        };
        Session::start_with(config, source, password, hooks)
    }

    pub(crate) fn start_with(
        config: SessionConfig,
        source: WalletSource,
        password: &str,
        hooks: Hooks,
    ) -> Session {
        let shared = Arc::new(Shared {
            state: Mutex::new(SessionState {
                revision: 0,
                phase: Phase::Opening,
                tor: Status {
                    state: State::Bootstrapping,
                    percent: 0,
                    message: None,
                },
                node: None,
                own_nodes: Vec::new(),
                sync: SyncState::NotYet,
                syncs_ended: 0,
                balance: Balance {
                    total: Amount::ZERO,
                    unlocked: Amount::ZERO,
                },
                receive_address: None,
                history: Vec::new(),
                send: SendState::Idle,
                rebuilt_from_keys: false,
                not_saved: None,
                last_problem: None,
            }),
            changed: Condvar::new(),
            waiting: AtomicUsize::new(0),
        });
        let (commands, inbox) = mpsc::channel();
        let password = Zeroizing::new(password.to_owned());
        let for_thread = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("veil-session".into())
            .spawn(move || run(config, source, password, hooks, &for_thread, &inbox))
            .expect("a thread for the session");
        Session {
            commands,
            shared,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// The state as it is now.
    pub fn state(&self) -> SessionState {
        self.shared.get()
    }

    /// Waits until the state satisfies `wanted`, and returns it; or returns
    /// `None` after `timeout`.
    pub fn wait_for(
        &self,
        timeout: Duration,
        wanted: impl Fn(&SessionState) -> bool,
    ) -> Option<SessionState> {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if wanted(&state) {
                return Some(state.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            state = self
                .shared
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Synchronises with the first node that carries it through. The
    /// outcome appears in [`SessionState::sync`]. Does nothing more if a
    /// synchronisation is already under way.
    pub fn sync(&self) {
        self.send(Command::Sync);
    }

    /// Issues another receive address although the current one is unused.
    pub fn new_address(&self) {
        self.send(Command::NewAddress);
    }

    /// Builds a transaction for review. [`SessionState::send`] goes to
    /// `Review` with the fee, or to `Failed`. Nothing is sent.
    ///
    /// A transaction is built only from a wallet that is level with a
    /// node: when the last synchronisation of this session finished. If
    /// one is under way, or the last one failed, or there was none yet,
    /// the send goes to `Preparing`, a synchronisation is made first, and
    /// the transaction is built when that has finished; if it fails, so
    /// does the send, with the same reason. An address that is not one
    /// and an amount of zero are refused at once, without a node. While
    /// the outcome of an earlier send is unknown and not settled, nothing
    /// is built: see [`SendState::OutcomeUnknown`].
    pub fn prepare_send(&self, to: &str, amount: SendAmount) {
        self.send(Command::PrepareSend {
            to: to.to_owned(),
            amount,
        });
    }

    /// Sends the transaction under review.
    pub fn confirm_send(&self) {
        self.send(Command::ConfirmSend);
    }

    /// Drops the transaction under review or the send waiting to be built,
    /// or takes a send that failed off the state. It never touches a send
    /// that went out or may have: a cancel that arrives when the
    /// transaction has just been sent leaves it shown as sent.
    pub fn cancel_send(&self) {
        self.send(Command::CancelSend);
    }

    /// Takes the outcome of the last send off the state, once the user has
    /// seen it: sent, failed, or unknown and settled. An outcome that is
    /// unknown and not settled stays.
    pub fn clear_send(&self) {
        self.send(Command::ClearSend);
    }

    /// The wallet's recovery phrase. Waits for the session's thread; a
    /// synchronisation under way stops for it and continues afterwards.
    pub fn recovery_phrase(&self) -> Result<SeedPhrase, Problem> {
        let (reply, answer) = mpsc::channel();
        self.send(Command::Phrase(reply));
        answer.recv().unwrap_or_else(|_| Err(session_ended()))
    }

    /// Whether `password` is the wallet's password: asked before the
    /// recovery phrase is shown again or the wallet is removed. Waits for
    /// the session's thread, like [`Session::recovery_phrase`].
    pub fn is_password(&self, password: &str) -> Result<bool, Problem> {
        let (reply, answer) = mpsc::channel();
        self.send(Command::CheckPassword(
            Zeroizing::new(password.to_owned()),
            reply,
        ));
        answer.recv().unwrap_or_else(|_| Err(session_ended()))
    }

    /// Gives the wallet the password `new`, if `old` is its password now;
    /// otherwise fails with [`ProblemKind::WrongPassword`] and changes
    /// nothing. Waits for the session's thread.
    pub fn change_password(&self, old: &str, new: &str) -> Result<(), Problem> {
        let (reply, answer) = mpsc::channel();
        self.send(Command::ChangePassword {
            old: Zeroizing::new(old.to_owned()),
            new: Zeroizing::new(new.to_owned()),
            reply,
        });
        answer.recv().unwrap_or_else(|_| Err(session_ended()))
    }

    /// Uses the user's own nodes from now on, in the order given, and
    /// keeps them in the wallet for the sessions to come; an empty list
    /// goes back to the nodes of [`SessionConfig::nodes`]. A
    /// synchronisation under way is dropped and a new one begins.
    /// [`SessionState::own_nodes`] shows what is in force.
    pub fn set_nodes(&self, nodes: Vec<OnionNode>) {
        self.send(Command::SetNodes(nodes));
    }

    /// Saves and closes the wallet and stops Tor. Returns the last state:
    /// its phase is `Closed`, or `Failed` with the reason. A
    /// synchronisation under way is stopped; what it scanned is saved, and
    /// the next session continues from there.
    pub fn close(self) -> SessionState {
        self.end()
    }

    /// What [`Session::close`] does, for a session that is shared. Calls
    /// that wait for the session's thread and are under way get their
    /// answer first; any made afterwards is told that the session has
    /// ended.
    pub(crate) fn end(&self) -> SessionState {
        self.shut_down();
        self.shared.get()
    }

    fn send(&self, command: Command) {
        // Counted before it is sent, so that the count is never behind.
        if command.interrupts() {
            self.shared.waiting.fetch_add(1, Ordering::SeqCst);
        }
        // A session whose thread has ended takes no commands; its state
        // already says why.
        let _ = self.commands.send(command);
    }

    fn shut_down(&self) {
        let thread = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        // Only the first to ask ends it; there is nothing left for a second.
        if let Some(thread) = thread {
            self.send(Command::Close);
            let _ = thread.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// What a call that waits for the session's thread gets when there is none.
fn session_ended() -> Problem {
    Problem::new(ProblemKind::NotNow, "the session has ended")
}

/// The session's thread, from opening the wallet to closing it.
fn run(
    config: SessionConfig,
    source: WalletSource,
    password: Zeroizing<String>,
    hooks: Hooks,
    shared: &Shared,
    inbox: &Receiver<Command>,
) {
    let save_every = hooks.save_every;
    let send_settles_after = hooks.send_settles_after;
    let mut core = match open(&config, source, &password, hooks) {
        Ok(core) => core,
        Err(problem) => {
            shared.update(|state| state.phase = Phase::Failed(problem));
            return;
        }
    };
    drop(password);
    // The user's own nodes, if the wallet has any on record, take the
    // place of the ones the session was started with.
    let mut nodes = config.nodes.clone();
    let own = own_nodes(&core);
    if !own.is_empty() {
        match nodes_for(&own).and_then(|chosen| Ok(core.set_nodes(chosen)?)) {
            Ok(()) => {
                nodes = own.clone();
                shared.update(|state| state.own_nodes = own);
            }
            Err(problem) => shared.update(|state| state.last_problem = Some(problem)),
        }
    }
    let rebuilt = core.was_rebuilt_from_keys();
    // A wallet opened from its keys alone has lost what its data file
    // held besides the chain, the user's own nodes among it. It does not
    // go to the public nodes by itself: somebody who had set a node of
    // their own gets to set it again first. A synchronisation that is
    // asked for, new nodes, or a send end the wait.
    let mut held = rebuilt;
    // Whether the last synchronisation of this session ended level with a
    // node. A transaction is built only then.
    let mut level = false;
    publish_wallet(&mut core, shared);
    // A send whose outcome is unknown, until it is found or cleared. One
    // left by an earlier session is taken up again.
    let mut unknown = UnknownSend::remembered(&core);
    // Said last, and together with everything the wallet brought with it:
    // whoever sees the wallet open sees it whole.
    shared.update(|state| {
        state.tor = core.tor_status();
        state.rebuilt_from_keys = rebuilt;
        if let Some(send) = &unknown {
            state.send = SendState::OutcomeUnknown {
                txids: send.txids.clone(),
                settled: false,
            };
        }
        state.phase = Phase::Ready;
    });

    let mut last_sync: Option<Instant> = None;
    // A synchronisation was asked for, or a command interrupted one, and it
    // has not ended yet.
    let mut sync_owed = false;
    // The synchronisation a command interrupted, to be continued.
    let mut interrupted: Option<SyncAttempt> = None;
    // A send asked for while a synchronisation was owed. It is built when
    // that has ended, so never from a wallet that knows half the chain.
    let mut waiting_send: Option<(String, SendAmount)> = None;
    // When the synchronisation under way, or the last one, began.
    let mut sync_began = Instant::now();
    // The node the last send of this session went through: the first to
    // be asked by the next one.
    let mut sent_through: Option<OnionNode> = None;
    loop {
        // With a synchronisation owed there is nothing to wait for: take
        // what has arrived and go on with it.
        let patience = if sync_owed { Duration::ZERO } else { TICK };
        let command = inbox.recv_timeout(patience);
        if let Ok(command) = &command {
            shared.taken(command);
        }
        if matches!(
            command,
            Ok(Command::Sync | Command::NewAddress | Command::PrepareSend { .. })
                | Ok(Command::ConfirmSend | Command::CancelSend | Command::ClearSend)
                | Ok(Command::SetNodes(_))
        ) {
            shared.update(|state| state.last_problem = None);
        }
        match command {
            Ok(Command::Close) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Sync) => {
                held = false;
                sync_owed = true;
            }
            Ok(Command::NewAddress) => {
                let issued = core.new_receive_address();
                shared.update(|state| match issued {
                    Ok(address) => state.receive_address = Some(address),
                    Err(error) => state.last_problem = Some(error.into()),
                });
            }
            Ok(Command::PrepareSend { to, amount }) => {
                if unknown.as_ref().is_some_and(|send| !send.settled) {
                    shared.update(|state| state.last_problem = Some(outcome_not_known()));
                    continue;
                }
                forget(&mut unknown, &mut core, shared);
                if let Some(problem) = not_a_send(&core, &to, amount) {
                    // Refused without a node, and so without waiting for
                    // a synchronisation either.
                    shared.update(|state| state.send = SendState::Failed(problem));
                } else if sync_owed || !level {
                    shared.update(|state| state.send = SendState::Preparing);
                    waiting_send = Some((to, amount));
                    held = false;
                    sync_owed = true;
                } else {
                    match review(
                        &mut core,
                        &to,
                        amount,
                        shared,
                        inbox,
                        &nodes,
                        &mut sent_through,
                    ) {
                        AfterReview::Close => break,
                        AfterReview::Continue => {}
                        AfterReview::Unknown(txids) => {
                            unknown = Some(UnknownSend::remember(txids, &mut core, shared));
                        }
                    }
                }
            }
            Ok(Command::ConfirmSend) => shared.update(|state| {
                state.last_problem = Some(Problem::new(
                    ProblemKind::NotNow,
                    "there is no transaction under review",
                ));
            }),
            Ok(Command::CancelSend) => {
                // Only what has gone nowhere can be cancelled. A cancel
                // may arrive when the transaction it was meant for has
                // just been sent: that stays on show as sent.
                waiting_send = None;
                shared.update(|state| {
                    if matches!(state.send, SendState::Preparing | SendState::Failed(_)) {
                        state.send = SendState::Idle;
                    } else if matches!(state.send, SendState::OutcomeUnknown { settled: false, .. })
                    {
                        state.last_problem = Some(outcome_not_known());
                    }
                });
            }
            Ok(Command::ClearSend) => {
                if unknown.as_ref().is_some_and(|send| !send.settled) {
                    shared.update(|state| state.last_problem = Some(outcome_not_known()));
                } else {
                    forget(&mut unknown, &mut core, shared);
                    shared.update(|state| {
                        if !matches!(state.send, SendState::Preparing) {
                            state.send = SendState::Idle;
                        }
                    });
                }
            }
            Ok(Command::Phrase(reply)) => {
                let _ = reply.send(core.seed_phrase().map_err(Problem::from));
            }
            Ok(Command::CheckPassword(password, reply)) => {
                let _ = reply.send(Ok(core.is_password(&password)));
            }
            Ok(Command::ChangePassword { old, new, reply }) => {
                let changed = if core.is_password(&old) {
                    core.change_password(&new).map_err(Problem::from)
                } else {
                    Err(Problem::new(
                        ProblemKind::WrongPassword,
                        "the old password is not the wallet's password",
                    ))
                };
                let _ = reply.send(changed);
            }
            Ok(Command::SetNodes(own)) => {
                let chosen = if own.is_empty() {
                    config.nodes.clone()
                } else {
                    own.clone()
                };
                let set = nodes_for(&chosen)
                    .and_then(|pair| Ok(core.set_nodes(pair)?))
                    .and_then(|()| Ok(core.set_note(OWN_NODES_NOTE, &node_list(&own))?));
                match set {
                    Ok(()) => {
                        nodes = chosen;
                        // The nodes in force and whether they reached the
                        // disk, in one change.
                        let not_saved = try_to_save(&mut core);
                        shared.update(|state| {
                            state.own_nodes = own;
                            state.not_saved = not_saved;
                        });
                        // Whatever was under way was with the old nodes.
                        interrupted = None;
                        level = false;
                        held = false;
                        sent_through = None;
                        sync_owed = true;
                    }
                    Err(problem) => shared.update(|state| state.last_problem = Some(problem)),
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let tor = core.tor_status();
        let ready = tor.state == State::Ready;
        shared.update(|state| state.tor = tor);
        if let Some(every) = config.sync_every {
            if !held && ready && last_sync.is_none_or(|at| at.elapsed() >= every) {
                sync_owed = true;
            }
        }
        // Commands that have arrived come first; the synchronisation would
        // only stop for them at once.
        if !sync_owed || shared.is_waiting() {
            continue;
        }
        if interrupted.is_none() {
            sync_began = Instant::now();
        }
        let ended = sync(
            &mut core,
            &nodes,
            shared,
            save_every,
            interrupted.take(),
            unknown
                .as_mut()
                .map(|send| (send, sync_began, send_settles_after)),
            waiting_send.is_some(),
        );
        level = match ended {
            Synced::Stopped(attempt) => {
                interrupted = Some(attempt);
                continue;
            }
            Synced::Level => true,
            Synced::Failed => false,
        };
        sync_owed = false;
        last_sync = Some(Instant::now());
        if unknown.as_ref().is_some_and(|send| send.found) {
            forget(&mut unknown, &mut core, shared);
        }
        // A send that waited for this synchronisation is built now, if the
        // wallet is level with a node. If not, the synchronisation has
        // said, with its own end, that the send failed and why.
        if let Some((to, amount)) = waiting_send.take() {
            if !level {
                continue;
            }
            match review(
                &mut core,
                &to,
                amount,
                shared,
                inbox,
                &nodes,
                &mut sent_through,
            ) {
                AfterReview::Close => break,
                AfterReview::Continue => {}
                AfterReview::Unknown(txids) => {
                    unknown = Some(UnknownSend::remember(txids, &mut core, shared));
                }
            }
        }
    }

    let closed = core.close();
    shared.update(|state| {
        state.phase = match closed {
            Ok(()) => Phase::Closed,
            Err(error) => Phase::Failed(error.into()),
        };
    });
}

/// Loads the library, opens the wallet, starts the transport, and joins
/// the two.
///
/// A wallet that exists is opened before anything is started, so that a
/// wrong password never starts Tor. A wallet that is made here is made
/// after the transport has started: were it the other way round and Tor
/// failed to start, the files of a wallet nobody has seen the recovery
/// phrase of would be left behind, and a second attempt would find a
/// wallet in its way.
fn open(
    config: &SessionConfig,
    source: WalletSource,
    password: &str,
    hooks: Hooks,
) -> Result<Core, Problem> {
    let lib = WalletLibrary::load(&config.wallet_library)?;
    let path = &config.wallet_path;
    let nodes = nodes_for(&config.nodes)?;
    let start = hooks.transport;
    let transport;
    let mut wallet = match source {
        WalletSource::Open => {
            let wallet = Wallet::open(&lib, path, password, config.network)?;
            transport = start(config.tor.clone())?;
            wallet
        }
        WalletSource::Create => {
            place_is_free(path)?;
            transport = start(config.tor.clone())?;
            Wallet::create(&lib, path, password, config.network)?.0
        }
        WalletSource::Restore {
            phrase,
            restore_height,
        } => {
            place_is_free(path)?;
            transport = start(config.tor.clone())?;
            Wallet::restore(
                &lib,
                path,
                password,
                config.network,
                &phrase,
                restore_height,
            )
            .map_err(|error| match error {
                // The library's own refusal of a phrase. Nothing else it
                // can object to is left: the place was checked to be free.
                veil_wallet::Error::Wallet(detail) => {
                    Problem::new(ProblemKind::InvalidPhrase, detail)
                }
                other => other.into(),
            })?
        }
    };
    (hooks.wallet)(&mut wallet);
    Ok(Core::with_transport(transport, wallet, nodes)?)
}

/// Fails if a wallet, or anything else, is where a new wallet is to be
/// made. The wallet layer checks the same again when it makes the wallet;
/// this is asked first so that a wallet in the way starts nothing.
fn place_is_free(path: &std::path::Path) -> Result<(), Problem> {
    let mut keys_file = path.as_os_str().to_owned();
    keys_file.push(".keys");
    if path.exists() || std::path::Path::new(&keys_file).exists() {
        return Err(veil_wallet::Error::WalletExists(path.to_path_buf()).into());
    }
    Ok(())
}

/// Why `to` and `amount` cannot be a send at all, if they cannot: found
/// out without a node.
fn not_a_send(core: &Core, to: &str, amount: SendAmount) -> Option<Problem> {
    if !core.is_valid_address(to) {
        return Some(veil_wallet::Error::InvalidAddress.into());
    }
    if matches!(amount, SendAmount::Exact(amount) if amount.is_zero()) {
        return Some(veil_wallet::Error::ZeroAmount.into());
    }
    None
}

/// Where the wallet keeps the user's own nodes: in its data file, as
/// `host.onion:port` separated by commas.
const OWN_NODES_NOTE: &str = "veil.nodes";

fn node_list(nodes: &[OnionNode]) -> String {
    nodes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// The user's own nodes as the wallet has them on record; none if there
/// is no record or it cannot be read as nodes.
fn own_nodes(core: &Core) -> Vec<OnionNode> {
    let Ok(note) = core.note(OWN_NODES_NOTE) else {
        return Vec::new();
    };
    note.split(',')
        .filter(|node| !node.is_empty())
        .map(str::parse)
        .collect::<Result<Vec<OnionNode>, _>>()
        .unwrap_or_default()
}

/// The nodes a core starts on, given the list to try in order: the first
/// synchronises, and the first after it that is another service broadcasts.
fn nodes_for(list: &[OnionNode]) -> Result<Nodes, Problem> {
    match list {
        [] => Err(Problem::new(
            ProblemKind::NoNode,
            "the session was given no node",
        )),
        [only] => Ok(Nodes::single(only.clone())),
        [first, rest @ ..] => match rest.iter().find(|node| node.host() != first.host()) {
            Some(other) => Ok(Nodes::separate(first.clone(), other.clone())?),
            None => Ok(Nodes::single(first.clone())),
        },
    }
}

/// What the wallet knows, read at one moment: to be put into the state
/// together with whatever else changed at that moment, in one change.
struct Seen {
    balance: Balance,
    address: Result<ReceiveAddress, CoreError>,
    history: Result<Vec<Transfer>, CoreError>,
}

impl Seen {
    fn of(core: &mut Core) -> Seen {
        Seen {
            balance: core.balance(),
            address: core.receive_address(),
            history: core.history(),
        }
    }

    fn show(self, state: &mut SessionState) {
        state.balance = self.balance;
        match self.address {
            Ok(address) => state.receive_address = Some(address),
            Err(error) => state.last_problem = Some(error.into()),
        }
        match self.history {
            Ok(history) => state.history = history,
            Err(error) => state.last_problem = Some(error.into()),
        }
    }
}

/// Copies what the wallet knows into the state.
fn publish_wallet(core: &mut Core, shared: &Shared) {
    let seen = Seen::of(core);
    shared.update(|state| seen.show(state));
}

/// Writes the wallet to disk, and says in the state whether that worked:
/// a wallet that cannot be saved looks like any other until the app is
/// closed, and then what it learned is gone.
fn save(core: &mut Core, shared: &Shared) {
    let not_saved = try_to_save(core);
    shared.update(|state| state.not_saved = not_saved);
}

/// Writes the wallet to disk and returns why it could not be, if it could
/// not: what [`SessionState::not_saved`] is to hold from then on.
fn try_to_save(core: &mut Core) -> Option<Problem> {
    core.save().err().map(Problem::from)
}

/// How a call of [`sync`] ended.
enum Synced {
    /// Stopped for a command; to be continued with this.
    Stopped(SyncAttempt),
    /// Finished: the wallet is level with a node.
    Level,
    /// Failed; the state says why.
    Failed,
}

/// Synchronises until the wallet is level with a node or the attempt has
/// failed, and puts the outcome into the state. Once blocks are arriving,
/// stops every `save_every` to save what was scanned, and goes on.
///
/// It also stops when a command arrives. Then it saves and returns the
/// attempt, to be passed in again as `interrupted` once the command is
/// dealt with; the state goes on showing the synchronisation as running.
///
/// `unknown` is a send whose outcome is unknown, with when this
/// synchronisation began and how long after the attempt its silence
/// counts. `send_waits` says that a send is waiting to be built after
/// this synchronisation; if the synchronisation fails, that send fails
/// with it. The end of the synchronisation, what the wallet knows after
/// it, and what it means for either send are reported in one change of
/// the state, so that nobody sees one without the others.
fn sync(
    core: &mut Core,
    candidates: &[OnionNode],
    shared: &Shared,
    save_every: Duration,
    interrupted: Option<SyncAttempt>,
    unknown: Option<(&mut UnknownSend, Instant, Duration)>,
    send_waits: bool,
) -> Synced {
    let mut attempt = interrupted.unwrap_or_else(|| {
        shared.update(|state| state.sync = SyncState::Running { progress: None });
        SyncAttempt::through(candidates)
    });
    let outcome = loop {
        // The clock for saving starts with the first blocks of this
        // stretch. Before them there is nothing to save, and a stretch
        // that has scanned something has moved on, however often it is
        // stopped.
        let scanning_since = OnceLock::new();
        let stop = || {
            shared.is_waiting()
                || scanning_since
                    .get()
                    .is_some_and(|since: &Instant| since.elapsed() >= save_every)
        };
        let outcome = core.continue_sync(&mut attempt, &stop, &mut |progress| {
            scanning_since.get_or_init(Instant::now);
            shared.update(|state| {
                state.sync = SyncState::Running {
                    progress: Some(progress),
                }
            });
        });
        if !matches!(
            outcome,
            Err(CoreError::Wallet(veil_wallet::Error::SyncStopped))
        ) {
            break outcome;
        }
        // Stopped on the way, for a command or because it is time to save.
        // Either way what was scanned goes to disk first.
        save(core, shared);
        if shared.is_waiting() {
            return Synced::Stopped(attempt);
        }
    };
    let node = core.nodes().sync().clone();
    let tor = core.tor_status();
    let seen = Seen::of(core);
    let finished = outcome.is_ok();
    shared.update(|state| {
        seen.show(state);
        state.tor = tor;
        state.syncs_ended += 1;
        match outcome {
            Ok(progress) => {
                state.node = Some(node);
                state.sync = SyncState::Done { progress };
            }
            Err(error) => {
                let problem = Problem::from(error);
                if send_waits {
                    state.send = SendState::Failed(problem.clone());
                }
                state.sync = SyncState::Failed(problem);
            }
        }
        if let Some((send, sync_began, settles_after)) = unknown {
            send.look(state, finished, sync_began, settles_after);
        }
    });
    // What was scanned is kept even if the app is killed a moment later.
    save(core, shared);
    if finished {
        Synced::Level
    } else {
        Synced::Failed
    }
}

/// A send whose outcome is unknown: the connection was lost while the
/// transaction was being handed to the node.
struct UnknownSend {
    txids: Vec<String>,
    since: Instant,
    /// See [`SendState::OutcomeUnknown`].
    settled: bool,
    /// The transaction has turned up in the history: it was sent.
    found: bool,
}

/// Where the wallet keeps a send whose outcome is unknown: in its data
/// file, encrypted with the rest, as the time of the attempt in seconds
/// since 1970 and the transaction ids.
const UNKNOWN_SEND_NOTE: &str = "veil.unknown_send";

impl UnknownSend {
    /// Takes note of a send whose outcome is unknown, in memory and in the
    /// wallet, and writes the wallet to disk at once: the app may be
    /// killed, or the power go, at any moment after a connection was lost,
    /// and the next session has to know that a payment may be on its way.
    /// If it cannot be written, [`SessionState::not_saved`] says so.
    fn remember(txids: Vec<String>, core: &mut Core, shared: &Shared) -> UnknownSend {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let note = format!("{now} {}", txids.join(","));
        match core.set_note(UNKNOWN_SEND_NOTE, &note) {
            Ok(()) => save(core, shared),
            Err(error) => shared.update(|state| state.not_saved = Some(error.into())),
        }
        UnknownSend {
            txids,
            since: Instant::now(),
            settled: false,
            found: false,
        }
    }

    /// The send an earlier session left in the wallet, if any. It counts
    /// as old as the clock says it is; if the clock makes no sense of it,
    /// as new.
    fn remembered(core: &Core) -> Option<UnknownSend> {
        let note = core.note(UNKNOWN_SEND_NOTE).ok()?;
        let (at, ids) = note.split_once(' ')?;
        let at: u64 = at.parse().ok()?;
        let txids: Vec<String> = ids
            .split(',')
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned)
            .collect();
        if txids.is_empty() {
            return None;
        }
        let age = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|now| now.as_secs().checked_sub(at))
            .map_or(Duration::ZERO, Duration::from_secs);
        let now = Instant::now();
        Some(UnknownSend {
            txids,
            since: now.checked_sub(age).unwrap_or(now),
            settled: false,
            found: false,
        })
    }

    /// Looks at the history a synchronisation has just left in `state`,
    /// and changes the send in `state` if that settles anything: to `Sent`
    /// if the transaction is there, to settled if a synchronisation that
    /// finished, and that began at `sync_began`, long enough after the
    /// attempt, does not show it.
    ///
    /// Called from inside the change that reports the end of that
    /// synchronisation, so that nobody sees the one without the other.
    fn look(
        &mut self,
        state: &mut SessionState,
        finished: bool,
        sync_began: Instant,
        settles_after: Duration,
    ) {
        self.found = state
            .history
            .iter()
            .any(|transfer| !transfer.failed && self.txids.contains(&transfer.txid));
        if self.found {
            state.send = SendState::Sent {
                txids: self.txids.clone(),
            };
            return;
        }
        // Only a synchronisation that finished, and that began long enough
        // after the attempt, says anything by not showing it.
        let late_enough = sync_began
            .checked_duration_since(self.since)
            .is_some_and(|waited| waited >= settles_after);
        if finished && late_enough && !self.settled {
            self.settled = true;
            state.send = SendState::OutcomeUnknown {
                txids: self.txids.clone(),
                settled: true,
            };
        }
    }
}

/// Lets go of a send whose outcome was unknown, in memory and in the
/// wallet, and saves the wallet. Does nothing if there is none.
fn forget(unknown: &mut Option<UnknownSend>, core: &mut Core, shared: &Shared) {
    if unknown.take().is_none() {
        return;
    }
    if let Err(error) = core.set_note(UNKNOWN_SEND_NOTE, "") {
        shared.update(|state| state.last_problem = Some(error.into()));
    }
    save(core, shared);
}

/// Why a send is neither built nor cleared while the last one's outcome is
/// unknown.
fn outcome_not_known() -> Problem {
    Problem::new(
        ProblemKind::NotNow,
        "the outcome of the last send is not known yet",
    )
}

/// Why nothing else is done while a transaction is held for review.
fn under_review() -> Problem {
    Problem::new(ProblemKind::NotNow, "a transaction is under review")
}

#[derive(PartialEq, Eq)]
enum AfterReview {
    /// The send is over, one way or another; the state says which.
    Continue,
    Close,
    /// The transaction with these ids may have gone out or not.
    Unknown(Vec<String>),
}

/// Builds a transaction and holds it until the user confirms or cancels.
/// While it is held, the session takes no other command but closing. Then
/// publishes what the wallet knows and saves it.
///
/// `candidates` are the session's nodes, and `sent_through` the node the
/// last send went through, replaced by the one this send goes through.
fn review(
    core: &mut Core,
    to: &str,
    amount: SendAmount,
    shared: &Shared,
    inbox: &Receiver<Command>,
    candidates: &[OnionNode],
    sent_through: &mut Option<OnionNode>,
) -> AfterReview {
    let after = hold_for_review(core, to, amount, shared, inbox, candidates, sent_through);
    if after != AfterReview::Close {
        publish_wallet(core, shared);
        save(core, shared);
    }
    after
}

fn hold_for_review(
    core: &mut Core,
    to: &str,
    amount: SendAmount,
    shared: &Shared,
    inbox: &Receiver<Command>,
    candidates: &[OnionNode],
    sent_through: &mut Option<OnionNode>,
) -> AfterReview {
    shared.update(|state| state.send = SendState::Preparing);
    // Where the transaction may go once it is confirmed, worked out now:
    // while it is held, the core is not to be asked anything. No node on
    // this list is contacted before the user has confirmed.
    let (route, behind): (Vec<OnionNode>, Vec<_>) = core
        .broadcast_route(candidates, sent_through.as_ref())
        .into_iter()
        .unzip();
    let pending = match core.prepare_send(to, amount) {
        Ok(pending) => pending,
        Err(error) => {
            shared.update(|state| state.send = SendState::Failed(error.into()));
            return AfterReview::Continue;
        }
    };
    let Some(total) = pending.total() else {
        shared.update(|state| {
            state.send = SendState::Failed(Problem::new(
                ProblemKind::Other,
                "the amount and the fee do not fit into one amount",
            ));
        });
        return AfterReview::Continue;
    };
    shared.update(|state| {
        state.send = SendState::Review {
            to: to.to_owned(),
            amount: pending.amount(),
            fee: pending.fee(),
            total,
            source_addresses: pending.source_addresses().to_vec(),
        }
    });
    loop {
        let command = inbox.recv();
        if let Ok(command) = &command {
            shared.taken(command);
        }
        match command {
            Ok(Command::ConfirmSend) => {
                shared.update(|state| state.send = SendState::Sending);
                // Kept for the case that the answer is lost: by these the
                // transaction is recognised if it went out after all.
                let ids = pending.txids().to_vec();
                // Handed to the first node that answers when asked whether
                // it is there. A node that is down costs a moment and
                // nothing else; if none is there, nothing was sent, and
                // that is a failure that can be called one.
                let sent = pending.commit_through(&behind).map(|(txids, position)| {
                    *sent_through = Some(route[position].clone());
                    txids
                });
                let unknown = matches!(sent, Err(veil_wallet::Error::SendOutcomeUnknown));
                // How it went and what it left in the wallet, in one
                // change: nobody sees "sent" over the balance of before.
                let seen = Seen::of(core);
                shared.update(|state| {
                    seen.show(state);
                    state.send = match sent {
                        Ok(txids) => SendState::Sent { txids },
                        Err(veil_wallet::Error::SendOutcomeUnknown) => SendState::OutcomeUnknown {
                            txids: ids.clone(),
                            settled: false,
                        },
                        Err(error) => SendState::Failed(error.into()),
                    }
                });
                return if unknown {
                    AfterReview::Unknown(ids)
                } else {
                    AfterReview::Continue
                };
            }
            Ok(Command::CancelSend) => {
                drop(pending);
                shared.update(|state| state.send = SendState::Idle);
                return AfterReview::Continue;
            }
            Ok(Command::Close) | Err(_) => return AfterReview::Close,
            Ok(Command::Phrase(reply)) => {
                let _ = reply.send(Err(under_review()));
            }
            Ok(Command::CheckPassword(_, reply)) => {
                let _ = reply.send(Err(under_review()));
            }
            Ok(Command::ChangePassword { reply, .. }) => {
                let _ = reply.send(Err(under_review()));
            }
            Ok(
                Command::Sync
                | Command::NewAddress
                | Command::PrepareSend { .. }
                | Command::ClearSend
                | Command::SetNodes(_),
            ) => {
                shared.update(|state| {
                    state.last_problem = Some(Problem::new(
                        ProblemKind::NotNow,
                        "a transaction is under review",
                    ));
                });
            }
        }
    }
}
