//! The C interface: what it reads, what it writes, and a session driven
//! through it from start to close.

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use serde_json::Value;
use veil_tor::{StandIn, State, Status, Transport};
use veil_wallet::{Amount, Balance, Direction, ReceiveAddress, SendAmount, SyncProgress, Transfer};

use super::{TempDir, BROADCAST_HOST, PASSWORD, SYNC_HOST};
use crate::ffi::{
    parse_command, parse_start, start_with, state_json, veil_secret_free, veil_session_close,
    veil_session_command, veil_session_recovery_phrase, veil_session_revision, veil_session_start,
    veil_session_state, veil_string_free, Request, VeilSession,
};
use crate::session::Hooks;
use crate::{
    OnionNode, Phase, Problem, ProblemKind, SendState, SessionState, SyncState, WalletSource,
};

const ADDRESS: &str = "73a4nWuvkYoYoksGurDjKZQcZkmaxLaKbbeiKzHnMmqKivrCzq5Q2JtJG1UZNZFqLPbQ3MiXCk2Q5bdwdUNSr7X9QrPubkn";

fn amount(text: &str) -> Amount {
    text.parse().unwrap()
}

/// A configuration as the app would write it.
fn config_json(dir: &TempDir, source: &str) -> String {
    let library = std::env::var("VEIL_WALLET_LIB").unwrap_or_else(|_| "no-library".into());
    serde_json::json!({
        "network": "stagenet",
        "source": source,
        "wallet_library": library,
        "wallet_path": dir.file("w"),
        "tor": { "state_dir": dir.file("tor-state"), "cache_dir": dir.file("tor-cache") },
        "nodes": [format!("{SYNC_HOST}:38089"), format!("{BROADCAST_HOST}:38089")],
    })
    .to_string()
}

fn kind_of(result: Result<impl Sized, Problem>) -> Option<ProblemKind> {
    result.err().map(|problem| problem.kind)
}

#[test]
fn a_start_request_is_read_or_refused_with_a_reason() {
    let dir = TempDir::new("ffi-start");
    let (config, source) = parse_start(&config_json(&dir, "create"), None).unwrap();
    assert!(matches!(source, WalletSource::Create));
    assert_eq!(config.wallet_path, dir.file("w"));
    assert_eq!(config.nodes.len(), 2);
    assert_eq!(config.sync_every, None);
    assert!(matches!(
        parse_start(&config_json(&dir, "open"), None).unwrap().1,
        WalletSource::Open
    ));

    let change = |field: &str, value: Value| {
        let mut config: Value = serde_json::from_str(&config_json(&dir, "create")).unwrap();
        config[field] = value;
        config.to_string()
    };
    let with_timer = change("sync_every_seconds", 90.into());
    assert_eq!(
        parse_start(&with_timer, None).unwrap().0.sync_every,
        Some(Duration::from_secs(90))
    );

    // Without nodes of the user's own, the public ones are used, all of
    // them, in whatever order.
    for nodes in [Value::Null, serde_json::json!([])] {
        let (config, _) = parse_start(&change("nodes", nodes), None).unwrap();
        assert_eq!(config.nodes.len(), 4);
        assert!(config.nodes.iter().all(|node| node.port() == 38089));
    }
    let own = serde_json::json!([format!("{SYNC_HOST}:38081")]);
    let (config, _) = parse_start(&change("nodes", own), None).unwrap();
    assert_eq!(
        config.nodes,
        vec![OnionNode::new(SYNC_HOST, 38081).unwrap()]
    );

    // Only stagenet, for now.
    let mainnet = change("network", "mainnet".into());
    assert_eq!(
        kind_of(parse_start(&mainnet, None)),
        Some(ProblemKind::BadRequest)
    );
    // A node is an onion service.
    let clearnet = change("nodes", serde_json::json!(["node.example.com:38089"]));
    assert_eq!(
        kind_of(parse_start(&clearnet, None)),
        Some(ProblemKind::InvalidNode)
    );
    // A restore needs its phrase, and the phrase has to be one.
    let restore = change("source", "restore".into());
    assert_eq!(
        kind_of(parse_start(&restore, None)),
        Some(ProblemKind::BadRequest)
    );
    assert_eq!(
        kind_of(parse_start(&restore, Some("three words only"))),
        Some(ProblemKind::InvalidPhrase)
    );
    for broken in [
        "",
        "[]",
        "{}",
        "{\"network\": 5}",
        &change("source", "borrow".into()),
    ] {
        assert_eq!(
            kind_of(parse_start(broken, None)),
            Some(ProblemKind::BadRequest),
            "{broken}"
        );
    }
}

