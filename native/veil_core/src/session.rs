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

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use veil_tor::{State, Status, TorConfig, TorService, Transport};
use veil_wallet::{
    Amount, Balance, Network, ReceiveAddress, SeedPhrase, SendAmount, SyncProgress, Transfer,
    Wallet, WalletLibrary,
};
use zeroize::Zeroizing;

use crate::{Core, CoreError, Nodes, OnionNode};

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
    /// The amount to send is zero.
    ZeroAmount,
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
            Error::NodeUnreachable | Error::NodeIncompatible | Error::NodeSyncing { .. } => {
                ProblemKind::NoNode
            }
            Error::InvalidAddress => ProblemKind::InvalidAddress,
            Error::ZeroAmount => ProblemKind::ZeroAmount,
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
    /// Running. `progress` appears once the first blocks have arrived.
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
    /// The transaction is being built.
    Preparing,
    /// Built and signed, waiting for the user. Nothing has left the device.
    Review {
        to: String,
        amount: Amount,
        fee: Amount,
    },
    /// Being handed to the node.
    Sending,
    /// Sent.
    Sent { txids: Vec<String> },
    /// The connection was lost while sending. The interface must not offer
    /// to send again before a synchronisation has finished.
    OutcomeUnknown,
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
    /// Why the last command did nothing, when the reason has no place of
    /// its own in the state: a request for one address too many, or a
    /// command that does not fit what the session is doing. Cleared by the
    /// next command.
    pub last_problem: Option<Problem>,
}

