//! C interface to the session, called from the app through FFI.
//!
//! The interface is a handful of functions, and what crosses it is text: a
//! JSON object to start a session, a JSON object per command, and the state
//! as a JSON object. The two secrets, the password and a recovery phrase,
//! travel as arguments of their own and never inside that text.
//!
//! Every function is safe to call from any thread. Reading the state and
//! giving a command never wait for the network; closing a session and
//! asking for the recovery phrase wait for the session's thread. A null
//! handle is accepted everywhere. Panics never cross this boundary.
//!
//! The format is described in the crate's README, and the state as it
//! crosses the interface is pinned by the files in `tests/fixtures`, which
//! the Dart side reads in its own tests.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;
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
    /// `None` once the session has been closed through the handle.
    session: Mutex<Option<Session>>,
}

/// A command as it arrives in text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Sync,
    NewAddress,
    PrepareSend { to: String, amount: SendAmount },
    ConfirmSend,
    CancelSend,
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
    // The user's own nodes, in their order. Without any, the public ones.
    let no_nodes = Vec::new();
    let nodes = match config.get("nodes") {
        None | Some(Value::Null) => &no_nodes,
        Some(nodes) => nodes
            .as_array()
            .ok_or_else(|| bad_request("`nodes` is not a list"))?,
    };
    let nodes = nodes
        .iter()
        .map(|node| {
            node.as_str()
                .and_then(|node| node.parse::<OnionNode>().ok())
                .ok_or_else(|| Problem {
                    kind: ProblemKind::InvalidNode,
                    detail: format!("{node} is not a node; a node is written as host.onion:port"),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
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
            // Bridges are not reachable through this interface yet.
            bridges: Bridges::none(),
        },
        nodes,
        sync_every,
    };
    Ok((config, source))
}

/// Reads one command.
pub(crate) fn parse_command(command: &str) -> Result<Request, Problem> {
    let command = object(command)?;
    match text(&command, "command")? {
        "sync" => Ok(Request::Sync),
        "new_address" => Ok(Request::NewAddress),
        "confirm_send" => Ok(Request::ConfirmSend),
        "cancel_send" => Ok(Request::CancelSend),
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
        ProblemKind::InvalidPhrase => "invalid_phrase",
        ProblemKind::TorNotReady => "tor_not_ready",
        ProblemKind::TorFailed => "tor_failed",
        ProblemKind::NoNode => "no_node",
        ProblemKind::InvalidAddress => "invalid_address",
        ProblemKind::InvalidAmount => "invalid_amount",
        ProblemKind::InvalidNode => "invalid_node",
        ProblemKind::ZeroAmount => "zero_amount",
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
        SendState::Review { to, amount, fee } => json!({
            "name": "review",
            "to": to,
            "amount": amount.to_string(),
            "fee": fee.to_string(),
        }),
        SendState::Sending => json!({ "name": "sending" }),
        SendState::Sent { txids } => json!({ "name": "sent", "txids": txids }),
        SendState::OutcomeUnknown => json!({ "name": "outcome_unknown" }),
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
        "sync": sync,
        "syncs_ended": state.syncs_ended,
        "balance": {
            "total": state.balance.total.to_string(),
            "unlocked": state.balance.unlocked.to_string(),
        },
        "receive_address": state.receive_address.as_ref().map(|address| json!({
            "index": address.index,
            "address": address.address,
        })),
        "history": history,
        "send": send,
        "rebuilt_from_keys": state.rebuilt_from_keys,
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
    }
}

/// Hands text to C. Interior NUL bytes, which C cannot carry, become spaces.
fn to_c(text: String) -> *mut c_char {
    CString::new(text.replace('\0', " ")).map_or(ptr::null_mut(), CString::into_raw)
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
unsafe fn with_session<T>(
    handle: *const VeilSession,
    fallback: T,
    read: impl FnOnce(&Session) -> T,
) -> T {
    // SAFETY: the caller guarantees null or a live handle.
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return fallback;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let session = handle.session.lock().unwrap_or_else(|e| e.into_inner());
        session.as_ref().map(read)
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
        session: Mutex::new(Some(session)),
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
/// `to` and `amount`, a decimal number as text or `all`), `confirm_send`
/// and `cancel_send`.
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
                Ok(phrase) => to_c(phrase.as_str().to_owned()),
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
        session.map(|session| state_json(&session.close()).to_string())
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