#[test]
fn commands_are_read_or_refused_with_a_reason() {
    let read = |text: &str| parse_command(text);
    assert_eq!(read(r#"{"command":"sync"}"#).unwrap(), Request::Sync);
    assert_eq!(
        read(r#"{"command":"new_address"}"#).unwrap(),
        Request::NewAddress
    );
    assert_eq!(
        read(r#"{"command":"confirm_send"}"#).unwrap(),
        Request::ConfirmSend
    );
    assert_eq!(
        read(r#"{"command":"cancel_send"}"#).unwrap(),
        Request::CancelSend
    );
    assert_eq!(
        read(r#"{"command":"prepare_send","to":"7abc","amount":"1.5"}"#).unwrap(),
        Request::PrepareSend {
            to: "7abc".into(),
            amount: SendAmount::Exact(amount("1.5")),
        }
    );
    assert_eq!(
        read(r#"{"command":"prepare_send","to":"7abc","amount":"all"}"#).unwrap(),
        Request::PrepareSend {
            to: "7abc".into(),
            amount: SendAmount::Everything,
        }
    );
    // A comma is a decimal separator too, as people type it.
    assert_eq!(
        read(r#"{"command":"prepare_send","to":"7abc","amount":"1,5"}"#).unwrap(),
        Request::PrepareSend {
            to: "7abc".into(),
            amount: SendAmount::Exact(amount("1.5")),
        }
    );
    // An amount is text, exact; what is not a number is the user's mistake.
    for typed in ["1.2.3", "one", "1.0000000000001", ""] {
        let command = serde_json::json!({"command": "prepare_send", "to": "7abc", "amount": typed});
        assert_eq!(
            kind_of(read(&command.to_string())),
            Some(ProblemKind::InvalidAmount),
            "{typed}"
        );
    }
    for broken in [
        "",
        "sync",
        "{}",
        r#"{"command":"launch"}"#,
        r#"{"command":"prepare_send","to":"7abc"}"#,
        r#"{"command":"prepare_send","to":"7abc","amount":1.5}"#,
    ] {
        assert_eq!(
            kind_of(read(broken)),
            Some(ProblemKind::BadRequest),
            "{broken}"
        );
    }
}

/// A state with something in every field, and one where things went wrong.
fn sample_states() -> [(&'static str, SessionState); 2] {
    let problem = |kind, detail: &str| Problem {
        kind,
        detail: detail.to_owned(),
    };
    let busy = SessionState {
        revision: 42,
        phase: Phase::Ready,
        tor: Status {
            state: State::Ready,
            percent: 100,
            message: None,
        },
        node: Some(OnionNode::new(SYNC_HOST, 38089).unwrap()),
        sync: SyncState::Running {
            progress: Some(SyncProgress {
                wallet_height: 2_250_000,
                node_height: 2_256_644,
            }),
        },
        syncs_ended: 3,
        balance: Balance {
            total: amount("12.345678901234"),
            unlocked: amount("10.5"),
        },
        receive_address: Some(ReceiveAddress {
            index: 3,
            address: ADDRESS.to_owned(),
        }),
        history: vec![
            Transfer {
                txid: "a".repeat(64),
                direction: Direction::Incoming,
                amount: amount("12.5"),
                fee: Amount::ZERO,
                block_height: 2_256_000,
                confirmations: 644,
                timestamp: 1_795_400_000,
                pending: false,
                failed: false,
                address_indices: vec![1, 2],
            },
            Transfer {
                txid: "b".repeat(64),
                direction: Direction::Outgoing,
                amount: amount("0.15"),
                fee: amount("0.000030660000"),
                block_height: 0,
                confirmations: 0,
                timestamp: 1_795_404_547,
                pending: true,
                failed: false,
                address_indices: Vec::new(),
            },
        ],
        send: SendState::Review {
            to: ADDRESS.to_owned(),
            amount: amount("1.5"),
            fee: amount("0.00003066"),
            total: amount("1.50003066"),
            source_addresses: vec![1, 3],
        },
        rebuilt_from_keys: false,
        last_problem: Some(problem(
            ProblemKind::NotNow,
            "a transaction is under review",
        )),
    };
    let troubled = SessionState {
        revision: 7,
        phase: Phase::Failed(problem(ProblemKind::WrongPassword, "wrong password")),
        tor: Status {
            state: State::Bootstrapping,
            percent: 45,
            message: Some("the network may be blocking Tor".to_owned()),
        },
        node: None,
        sync: SyncState::Failed(problem(
            ProblemKind::TorNotReady,
            "Tor is not ready yet (45%)",
        )),
        syncs_ended: 1,
        balance: Balance {
            total: Amount::ZERO,
            unlocked: Amount::ZERO,
        },
        receive_address: None,
        history: Vec::new(),
        send: SendState::OutcomeUnknown {
            txids: vec!["c".repeat(64)],
            settled: false,
        },
        rebuilt_from_keys: true,
        last_problem: None,
    };
    [("state-busy.json", busy), ("state-troubled.json", troubled)]
}

/// The files in `tests/fixtures` are the state as it crosses the
/// interface. The Dart side reads the same files in its tests, so a change
/// here that it does not follow fails there. `VEIL_WRITE_FIXTURES=1`
/// writes the files instead of comparing with them.
#[test]
fn the_state_crosses_the_interface_as_the_fixtures_say() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (name, state) in sample_states() {
        let written = state_json(&state);
        let file = fixtures.join(name);
        if std::env::var_os("VEIL_WRITE_FIXTURES").is_some() {
            let pretty = serde_json::to_string_pretty(&written).unwrap();
            std::fs::write(&file, pretty + "\n").unwrap();
        }
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(written, expected, "{name}");
    }
}

fn c(text: &str) -> CString {
    CString::new(text).unwrap()
}

/// Takes text handed out by the interface, as JSON, and frees it.
fn take_json(text: *mut c_char) -> Option<Value> {
    if text.is_null() {
        return None;
    }
    // SAFETY: a non-null pointer from the interface is a C string.
    let value = serde_json::from_str(unsafe { CStr::from_ptr(text) }.to_str().unwrap()).unwrap();
    // SAFETY: handed out by the interface and not freed yet.
    unsafe { veil_string_free(text) };
    Some(value)
}

fn state_of(handle: *const VeilSession) -> Value {
    // SAFETY: a live handle.
    take_json(unsafe { veil_session_state(handle) }).expect("a live session has a state")
}

fn command(handle: *const VeilSession, command: Value) -> Option<Value> {
    let command = c(&command.to_string());
    // SAFETY: a live or null handle and a valid string.
    take_json(unsafe { veil_session_command(handle, command.as_ptr()) })
}

/// Polls the state, as the app does, until it satisfies `wanted`.
fn wait(handle: *const VeilSession, wanted: impl Fn(&Value) -> bool) -> Value {
    let started = Instant::now();
    loop {
        let state = state_of(handle);
        if wanted(&state) {
            return state;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the session never got there: {state:#}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_null_handle_and_a_broken_request_are_harmless() {
    let null: *mut VeilSession = ptr::null_mut();
    // SAFETY: null is accepted everywhere.
    unsafe {
        assert_eq!(veil_session_revision(null), 0);
        assert!(veil_session_state(null).is_null());
        assert!(veil_session_recovery_phrase(null).is_null());
        assert!(veil_session_close(null).is_null());
        veil_string_free(ptr::null_mut());
        veil_secret_free(ptr::null_mut());
    }
    let refused = command(null, serde_json::json!({"command": "sync"})).unwrap();
    assert_eq!(refused["kind"], "not_now");

    // A request that cannot be read starts nothing and says why.
    let password = c(PASSWORD);
    for config in [ptr::null(), c("{}").as_ptr()] {
        let mut problem: *mut c_char = ptr::null_mut();
        // SAFETY: valid strings or null, and space for one pointer.
        let handle =
            unsafe { veil_session_start(config, password.as_ptr(), ptr::null(), &mut problem) };
        assert!(handle.is_null());
        assert_eq!(take_json(problem).unwrap()["kind"], "bad_request");
    }
    // The reason may be left unasked for.
    // SAFETY: as above, with nowhere to put the reason.
    let handle =
        unsafe { veil_session_start(ptr::null(), password.as_ptr(), ptr::null(), ptr::null_mut()) };
    assert!(handle.is_null());
}

#[test]
fn a_wallet_that_is_not_there_fails_before_tor_is_started() {
    if std::env::var_os("VEIL_WALLET_LIB").is_none() {
        eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
        return;
    }
    // This goes through the real entry point, which would start the real
    // Tor client once the wallet is open. It never gets that far: there is
    // no wallet to open.
    let dir = TempDir::new("ffi-missing");
    let (config, password) = (c(&config_json(&dir, "open")), c(PASSWORD));
    // SAFETY: valid strings.
    let handle = unsafe {
        veil_session_start(
            config.as_ptr(),
            password.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
        )
    };
    assert!(!handle.is_null());
    let state = wait(handle, |state| state["phase"]["name"] != "opening");
    assert_eq!(state["phase"]["name"], "failed", "{state:#}");
    assert!(!dir.file("tor-state").exists(), "Tor was started");
    // SAFETY: a live handle, used by nobody else.
    let last = take_json(unsafe { veil_session_close(handle) }).unwrap();
    assert_eq!(last["phase"]["name"], "failed");
}

#[test]
fn a_session_is_driven_through_the_interface_from_start_to_close() {
    if std::env::var_os("VEIL_WALLET_LIB").is_none() {
        eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
        return;
    }
    let dir = TempDir::new("ffi-session");
    let stand_in = StandIn::start().unwrap();
    let transport = stand_in.clone();
    let hooks = Hooks {
        transport: Box::new(move |_| Ok(Box::new(transport) as Box<dyn Transport>)),
        wallet: Box::new(|_| {}),
        save_every: crate::session::SAVE_EVERY,
        send_settles_after: crate::session::SEND_SETTLES_AFTER,
    };
    let handle = start_with(&config_json(&dir, "create"), PASSWORD, None, hooks).unwrap();

    let state = wait(handle, |state| state["phase"]["name"] != "opening");
    assert_eq!(state["phase"]["name"], "ready", "{state:#}");
    assert_eq!(state["tor"]["state"], "ready");
    assert_eq!(state["sync"]["name"], "not_yet");
    assert_eq!(state["send"]["name"], "idle");
    assert_eq!(state["balance"]["total"], "0.00");
    assert_eq!(state["receive_address"]["index"], 1);
    assert_eq!(state["node"], Value::Null);
    // SAFETY: a live handle.
    assert_eq!(unsafe { veil_session_revision(handle) }, state["revision"]);

    // A command is handed over, and its outcome shows in the state.
    assert_eq!(
        command(handle, serde_json::json!({"command": "new_address"})),
        None
    );
    wait(handle, |state| state["receive_address"]["index"] == 2);
    assert_eq!(
        command(handle, serde_json::json!({"command": "sync"})),
        None
    );
    let state = wait(handle, |state| state["syncs_ended"] == 1);
    assert_eq!(state["sync"]["name"], "failed");
    assert_eq!(state["sync"]["problem"]["kind"], "no_node");

    // What cannot be read is refused on the spot and changes nothing.
    // SAFETY: a live handle.
    let before = unsafe { veil_session_revision(handle) };
    let refused = command(handle, serde_json::json!({"command": "launch"})).unwrap();
    assert_eq!(refused["kind"], "bad_request");
    let typed = serde_json::json!({"command": "prepare_send", "to": ADDRESS, "amount": "1.2.3"});
    assert_eq!(command(handle, typed).unwrap()["kind"], "invalid_amount");
    // SAFETY: a live handle.
    assert_eq!(unsafe { veil_session_revision(handle) }, before);

    // The recovery phrase comes out on its own, never in the state.
    // SAFETY: a live handle; the phrase is a C string until it is freed.
    let phrase = unsafe {
        let secret = veil_session_recovery_phrase(handle);
        assert!(!secret.is_null());
        let phrase = CStr::from_ptr(secret).to_str().unwrap().to_owned();
        veil_secret_free(secret);
        phrase
    };
    assert_eq!(phrase.split_whitespace().count(), 16);
    let first_word = phrase.split_whitespace().next().unwrap();
    assert!(!state_of(handle)
        .to_string()
        .contains(&format!("{first_word} ")));

    // SAFETY: a live handle, used by nobody else.
    let last = take_json(unsafe { veil_session_close(handle) }).unwrap();
    assert_eq!(last["phase"]["name"], "closed");
}