/// A wallet running on its own thread.
///
/// Dropping a session closes it: the wallet is saved and Tor stopped. That
/// waits for a synchronisation under way, which cannot be interrupted yet.
pub struct Session {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

enum Command {
    Sync,
    NewAddress,
    PrepareSend { to: String, amount: SendAmount },
    ConfirmSend,
    CancelSend,
    Phrase(Sender<Result<SeedPhrase, Problem>>),
    Close,
}

/// The state, and a way to wait for it to change.
struct Shared {
    state: Mutex<SessionState>,
    changed: Condvar,
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
}

/// What a session is given besides its configuration: how to get a
/// transport, and anything to do to the wallet once it is open. The app
/// starts Tor and does nothing to the wallet; tests put a stand-in in
/// Tor's place and make the wallet accept a private chain.
pub(crate) struct Hooks {
    pub transport: StartTransport,
    pub wallet: Box<dyn FnOnce(&mut Wallet) + Send>,
}

/// Starts whatever carries the wallet's traffic.
pub(crate) type StartTransport =
    Box<dyn FnOnce(TorConfig) -> Result<Box<dyn Transport>, Problem> + Send>;

/// How often an idle session looks at Tor and at its timer.
const TICK: Duration = Duration::from_millis(250);

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
                last_problem: None,
            }),
            changed: Condvar::new(),
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
            thread: Some(thread),
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
    /// outcome appears in [`SessionState::sync`].
    pub fn sync(&self) {
        self.send(Command::Sync);
    }

    /// Issues another receive address although the current one is unused.
    pub fn new_address(&self) {
        self.send(Command::NewAddress);
    }

    /// Builds a transaction for review. [`SessionState::send`] goes to
    /// `Review` with the fee, or to `Failed`. Nothing is sent.
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

    /// Drops the transaction under review, or clears the outcome of the
    /// last send from the state.
    pub fn cancel_send(&self) {
        self.send(Command::CancelSend);
    }

    /// The wallet's recovery phrase. Waits for the session's thread, so for
    /// the end of a synchronisation if one is running.
    pub fn recovery_phrase(&self) -> Result<SeedPhrase, Problem> {
        let (reply, answer) = mpsc::channel();
        self.send(Command::Phrase(reply));
        answer
            .recv()
            .unwrap_or_else(|_| Err(Problem::new(ProblemKind::NotNow, "the session has ended")))
    }

    /// Saves and closes the wallet and stops Tor. Returns the last state:
    /// its phase is `Closed`, or `Failed` with the reason.
    pub fn close(mut self) -> SessionState {
        self.shut_down();
        self.shared.get()
    }

    fn send(&self, command: Command) {
        // A session whose thread has ended takes no commands; its state
        // already says why.
        let _ = self.commands.send(command);
    }

    fn shut_down(&mut self) {
        let _ = self.commands.send(Command::Close);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shut_down();
    }
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
    let mut core = match open(&config, source, &password, hooks) {
        Ok(core) => core,
        Err(problem) => {
            shared.update(|state| state.phase = Phase::Failed(problem));
            return;
        }
    };
    drop(password);
    let rebuilt = core.was_rebuilt_from_keys();
    publish_wallet(&mut core, shared);
    shared.update(|state| {
        state.tor = core.tor_status();
        state.rebuilt_from_keys = rebuilt;
        state.phase = Phase::Ready;
    });

    let mut last_sync: Option<Instant> = None;
    loop {
        let command = inbox.recv_timeout(TICK);
        if matches!(
            command,
            Ok(Command::Sync | Command::NewAddress | Command::PrepareSend { .. })
                | Ok(Command::ConfirmSend | Command::CancelSend)
        ) {
            shared.update(|state| state.last_problem = None);
        }
        match command {
            Ok(Command::Close) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Sync) => {
                sync(&mut core, &config, shared);
                last_sync = Some(Instant::now());
            }
            Ok(Command::NewAddress) => {
                let issued = core.new_receive_address();
                shared.update(|state| match issued {
                    Ok(address) => state.receive_address = Some(address),
                    Err(error) => state.last_problem = Some(error.into()),
                });
            }
            Ok(Command::PrepareSend { to, amount }) => {
                if review(&mut core, &to, amount, shared, inbox) == AfterReview::Close {
                    break;
                }
                publish_wallet(&mut core, shared);
                save(&mut core, shared);
            }
            Ok(Command::ConfirmSend) => shared.update(|state| {
                state.last_problem = Some(Problem::new(
                    ProblemKind::NotNow,
                    "there is no transaction under review",
                ));
            }),
            Ok(Command::CancelSend) => shared.update(|state| state.send = SendState::Idle),
            Ok(Command::Phrase(reply)) => {
                let _ = reply.send(core.seed_phrase().map_err(Problem::from));
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let tor = core.tor_status();
        let ready = tor.state == State::Ready;
        shared.update(|state| state.tor = tor);
        if let Some(every) = config.sync_every {
            if ready && last_sync.is_none_or(|at| at.elapsed() >= every) {
                sync(&mut core, &config, shared);
                last_sync = Some(Instant::now());
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
fn open(
    config: &SessionConfig,
    source: WalletSource,
    password: &str,
    hooks: Hooks,
) -> Result<Core, Problem> {
    let lib = WalletLibrary::load(&config.wallet_library)?;
    let path = &config.wallet_path;
    let mut wallet = match source {
        WalletSource::Create => Wallet::create(&lib, path, password, config.network)?.0,
        WalletSource::Restore {
            phrase,
            restore_height,
        } => Wallet::restore(
            &lib,
            path,
            password,
            config.network,
            &phrase,
            restore_height,
        )?,
        WalletSource::Open => Wallet::open(&lib, path, password, config.network)?,
    };
    (hooks.wallet)(&mut wallet);
    let nodes = match config.nodes.as_slice() {
        [] => {
            return Err(Problem::new(
                ProblemKind::NoNode,
                "the session was given no node",
            ))
        }
        [only] => Nodes::single(only.clone()),
        [first, rest @ ..] => match rest.iter().find(|node| node.host() != first.host()) {
            Some(other) => Nodes::separate(first.clone(), other.clone())?,
            None => Nodes::single(first.clone()),
        },
    };
    let transport = (hooks.transport)(config.tor.clone())?;
    Ok(Core::with_transport(transport, wallet, nodes)?)
}

/// Copies what the wallet knows into the state.
fn publish_wallet(core: &mut Core, shared: &Shared) {
    let balance = core.balance();
    let address = core.receive_address();
    let history = core.history();
    shared.update(|state| {
        state.balance = balance;
        match address {
            Ok(address) => state.receive_address = Some(address),
            Err(error) => state.last_problem = Some(error.into()),
        }
        match history {
            Ok(history) => state.history = history,
            Err(error) => state.last_problem = Some(error.into()),
        }
    });
}

fn save(core: &mut Core, shared: &Shared) {
    if let Err(error) = core.save() {
        shared.update(|state| state.last_problem = Some(error.into()));
    }
}

fn sync(core: &mut Core, config: &SessionConfig, shared: &Shared) {
    shared.update(|state| state.sync = SyncState::Running { progress: None });
    let outcome = core.sync_with_any(&config.nodes, &mut |progress| {
        shared.update(|state| {
            state.sync = SyncState::Running {
                progress: Some(progress),
            }
        });
    });
    let node = core.nodes().sync().clone();
    let tor = core.tor_status();
    publish_wallet(core, shared);
    shared.update(|state| {
        state.tor = tor;
        state.syncs_ended += 1;
        match outcome {
            Ok(progress) => {
                state.node = Some(node);
                state.sync = SyncState::Done { progress };
            }
            Err(error) => state.sync = SyncState::Failed(error.into()),
        }
    });
    // What was scanned is kept even if the app is killed a moment later.
    save(core, shared);
}

#[derive(PartialEq, Eq)]
enum AfterReview {
    Continue,
    Close,
}

/// Builds a transaction and holds it until the user confirms or cancels.
/// While it is held, the session takes no other command but closing.
fn review(
    core: &mut Core,
    to: &str,
    amount: SendAmount,
    shared: &Shared,
    inbox: &Receiver<Command>,
) -> AfterReview {
    shared.update(|state| state.send = SendState::Preparing);
    let pending = match core.prepare_send(to, amount) {
        Ok(pending) => pending,
        Err(error) => {
            shared.update(|state| state.send = SendState::Failed(error.into()));
            return AfterReview::Continue;
        }
    };
    shared.update(|state| {
        state.send = SendState::Review {
            to: to.to_owned(),
            amount: pending.amount(),
            fee: pending.fee(),
        }
    });
    loop {
        match inbox.recv() {
            Ok(Command::ConfirmSend) => {
                shared.update(|state| state.send = SendState::Sending);
                let sent = pending.commit();
                shared.update(|state| {
                    state.send = match sent {
                        Ok(txids) => SendState::Sent { txids },
                        Err(veil_wallet::Error::SendOutcomeUnknown) => SendState::OutcomeUnknown,
                        Err(error) => SendState::Failed(error.into()),
                    }
                });
                return AfterReview::Continue;
            }
            Ok(Command::CancelSend) => {
                drop(pending);
                shared.update(|state| state.send = SendState::Idle);
                return AfterReview::Continue;
            }
            Ok(Command::Close) | Err(_) => return AfterReview::Close,
            Ok(Command::Phrase(reply)) => {
                let _ = reply.send(Err(Problem::new(
                    ProblemKind::NotNow,
                    "a transaction is under review",
                )));
            }
            Ok(Command::Sync | Command::NewAddress | Command::PrepareSend { .. }) => {
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
