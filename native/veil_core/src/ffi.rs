//! C interface to the session, called from the app through FFI.
//!
//! The interface is a handful of functions, and what crosses it is text: a
//! JSON object to start a session, a JSON object per command, and the state
//! as a JSON object. The two secrets, the password and a recovery phrase,
//! travel as arguments of their own and never inside that text.
//!
//! Every function is safe to call from any thread. Reading the state and
//! giving a command never wait, neither for the network nor for a call
//! that does wait and is under way on another thread. Closing a session,
//! asking for the recovery phrase, and checking or changing the password
//! wait for the session's thread. A null handle is accepted everywhere.
//! Panics never cross this boundary.
//!
//! The format is described in the crate's README, and the state as it
//! crosses the interface is pinned by the files in `tests/fixtures`, which
//! the Dart side reads in its own tests.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Map, Value};
use veil_tor::{Bridges, State, TorConfig, TorService, Transport};
use veil_wallet::{Amount, Direction, Network, SeedPhrase, SendAmount};

use crate::session::Hooks;
use crate::{
    OnionNode, Phase, Problem, ProblemKind, SendState, Session, SessionConfig, SessionState,
    SyncState, WalletSource,
};

/// Opaque handle to a session.
pub struct VeilSession {
    /// `None` once the session has been closed through the handle. Shared,
    /// so that a call holds the lock only for as long as it takes to get
    /// hold of the session: one that waits for the session's thread must
    /// not keep another thread from reading the state meanwhile.
    session: Mutex<Option<Arc<Session>>>,
}

/// A command as it arrives in text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Sync,
    NewAddress,
    PrepareSend {
        to: String,
        amount: SendAmount,
    },
    ConfirmSend,
    CancelSend,
    ClearSend,
    /// The user's own nodes; none for the ones the session started with.
    SetNodes(Vec<OnionNode>),
}

fn bad_request(detail: impl Into<String>) -> Problem {
    Problem {
        kind: ProblemKind::BadRequest,
        detail: detail.into(),
    }
}

fn text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, Problem> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request(format!("`{field}` is missing or is not text")))
}

fn object(json: &str) -> Result<Map<String, Value>, Problem> {
    match serde_json::from_str(json) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(bad_request("the request is not a JSON object")),
        Err(error) => Err(bad_request(format!("the request is not JSON: {error}"))),
    }
}

/// Reads what a session is started with. `phrase` is the recovery phrase
/// for a restore.
pub(crate) fn parse_start(
    config: &str,
    phrase: Option<&str>,
) -> Result<(SessionConfig, WalletSource), Problem> {
    let config = object(config)?;
    // Until the release steps the app works on stagenet only.
    if text(&config, "network")? != "stagenet" {
        return Err(bad_request("the only network is `stagenet`"));
    }
    let tor = config
        .get("tor")
        .and_then(Value::as_object)
        .ok_or_else(|| bad_request("`tor` is missing or is not an object"))?;
    // Nodes named here, in their order. Without any, the public ones.
    let nodes = nodes_in(&config)?;
    let nodes = crate::stagenet_candidates(nodes);
    let sync_every = match config.get("sync_every_seconds") {
        None | Some(Value::Null) => None,
        Some(seconds) => Some(Duration::from_secs(
            seconds.as_u64().filter(|s| *s > 0).ok_or_else(|| {
                bad_request("`sync_every_seconds` is not a whole number above zero")
            })?,
        )),
    };
    let source = match text(&config, "source")? {
        "create" => WalletSource::Create,
        "open" => WalletSource::Open,
        "restore" => WalletSource::Restore {
            phrase: SeedPhrase::parse(
                phrase.ok_or_else(|| bad_request("a restore needs the recovery phrase"))?,
            )?,
            restore_height: match config.get("restore_height") {
                None | Some(Value::Null) => 0,
                Some(height) => height
                    .as_u64()
                    .ok_or_else(|| bad_request("`restore_height` is not a block height"))?,
            },
        },
        other => {
            return Err(bad_request(format!(
                "`{other}` is not a source of a wallet"
            )))
        }
    };
    let config = SessionConfig {
        wallet_library: PathBuf::from(text(&config, "wallet_library")?),
        wallet_path: PathBuf::from(text(&config, "wallet_path")?),
        network: Network::Stagenet,
        tor: TorConfig {
            state_dir: PathBuf::from(text(tor, "state_dir")?),
            cache_dir: PathBuf::from(text(tor, "cache_dir")?),
            bridges: bridges_in(&config)?,
        },
        nodes,
        sync_every,
    };
    Ok((config, source))
}

