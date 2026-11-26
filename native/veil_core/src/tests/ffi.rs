//! The C interface: what it reads, what it writes, and a session driven
//! through it from start to close.

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use serde_json::Value;
use veil_tor::{StandIn, State, Status, Transport};
use veil_wallet::{
    Amount, Balance, Direction, Network, ReceiveAddress, SendAmount, SyncProgress, Transfer,
};

use super::{TempDir, BROADCAST_HOST, PASSWORD, SYNC_HOST};
use crate::ffi::{
    parse_command, parse_start, start_with, state_json, veil_check_bridges, veil_secret_free,
    veil_session_change_password, veil_session_close, veil_session_command,
    veil_session_is_password, veil_session_recovery_phrase, veil_session_revision,
    veil_session_start, veil_session_state, veil_string_free, with_session, Request, VeilSession,
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

    // The network is one of two, by name, and is never left out: what
    // is asked for decides whether the wallet holds real money.
    let (stagenet, _) = parse_start(&config_json(&dir, "create"), None).unwrap();
    assert_eq!(stagenet.network, Network::Stagenet);
    let (mainnet, _) = parse_start(&change("network", "mainnet".into()), None).unwrap();
    assert_eq!(mainnet.network, Network::Mainnet);
    // The user's own nodes are used on either network, as named.
    assert_eq!(mainnet.nodes, stagenet.nodes);
    for other in ["testnet", "Mainnet", "main", "", " mainnet"] {
        assert_eq!(
            kind_of(parse_start(&change("network", other.into()), None)),
            Some(ProblemKind::BadRequest),
            "{other:?}"
        );
    }
    assert_eq!(
        kind_of(parse_start(&change("network", Value::Null), None)),
        Some(ProblemKind::BadRequest)
    );
    let mut unnamed: Value = serde_json::from_str(&config_json(&dir, "create")).unwrap();
    unnamed.as_object_mut().unwrap().remove("network");
    assert_eq!(
        kind_of(parse_start(&unnamed.to_string(), None)),
        Some(ProblemKind::BadRequest)
    );
    // Without nodes of the user's own, each network has its public ones,
    // and neither is given a node of the other.
    let mut public = Vec::new();
    for network in ["mainnet", "stagenet"] {
        let mut request: Value = serde_json::from_str(&change("network", network.into())).unwrap();
        request.as_object_mut().unwrap().remove("nodes");
        let (config, _) = parse_start(&request.to_string(), None).unwrap();
        let mut nodes: Vec<String> = config.nodes.iter().map(|node| node.to_string()).collect();
        nodes.sort();
        let mut expected: Vec<String> = crate::public_nodes(config.network)
            .iter()
            .map(|node| node.to_string())
            .collect();
        expected.sort();
        assert_eq!(nodes, expected, "{network}");
        public.push(nodes);
    }
    assert!(public[0].iter().all(|node| !public[1].contains(node)));
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
        read(r#"{"command":"clear_send"}"#).unwrap(),
        Request::ClearSend
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
    // The user's own nodes, or none for the ones the session started with.
    let own = serde_json::json!({
        "command": "set_nodes",
        "nodes": [format!("{SYNC_HOST}:38081"), format!(" {BROADCAST_HOST}:38089 ")],
    });
    assert_eq!(
        read(&own.to_string()).unwrap(),
        Request::SetNodes(vec![
            OnionNode::new(SYNC_HOST, 38081).unwrap(),
            OnionNode::new(BROADCAST_HOST, 38089).unwrap(),
        ])
    );
    for none in [
        r#"{"command":"set_nodes"}"#,
        r#"{"command":"set_nodes","nodes":[]}"#,
    ] {
        assert_eq!(read(none).unwrap(), Request::SetNodes(Vec::new()));
    }
    assert_eq!(
        read(r#"{"command":"set_nodes","nodes":["example.com:18081"]}"#)
            .unwrap_err()
            .kind,
        ProblemKind::InvalidNode
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
    // Nor is a comma that could be grouping thousands read either way.
    for typed in ["1.2.3", "one", "1.0000000000001", "", "1,250"] {
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

/// A state with something in every field, one where things went wrong,
/// and one of an open wallet whose Tor has lost its network.
fn sample_states() -> [(&'static str, SessionState); 3] {
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
        own_nodes: vec![
            OnionNode::new(SYNC_HOST, 38089).unwrap(),
            OnionNode::new(BROADCAST_HOST, 38089).unwrap(),
        ],
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
        not_saved: None,
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
        own_nodes: Vec::new(),
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
        not_saved: Some(problem(
            ProblemKind::Other,
            "failed to save file /home/user/.local/share/veil/stagenet/wallet.new",
        )),
        last_problem: None,
    };
    let lost = SessionState {
        revision: 19,
        phase: Phase::Ready,
        tor: Status {
            state: State::Lost,
            percent: 100,
            message: Some("error connecting to Tor".to_owned()),
        },
        node: Some(OnionNode::new(SYNC_HOST, 38089).unwrap()),
        own_nodes: Vec::new(),
        sync: SyncState::Failed(problem(
            ProblemKind::TorLost,
            "Tor cannot reach its network: error connecting to Tor",
        )),
        syncs_ended: 5,
        balance: Balance {
            total: amount("2"),
            unlocked: amount("2"),
        },
        receive_address: Some(ReceiveAddress {
            index: 1,
            address: ADDRESS.to_owned(),
        }),
        history: Vec::new(),
        send: SendState::Failed(problem(
            ProblemKind::TorLost,
            "Tor cannot reach its network: error connecting to Tor",
        )),
        rebuilt_from_keys: false,
        not_saved: None,
        last_problem: None,
    };
    [
        ("state-busy.json", busy),
        ("state-troubled.json", troubled),
        ("state-lost.json", lost),
    ]
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
    // Loaded here, before any wait with a limit begins; see the function.
    if super::wallet_library().is_none() {
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

    // The password is checked and changed through calls of their own, and
    // is in no state either.
    let c = |text: &str| CString::new(text).unwrap();
    let (right, wrong, new) = (c(PASSWORD), c("not the password"), c("a new password"));
    // SAFETY: a live handle and valid strings.
    unsafe {
        assert_eq!(veil_session_is_password(handle, right.as_ptr()), 1);
        assert_eq!(veil_session_is_password(handle, wrong.as_ptr()), 0);
        assert_eq!(veil_session_is_password(handle, ptr::null()), -1);
        assert_eq!(veil_session_is_password(ptr::null(), right.as_ptr()), -1);
        let refused = take_json(veil_session_change_password(
            handle,
            wrong.as_ptr(),
            new.as_ptr(),
        ))
        .unwrap();
        assert_eq!(refused["kind"], "wrong_password");
        assert_eq!(veil_session_is_password(handle, right.as_ptr()), 1);
        assert!(veil_session_change_password(handle, right.as_ptr(), new.as_ptr()).is_null());
        assert_eq!(veil_session_is_password(handle, new.as_ptr()), 1);
        assert_eq!(veil_session_is_password(handle, right.as_ptr()), 0);
        let refused = take_json(veil_session_change_password(
            handle,
            new.as_ptr(),
            ptr::null(),
        ))
        .unwrap();
        assert_eq!(refused["kind"], "bad_request");
    }

    // A call that waits for the session's thread keeps nobody else from
    // the session meanwhile: the app reads the state several times a
    // second, on another thread, and must not stand still for as long as
    // a password takes to check in the middle of a synchronisation. Shown
    // by a call made from within another; were the handle locked for the
    // whole of a call, this one would never come back.
    // SAFETY: a live handle, in both calls.
    let within = unsafe {
        with_session(handle, 0, |session| {
            assert_eq!(session.is_password(PASSWORD), Ok(false));
            veil_session_revision(handle)
        })
    };
    assert_ne!(within, 0);

    // Nodes of the user's own are taken, and shown as in force.
    let own = format!("{BROADCAST_HOST}:38081");
    assert_eq!(
        command(
            handle,
            serde_json::json!({"command": "set_nodes", "nodes": [own]})
        ),
        None
    );
    let state = wait(handle, |state| {
        state["own_nodes"] == serde_json::json!([own])
    });
    assert_eq!(state["last_problem"], Value::Null);

    // SAFETY: a live handle, used by nobody else.
    let last = take_json(unsafe { veil_session_close(handle) }).unwrap();
    assert_eq!(last["phase"]["name"], "closed");
}

#[test]
fn bridges_are_checked_before_anything_is_started() {
    let dir = TempDir::new("ffi-bridges");
    let with = |bridges: Value| {
        let mut config: Value = serde_json::from_str(&config_json(&dir, "create")).unwrap();
        config["bridges"] = bridges;
        parse_start(&config.to_string(), None).map(|(config, _)| config.tor.bridges)
    };
    // None, in every way of saying so.
    for none in [
        Value::Null,
        serde_json::json!([]),
        serde_json::json!(["", "# a comment"]),
    ] {
        assert!(with(none).unwrap().is_empty());
    }
    // A bridge that needs no transport needs no program either.
    let plain = "192.0.2.55:443 316E643333645F6D79216558614D3931657A5F5F";
    let bridges = with(serde_json::json!([plain, "  "])).unwrap();
    assert_eq!(bridges.lines, vec![plain.to_owned()]);
    assert!(bridges.transports.is_empty());
    // What is not a bridge is refused, by the number of its line and
    // without repeating it.
    let problem = with(serde_json::json!([plain, "not a bridge at all"])).unwrap_err();
    assert_eq!(problem.kind, ProblemKind::InvalidBridge, "{problem:?}");
    assert!(problem.detail.contains('2'), "{problem:?}");
    assert!(
        !problem.detail.contains("not a bridge at all"),
        "{problem:?}"
    );
    // A transport nobody has a program for is said to be missing.
    let unheard = format!("conjure {plain}");
    let problem = with(serde_json::json!([unheard])).unwrap_err();
    assert_eq!(problem.kind, ProblemKind::TransportMissing, "{problem:?}");
    assert!(problem.detail.contains("conjure"), "{problem:?}");
    assert_eq!(
        with(serde_json::json!("obfs4 ...")).unwrap_err().kind,
        ProblemKind::BadRequest
    );

    // The same check is there on its own, for a settings screen.
    let check = |request: &str| {
        let request = CString::new(request).unwrap();
        // SAFETY: a valid string; the result is freed below.
        let answer = unsafe { veil_check_bridges(request.as_ptr()) };
        if answer.is_null() {
            return None;
        }
        // SAFETY: a string this library returned.
        let text = unsafe { CStr::from_ptr(answer) }
            .to_str()
            .unwrap()
            .to_owned();
        // SAFETY: returned by this library, freed once.
        unsafe { veil_string_free(answer) };
        Some(serde_json::from_str::<Value>(&text).unwrap())
    };
    assert_eq!(
        check(&serde_json::json!({ "bridges": [plain] }).to_string()),
        None
    );
    assert_eq!(check("{}"), None);
    let refused = check(&serde_json::json!({ "bridges": ["nonsense"] }).to_string()).unwrap();
    assert_eq!(refused["kind"], "invalid_bridge");
    let refused = check(&serde_json::json!({ "bridges": [unheard] }).to_string()).unwrap();
    assert_eq!(refused["kind"], "transport_missing");
    assert_eq!(check("not json").unwrap()["kind"], "bad_request");
    // SAFETY: null is allowed.
    assert!(!unsafe { veil_check_bridges(std::ptr::null()) }.is_null());

    // An app that brings a transport program along says where it lies,
    // and that one is used, under the name it has in a package for
    // Android. Whatever is installed on this machine comes after it.
    let obfs4 = "obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F \
                 cert=YXJlIGZyZXF1ZW50bHkgZnVsbCBvZiBsaXR0bGUgbWVzc2FnZXMgeW91IGNhbiBmaW5kLg \
                 iat-mode=0";
    let shipped = dir.file("shipped");
    std::fs::create_dir_all(&shipped).unwrap();
    let packaged = shipped.join("liblyrebird.so");
    std::fs::write(&packaged, b"").unwrap();
    let mut config: Value = serde_json::from_str(&config_json(&dir, "create")).unwrap();
    config["bridges"] = serde_json::json!([obfs4]);
    config["transport_dir"] = serde_json::json!(shipped);
    let (started, _) = parse_start(&config.to_string(), None).unwrap();
    let programs = started.tor.bridges.transports;
    assert_eq!(programs.len(), 1, "{programs:?}");
    assert_eq!(programs[0].path, packaged);
    assert_eq!(programs[0].protocols, vec!["obfs4"]);
    assert_eq!(
        check(&serde_json::json!({ "bridges": [obfs4], "transport_dir": shipped }).to_string()),
        None
    );
    // A folder that is not text is a request nobody can read.
    config["transport_dir"] = serde_json::json!(7);
    let refused = parse_start(&config.to_string(), None).map(|_| ());
    assert_eq!(refused.unwrap_err().kind, ProblemKind::BadRequest);
}

/// One made-up text put where a request has a value, a different place
/// each time, both as text and, if it happens to be JSON, as what it says.
fn with_one_value_made_up(request: &Value, slots: &[&[&str]], case: usize, text: &str) -> String {
    let mut request = request.clone();
    let slot = slots[case % slots.len()];
    let value = match (case / slots.len()) % 2 {
        0 => Value::String(text.to_owned()),
        _ => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())),
    };
    let mut place = &mut request;
    for key in &slot[..slot.len() - 1] {
        place = &mut place[*key];
    }
    match slot[slot.len() - 1] {
        // The first of a list.
        "0" => place[0] = value,
        key => place[key] = value,
    }
    request.to_string()
}

fn is_an_onion_node(node: &OnionNode) -> bool {
    node.host().ends_with(".onion")
        && node.port() != 0
        && node.to_string().parse::<OnionNode>().ok().as_ref() == Some(node)
}

/// Whatever the app's side sends to start a session with: read as what it
/// says or refused with a reason, and a refusal never repeats the phrase.
#[test]
fn made_up_start_requests_are_read_or_refused() {
    use std::cell::Cell;

    let dir = TempDir::new("ffi-made-up-start");
    let mut request: Value = serde_json::from_str(&config_json(&dir, "restore")).unwrap();
    request["bridges"] =
        serde_json::json!(["192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956"]);
    request["sync_every_seconds"] = 60.into();
    request["restore_height"] = 1_234.into();
    request["transport_dir"] = Value::Null;
    let slots: [&[&str]; 13] = [
        &["network"],
        &["source"],
        &["wallet_library"],
        &["wallet_path"],
        &["tor"],
        &["tor", "state_dir"],
        &["tor", "cache_dir"],
        &["nodes"],
        &["nodes", "0"],
        &["bridges"],
        &["sync_every_seconds"],
        &["restore_height"],
        &["transport_dir"],
    ];
    // Words no message of the core's has in it.
    let phrase = (0..16)
        .map(|i| format!("zq7w{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let samples = [
        "stagenet",
        "restore",
        "create",
        "open",
        "mainnet",
        "0",
        "1",
        "-1",
        "1.5",
        "18446744073709551616",
        "null",
        "true",
        "[]",
        "{}",
        r#"["2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion:38089"]"#,
        r#"["obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F cert=YXJl iat-mode=0"]"#,
        r#"{"state_dir": "a", "cache_dir": "b"}"#,
        "/tmp/veil",
        phrase.as_str(),
    ];
    let case = Cell::new(0usize);
    veil_test_guard::fuzz::texts("start request", &samples, |text| {
        let number = case.replace(case.get() + 1);
        // Every third time the made-up text is the phrase, or the whole
        // request, instead of one value in it.
        let (config, given) = match number % 3 {
            0 => (request.to_string(), text.to_owned()),
            1 if number % 2 == 1 => (text.to_owned(), phrase.clone()),
            _ => (
                with_one_value_made_up(&request, &slots, number / 3, text),
                phrase.clone(),
            ),
        };
        match parse_start(&config, Some(&given)) {
            Ok((config, source)) => {
                assert!(config.nodes.iter().all(is_an_onion_node));
                assert!(!config.nodes.is_empty(), "a session with no node to ask");
                // Never the third network, and never one by default.
                assert!(matches!(
                    config.network,
                    Network::Mainnet | Network::Stagenet
                ));
                assert_ne!(config.sync_every, Some(Duration::ZERO));
                // Bridges that were asked for are there, with what they
                // need, or the request was refused.
                assert!(config.tor.bridges.transports_needed().is_ok());
                if let WalletSource::Restore { phrase, .. } = source {
                    assert!(matches!(phrase.words().count(), 16 | 25));
                }
            }
            Err(problem) => {
                assert!(
                    matches!(
                        problem.kind,
                        ProblemKind::BadRequest
                            | ProblemKind::InvalidNode
                            | ProblemKind::InvalidBridge
                            | ProblemKind::TransportMissing
                            | ProblemKind::InvalidPhrase
                    ),
                    "{problem:?}"
                );
                assert!(!problem.detail.is_empty());
                if number.is_multiple_of(3) {
                    assert!(!problem.detail.contains("zq7"), "{problem:?}");
                }
            }
        }
    });
}

/// Whatever arrives as a command: one of the commands, with what it was
/// given, or refused with a reason.
#[test]
fn made_up_commands_are_read_or_refused() {
    use std::cell::Cell;

    let send = serde_json::json!({"command": "prepare_send", "to": ADDRESS, "amount": "0.25"});
    let nodes =
        serde_json::json!({"command": "set_nodes", "nodes": [format!("{SYNC_HOST}:38089")]});
    let slots: [&[&str]; 3] = [&["command"], &["to"], &["amount"]];
    let node_slots: [&[&str]; 3] = [&["command"], &["nodes"], &["nodes", "0"]];
    let whole_send = send.to_string();
    let whole_nodes = nodes.to_string();
    let samples = [
        "sync",
        "new_address",
        "prepare_send",
        "confirm_send",
        "cancel_send",
        "clear_send",
        "set_nodes",
        "all",
        " all ",
        "0.25",
        "1,250",
        "-1",
        "null",
        "[]",
        ADDRESS,
        whole_send.as_str(),
        whole_nodes.as_str(),
        r#"{"command": "sync"}"#,
    ];
    let case = Cell::new(0usize);
    veil_test_guard::fuzz::texts("command", &samples, |text| {
        let number = case.replace(case.get() + 1);
        let command = match number % 3 {
            0 => text.to_owned(),
            1 => with_one_value_made_up(&send, &slots, number / 3, text),
            _ => with_one_value_made_up(&nodes, &node_slots, number / 3, text),
        };
        let sent: Option<Value> = serde_json::from_str(&command).ok();
        match parse_command(&command) {
            Ok(Request::PrepareSend { to, amount }) => {
                let sent = sent.unwrap();
                // The address goes to the wallet library as it came; the
                // amount is the one written.
                assert_eq!(sent["to"], to.as_str());
                let written = sent["amount"].as_str().unwrap().trim();
                match amount {
                    SendAmount::Everything => assert_eq!(written, "all"),
                    SendAmount::Exact(amount) => assert_eq!(written.parse(), Ok(amount)),
                }
            }
            Ok(Request::SetNodes(nodes)) => assert!(nodes.iter().all(is_an_onion_node)),
            Ok(other) => {
                let name = sent.unwrap()["command"].as_str().unwrap().to_owned();
                let expected = match other {
                    Request::Sync => "sync",
                    Request::NewAddress => "new_address",
                    Request::ConfirmSend => "confirm_send",
                    Request::CancelSend => "cancel_send",
                    Request::ClearSend => "clear_send",
                    Request::PrepareSend { .. } | Request::SetNodes(_) => unreachable!(),
                };
                assert_eq!(name, expected);
            }
            Err(problem) => {
                assert!(
                    matches!(
                        problem.kind,
                        ProblemKind::BadRequest
                            | ProblemKind::InvalidAmount
                            | ProblemKind::InvalidNode
                    ),
                    "{problem:?}"
                );
                assert!(!problem.detail.is_empty());
            }
        }
    });
}

/// The same through the interface itself, with bytes that need not even
/// be text: an answer every time, in JSON, and nothing started.
#[test]
fn made_up_bytes_through_the_interface_get_an_answer() {
    let command =
        serde_json::json!({"command": "prepare_send", "to": ADDRESS, "amount": "0.25"}).to_string();
    let bridges = serde_json::json!({
        "bridges": ["192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956", "# a note"],
        "transport_dir": "/nowhere",
    })
    .to_string();
    let samples: [&[u8]; 5] = [
        command.as_bytes(),
        bridges.as_bytes(),
        br#"{"command": "sync"}"#,
        br#"{"bridges": ["obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F cert=YXJl iat-mode=0"]}"#,
        b"{}",
    ];
    let kinds = [
        "bad_request",
        "not_now",
        "invalid_amount",
        "invalid_node",
        "invalid_bridge",
        "transport_missing",
    ];
    veil_test_guard::fuzz::bytes("interface", &samples, 3_000, |bytes| {
        // A C string ends at its first zero byte.
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let text = CString::new(&bytes[..end]).unwrap();
        let null: *const VeilSession = ptr::null();
        // SAFETY: a null handle and a valid C string.
        let refused = take_json(unsafe { veil_session_command(null, text.as_ptr()) })
            .expect("no session, so no command is taken");
        assert!(
            kinds.contains(&refused["kind"].as_str().unwrap()),
            "{refused}"
        );
        assert!(refused["detail"].is_string());
        // SAFETY: a valid C string.
        if let Some(refused) = take_json(unsafe { veil_check_bridges(text.as_ptr()) }) {
            assert!(
                kinds.contains(&refused["kind"].as_str().unwrap()),
                "{refused}"
            );
            assert!(refused["detail"].is_string());
        }
    });
}