/// The nodes in the `nodes` of a request, which may be left out.
fn nodes_in(request: &Map<String, Value>) -> Result<Vec<OnionNode>, Problem> {
    let nodes = match request.get("nodes") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(nodes) => nodes
            .as_array()
            .ok_or_else(|| bad_request("`nodes` is not a list"))?,
    };
    nodes
        .iter()
        .map(|node| {
            node.as_str()
                .and_then(|node| node.trim().parse::<OnionNode>().ok())
                .ok_or_else(|| Problem {
                    kind: ProblemKind::InvalidNode,
                    detail: format!("{node} is not a node; a node is written as host.onion:port"),
                })
        })
        .collect()
}

/// The bridges in the `bridges` of a request, a list of bridge lines that
/// may be left out, with the programs their transports need.
fn bridges_in(request: &Map<String, Value>) -> Result<Bridges, Problem> {
    let lines = match request.get("bridges") {
        None | Some(Value::Null) => return Ok(Bridges::none()),
        Some(lines) => lines
            .as_array()
            .ok_or_else(|| bad_request("`bridges` is not a list"))?,
    };
    let lines = lines
        .iter()
        .map(|line| {
            line.as_str()
                .map(str::to_owned)
                .ok_or_else(|| bad_request("a bridge is not text"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bridges = Bridges {
        // Empty lines and comments go, as in a file of bridges.
        lines: Bridges::lines_from_text(&lines.join("\n")),
        transports: Vec::new(),
    };
    let needed = bridges.transports_needed().map_err(|error| Problem {
        kind: ProblemKind::InvalidBridge,
        detail: error.to_string(),
    })?;
    bridges.transports =
        crate::transports::programs_for(&needed, Vec::new()).map_err(|missing| Problem {
            kind: ProblemKind::TransportMissing,
            detail: missing.to_string(),
        })?;
    Ok(bridges)
}

/// Reads one command.
pub(crate) fn parse_command(command: &str) -> Result<Request, Problem> {
    let command = object(command)?;
    match text(&command, "command")? {
        "set_nodes" => Ok(Request::SetNodes(nodes_in(&command)?)),
        "sync" => Ok(Request::Sync),
        "new_address" => Ok(Request::NewAddress),
        "confirm_send" => Ok(Request::ConfirmSend),
        "cancel_send" => Ok(Request::CancelSend),
        "clear_send" => Ok(Request::ClearSend),
        "prepare_send" => {
            let amount = match text(&command, "amount")?.trim() {
                "all" => SendAmount::Everything,
                amount => SendAmount::Exact(amount.parse::<Amount>().map_err(|error| Problem {
                    kind: ProblemKind::InvalidAmount,
                    detail: error.to_string(),
                })?),
            };
            Ok(Request::PrepareSend {
                to: text(&command, "to")?.to_owned(),
                amount,
            })
        }
        other => Err(bad_request(format!("`{other}` is not a command"))),
    }
}

fn kind_name(kind: ProblemKind) -> &'static str {
    match kind {
        ProblemKind::WrongPassword => "wrong_password",
        ProblemKind::WalletExists => "wallet_exists",
        ProblemKind::WalletInUse => "wallet_in_use",
        ProblemKind::InvalidPhrase => "invalid_phrase",
        ProblemKind::TorNotReady => "tor_not_ready",
        ProblemKind::TorFailed => "tor_failed",
        ProblemKind::NoNode => "no_node",
        ProblemKind::InvalidAddress => "invalid_address",
        ProblemKind::InvalidAmount => "invalid_amount",
        ProblemKind::InvalidNode => "invalid_node",
        ProblemKind::InvalidBridge => "invalid_bridge",
        ProblemKind::TransportMissing => "transport_missing",
        ProblemKind::ZeroAmount => "zero_amount",
        ProblemKind::NotEnoughFunds => "not_enough_funds",
        ProblemKind::SeveralTransactions => "several_transactions",
        ProblemKind::TooManyUnusedAddresses => "too_many_unused_addresses",
        ProblemKind::SendOutcomeUnknown => "send_outcome_unknown",
        ProblemKind::NotNow => "not_now",
        ProblemKind::BadRequest => "bad_request",
        ProblemKind::Other => "other",
    }
}

pub(crate) fn problem_json(problem: &Problem) -> Value {
    json!({ "kind": kind_name(problem.kind), "detail": problem.detail })
}

/// The state as it crosses the interface. Amounts are text, exact to the
/// last digit, so that the other side never does arithmetic on them.
pub(crate) fn state_json(state: &SessionState) -> Value {
    let phase = match &state.phase {
        Phase::Opening => json!({ "name": "opening" }),
        Phase::Ready => json!({ "name": "ready" }),
        Phase::Closed => json!({ "name": "closed" }),
        Phase::Failed(problem) => json!({ "name": "failed", "problem": problem_json(problem) }),
    };
    let tor_state = match state.tor.state {
        State::Bootstrapping => "bootstrapping",
        State::Ready => "ready",
        State::Failed => "failed",
    };
    let sync = match &state.sync {
        SyncState::NotYet => json!({ "name": "not_yet" }),
        SyncState::Running { progress: None } => json!({ "name": "running" }),
        SyncState::Running {
            progress: Some(progress),
        } => json!({
            "name": "running",
            "wallet_height": progress.wallet_height,
            "node_height": progress.node_height,
        }),
        SyncState::Done { progress } => json!({
            "name": "done",
            "wallet_height": progress.wallet_height,
            "node_height": progress.node_height,
        }),
        SyncState::Failed(problem) => json!({ "name": "failed", "problem": problem_json(problem) }),
    };
    let send = match &state.send {
        SendState::Idle => json!({ "name": "idle" }),
        SendState::Preparing => json!({ "name": "preparing" }),
        SendState::Review {
            to,
            amount,
            fee,
            total,
            source_addresses,
        } => json!({
            "name": "review",
            "to": to,
            "amount": amount.to_string(),
            "fee": fee.to_string(),
            "total": total.to_string(),
            "source_addresses": source_addresses,
        }),
        SendState::Sending => json!({ "name": "sending" }),
        SendState::Sent { txids } => json!({ "name": "sent", "txids": txids }),
        SendState::OutcomeUnknown { txids, settled } => json!({
            "name": "outcome_unknown",
            "txids": txids,
            "settled": settled,
        }),
        SendState::Failed(problem) => json!({ "name": "failed", "problem": problem_json(problem) }),
    };
    let history: Vec<Value> = state
        .history
        .iter()
        .map(|transfer| {
            json!({
                "txid": transfer.txid,
                "direction": match transfer.direction {
                    Direction::Incoming => "in",
                    Direction::Outgoing => "out",
                },
                "amount": transfer.amount.to_string(),
                "fee": transfer.fee.to_string(),
                "block_height": transfer.block_height,
                "confirmations": transfer.confirmations,
                "timestamp": transfer.timestamp,
                "pending": transfer.pending,
                "failed": transfer.failed,
                "address_indices": transfer.address_indices,
            })
        })
        .collect();
    json!({
        "revision": state.revision,
        "phase": phase,
        "tor": {
            "state": tor_state,
            "percent": state.tor.percent,
            "message": state.tor.message,
        },
        "node": state.node.as_ref().map(ToString::to_string),
        "own_nodes": state.own_nodes.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "sync": sync,
        "syncs_ended": state.syncs_ended,
        "balance": {
            "total": state.balance.total.to_string(),
            "unlocked": state.balance.unlocked.to_string(),
            // What is in the wallet and cannot be spent yet. Worked out
            // here so that no interface does arithmetic on money.
            "locked": state
                .balance
                .total
                .checked_sub(state.balance.unlocked)
                .unwrap_or(Amount::ZERO)
                .to_string(),
        },
        "receive_address": state.receive_address.as_ref().map(|address| json!({
            "index": address.index,
            "address": address.address,
        })),
        "history": history,
        "send": send,
        "rebuilt_from_keys": state.rebuilt_from_keys,
        "not_saved": state.not_saved.as_ref().map(problem_json),
        "last_problem": state.last_problem.as_ref().map(problem_json),
    })
}

fn apply(session: &Session, request: Request) {
    match request {
        Request::Sync => session.sync(),
        Request::NewAddress => session.new_address(),
        Request::PrepareSend { to, amount } => session.prepare_send(&to, amount),
        Request::ConfirmSend => session.confirm_send(),
        Request::CancelSend => session.cancel_send(),
        Request::ClearSend => session.clear_send(),
        Request::SetNodes(nodes) => session.set_nodes(nodes),
    }
}

/// Hands text to C. Interior NUL bytes, which C cannot carry, become spaces.
fn to_c(text: String) -> *mut c_char {
    CString::new(text.replace('\0', " ")).map_or(ptr::null_mut(), CString::into_raw)
}

/// Hands a secret to C, to be freed with `veil_secret_free`, which wipes
/// it. The one copy made here is the one handed over: nothing of the
/// secret is left behind in memory that is freed without being wiped,
/// which [`to_c`] would do.
fn secret_to_c(secret: &str) -> *mut c_char {
    CString::new(secret).map_or(ptr::null_mut(), CString::into_raw)
}

/// # Safety
///
/// `pointer` must be null or point to a NUL-terminated string that stays
/// valid for the duration of the call.
unsafe fn from_c<'a>(pointer: *const c_char) -> Option<&'a str> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    unsafe { CStr::from_ptr(pointer) }.to_str().ok()
}

/// Runs `read` on the session behind `handle`, or returns `fallback` for a
/// null handle, a closed session, or a panic.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`.
pub(crate) unsafe fn with_session<T>(
    handle: *const VeilSession,
    fallback: T,
    read: impl FnOnce(&Session) -> T,
) -> T {
    // SAFETY: the caller guarantees null or a live handle.
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return fallback;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let session = handle
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        session.map(|session| read(&session))
    }));
    match outcome {
        Ok(Some(value)) => value,
        _ => fallback,
    }
}

/// Starts a session with whatever `hooks` say carries its traffic.
pub(crate) fn start_with(
    config: &str,
    password: &str,
    phrase: Option<&str>,
    hooks: Hooks,
) -> Result<*mut VeilSession, Problem> {
    let (config, source) = parse_start(config, phrase)?;
    let session = Session::start_with(config, source, password, hooks);
    Ok(Box::into_raw(Box::new(VeilSession {
        session: Mutex::new(Some(Arc::new(session))),
    })))
}

/// Opens a wallet and starts Tor on a thread of their own, and returns at
/// once. `config` is a JSON object:
///
/// ```json
/// {"network": "stagenet", "source": "create",
///  "wallet_library": "...", "wallet_path": "...",
///  "tor": {"state_dir": "...", "cache_dir": "..."},
///  "nodes": ["host.onion:38089"], "sync_every_seconds": 120}
/// ```
///
/// `nodes` are the user's own, tried in the order given. Left out or
/// empty, the public stagenet nodes are used, in an order nobody can
/// predict. `sync_every_seconds` may be left out too.
///
/// `source` is `create`, `open`, or `restore`; a restore takes the recovery
/// phrase in `phrase` and may give `restore_height`. `phrase` is null
/// otherwise.
///
/// Returns a handle, to be released with `veil_session_close`. If the
/// request itself cannot be read, returns null and, when `problem` is not
/// null, stores there a JSON object `{"kind", "detail"}` to be freed with
/// `veil_string_free`. A wallet that does not open is not such a case: the
/// handle is returned and its state says why.
///
/// # Safety
///
/// `config` and `password` must point to NUL-terminated strings, `phrase`
/// must be null or point to one, and all must stay valid for the duration
/// of the call. `problem` must be null or point to writable space for one
/// pointer.
#[no_mangle]
pub unsafe extern "C" fn veil_session_start(
    config: *const c_char,
    password: *const c_char,
    phrase: *const c_char,
    problem: *mut *mut c_char,
) -> *mut VeilSession {
    // SAFETY: the caller guarantees the three strings.
    let (config, password, phrase) = unsafe { (from_c(config), from_c(password), from_c(phrase)) };
    let started = catch_unwind(|| {
        let (Some(config), Some(password)) = (config, password) else {
            return Err(bad_request("the configuration or the password is missing"));
        };
        let hooks = Hooks {
            transport: Box::new(|tor| match TorService::start(tor) {
                Ok(service) => Ok(Box::new(service) as Box<dyn Transport>),
                Err(error) => Err(Problem {
                    kind: ProblemKind::TorFailed,
                    detail: error.to_string(),
                }),
            }),
            wallet: Box::new(|_| {}),
            save_every: crate::session::SAVE_EVERY,
            send_settles_after: crate::session::SEND_SETTLES_AFTER,
        };
        start_with(config, password, phrase, hooks)
    });
    let failure = match started {
        Ok(Ok(handle)) => return handle,
        Ok(Err(problem)) => problem,
        Err(_) => bad_request("starting the session failed unexpectedly"),
    };
    if !problem.is_null() {
        // SAFETY: the caller guarantees writable space for one pointer.
        unsafe { *problem = to_c(problem_json(&failure).to_string()) };
    }
    ptr::null_mut()
}

/// The state's revision, which goes up with every change. Cheap enough to
/// ask many times a second; zero for a null or closed handle.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_session_revision(handle: *const VeilSession) -> u64 {
    // SAFETY: passed on from the caller.
    unsafe { with_session(handle, 0, |session| session.state().revision) }
}

/// The state as a JSON object, to be freed with `veil_string_free`. Null
/// for a null or closed handle.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_session_state(handle: *const VeilSession) -> *mut c_char {
    // SAFETY: passed on from the caller.
    unsafe {
        with_session(handle, ptr::null_mut(), |session| {
            to_c(state_json(&session.state()).to_string())
        })
    }
}

/// Gives the session a command, a JSON object such as `{"command":
/// "sync"}`. The commands are `sync`, `new_address`, `prepare_send` (with
/// `to` and `amount`, a decimal number as text or `all`), `confirm_send`,
/// `cancel_send` (drops a send that has gone nowhere), `clear_send` (takes
/// the outcome of the last send off the state), and `set_nodes` (with `nodes`, the user's own nodes as
/// `host.onion:port`; an empty list goes back to the nodes the session was
/// started with).
///
/// Returns null when the command was handed over; its outcome appears in
/// the state. Returns a JSON object `{"kind", "detail"}`, to be freed with
/// `veil_string_free`, when the command could not be read.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`.
/// `command` must be null or point to a NUL-terminated string that stays
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veil_session_command(
    handle: *const VeilSession,
    command: *const c_char,
) -> *mut c_char {
    // SAFETY: the caller guarantees the string.
    let command = unsafe { from_c(command) };
    let refused = |problem: Problem| to_c(problem_json(&problem).to_string());
    let request = match catch_unwind(|| command.map(parse_command)) {
        Ok(Some(Ok(request))) => request,
        Ok(Some(Err(problem))) => return refused(problem),
        _ => return refused(bad_request("the command is missing")),
    };
    // SAFETY: passed on from the caller.
    let handed_over = unsafe {
        with_session(handle, false, |session| {
            apply(session, request);
            true
        })
    };
    if handed_over {
        ptr::null_mut()
    } else {
        refused(Problem {
            kind: ProblemKind::NotNow,
            detail: "the session is closed".into(),
        })
    }
}

/// Checks bridge lines without starting anything: `{"bridges": ["obfs4
/// ...", ...]}`. Returns null if every line is a bridge and a program is
/// installed for every transport they need; otherwise a JSON object
/// `{"kind", "detail"}`, to be freed with `veil_string_free`, with the kind
/// `invalid_bridge` or `transport_missing`.
///
/// # Safety
///
/// `request` must be null or point to a NUL-terminated string that stays
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veil_check_bridges(request: *const c_char) -> *mut c_char {
    // SAFETY: the caller guarantees the string.
    let request = unsafe { from_c(request) };
    let checked = catch_unwind(|| match request {
        Some(request) => object(request).and_then(|request| bridges_in(&request)),
        None => Err(bad_request("the request is missing")),
    });
    match checked {
        Ok(Ok(_)) => ptr::null_mut(),
        Ok(Err(problem)) => to_c(problem_json(&problem).to_string()),
        Err(_) => {
            to_c(problem_json(&bad_request("checking the bridges failed unexpectedly")).to_string())
        }
    }
}

/// Whether `password` is the wallet's password: 1 if it is, 0 if it is
/// not, and -1 if that cannot be said now, for a null or closed handle or
/// while a transaction is under review. Waits for the session's thread,
/// like `veil_session_recovery_phrase`.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`;
/// `password` must be null or point to a NUL-terminated string that stays
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veil_session_is_password(
    handle: *const VeilSession,
    password: *const c_char,
) -> i32 {
    // SAFETY: the caller guarantees the string.
    let Some(password) = (unsafe { from_c(password) }) else {
        return -1;
    };
    // SAFETY: passed on from the caller.
    unsafe {
        with_session(handle, -1, |session| match session.is_password(password) {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(_) => -1,
        })
    }
}

/// Gives the wallet the password `new`, if `old` is its password now.
/// Returns null when it is changed; otherwise a JSON object `{"kind",
/// "detail"}`, to be freed with `veil_string_free`, with the kind
/// `wrong_password` when `old` is not the password, and nothing is
/// changed. Waits for the session's thread.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`; `old`
/// and `new` must be null or point to NUL-terminated strings that stay
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veil_session_change_password(
    handle: *const VeilSession,
    old: *const c_char,
    new: *const c_char,
) -> *mut c_char {
    let refused = |problem: Problem| to_c(problem_json(&problem).to_string());
    // SAFETY: the caller guarantees the strings.
    let (Some(old), Some(new)) = (unsafe { (from_c(old), from_c(new)) }) else {
        return refused(bad_request("a password is missing"));
    };
    let closed = || Problem {
        kind: ProblemKind::NotNow,
        detail: "the session is closed".into(),
    };
    // SAFETY: passed on from the caller.
    let changed = unsafe {
        with_session(handle, Err(closed()), |session| {
            session.change_password(old, new)
        })
    };
    match changed {
        Ok(()) => ptr::null_mut(),
        Err(problem) => refused(problem),
    }
}

/// The wallet's recovery phrase, to be freed with `veil_secret_free`, or
/// null if it cannot be given now. Waits for the session's thread, so for
/// the end of a synchronisation if one is running.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_session_recovery_phrase(handle: *const VeilSession) -> *mut c_char {
    // SAFETY: passed on from the caller.
    unsafe {
        with_session(handle, ptr::null_mut(), |session| {
            match session.recovery_phrase() {
                Ok(phrase) => secret_to_c(phrase.as_str()),
                Err(_) => ptr::null_mut(),
            }
        })
    }
}

/// Saves and closes the wallet, stops Tor, and releases the handle, which
/// must not be used afterwards. Waits for a synchronisation under way.
/// Returns the last state as a JSON object, to be freed with
/// `veil_string_free`; its phase is `closed`, or `failed` with the reason.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_session_start`, and
/// no other thread may be using it.
#[no_mangle]
pub unsafe extern "C" fn veil_session_close(handle: *mut VeilSession) -> *mut c_char {
    if handle.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the caller guarantees a live handle that nobody else uses.
    let handle = unsafe { Box::from_raw(handle) };
    let closed = catch_unwind(AssertUnwindSafe(|| {
        let session = handle
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        session.map(|session| state_json(&session.end()).to_string())
    }));
    match closed {
        Ok(Some(state)) => to_c(state),
        _ => ptr::null_mut(),
    }
}

/// Frees text returned by this interface, the recovery phrase excepted.
///
/// # Safety
///
/// `text` must be null or a pointer returned by this interface that has
/// not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn veil_string_free(text: *mut c_char) {
    if !text.is_null() {
        // SAFETY: the caller guarantees the pointer came from `to_c`.
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Wipes and frees a recovery phrase returned by
/// `veil_session_recovery_phrase`.
///
/// # Safety
///
/// `secret` must be null or a pointer returned by
/// `veil_session_recovery_phrase` that has not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn veil_secret_free(secret: *mut c_char) {
    if !secret.is_null() {
        // SAFETY: the caller guarantees the pointer came from `to_c`.
        let mut bytes = unsafe { CString::from_raw(secret) }.into_bytes();
        zeroize::Zeroize::zeroize(&mut bytes);
    }
}
