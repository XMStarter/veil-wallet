//! The session: the core on its own thread, behind a stand-in for Tor.
//!
//! These need the wallet library (`VEIL_WALLET_LIB`) and are skipped
//! without it. Nothing here leaves the machine.

use std::time::Duration;

use veil_tor::{Bridges, Purpose, StandIn, State, TorConfig, Transport};
use veil_wallet::{Amount, Network, SendAmount, MAX_UNUSED_ADDRESSES};

use super::fake_node::FakeNode;
use super::{
    asked_for_blocks, TempDir, BROADCAST_HOST, BROADCAST_PORT, PASSWORD, SLOW, SYNC_HOST, SYNC_PORT,
};
use crate::session::Hooks;
use crate::{
    OnionNode, Phase, Problem, ProblemKind, SendState, Session, SessionConfig, SessionState,
    SyncState, WalletSource,
};

/// Longer than anything here takes; a wait that runs this long has failed.
const SOON: Duration = Duration::from_secs(60);

/// A third onion name, for a node of the user's own.
const OWN_HOST: &str = "vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd.onion";

/// The configuration of a session whose wallet is `name` in `dir`, or
/// `None`, which skips the test, when there is no wallet library.
fn config(dir: &TempDir, name: &str) -> Option<SessionConfig> {
    let wallet_library = match std::env::var_os("VEIL_WALLET_LIB") {
        Some(path) => path.into(),
        None => {
            assert!(
                std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
                "VEIL_WALLET_LIB is not set but the wallet library is required"
            );
            eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
            return None;
        }
    };
    Some(SessionConfig {
        wallet_library,
        wallet_path: dir.file(name),
        network: Network::Mainnet,
        // Not used: the stand-in takes Tor's place.
        tor: TorConfig {
            state_dir: dir.file("tor-state"),
            cache_dir: dir.file("tor-cache"),
            bridges: Bridges::none(),
        },
        nodes: vec![
            OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap(),
            OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap(),
        ],
        sync_every: None,
    })
}

/// A session with `stand_in` where Tor would be.
fn start(config: SessionConfig, stand_in: &StandIn, source: WalletSource) -> Session {
    start_with_password(config, stand_in, source, PASSWORD)
}

fn start_with_password(
    config: SessionConfig,
    stand_in: &StandIn,
    source: WalletSource,
    password: &str,
) -> Session {
    let stand_in = stand_in.clone();
    let hooks = Hooks {
        transport: Box::new(move |_| Ok(Box::new(stand_in) as Box<dyn Transport>)),
        wallet: Box::new(|_| {}),
        save_every: crate::session::SAVE_EVERY,
        send_settles_after: crate::session::SEND_SETTLES_AFTER,
    };
    Session::start_with(config, source, password, hooks)
}

/// Waits for the wallet to be open, or for the reason it is not.
fn opened(session: &Session) -> SessionState {
    session
        .wait_for(SOON, |state| state.phase != Phase::Opening)
        .expect("the session opens its wallet or says why not")
}

fn wait(session: &Session, wanted: impl Fn(&SessionState) -> bool) -> SessionState {
    match session.wait_for(SOON, wanted) {
        Some(state) => state,
        None => panic!("the session never got there: {:#?}", session.state()),
    }
}

fn failed_with(phase: &Phase) -> Option<ProblemKind> {
    match phase {
        Phase::Failed(problem) => Some(problem.kind),
        _ => None,
    }
}

#[test]
fn a_new_wallet_is_ready_at_once_and_opens_again_as_the_same_wallet() {
    let dir = TempDir::new("session-new");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();

    let session = start(config.clone(), &stand_in, WalletSource::Create);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert_eq!(state.tor.state, State::Ready);
    assert_eq!(state.sync, SyncState::NotYet);
    assert_eq!(state.send, SendState::Idle);
    assert!(state.balance.total.is_zero());
    assert!(state.history.is_empty());
    assert!(!state.rebuilt_from_keys);
    assert_eq!(state.last_problem, None);
    let address = state
        .receive_address
        .clone()
        .expect("an address to receive on");
    assert_eq!(address.index, 1);
    let phrase = session.recovery_phrase().unwrap();
    assert_eq!(phrase.as_str().split_whitespace().count(), 16);
    // Opening a wallet asks nothing of any node.
    assert!(stand_in.requests().is_empty(), "{:?}", stand_in.requests());
    assert_eq!(session.close().phase, Phase::Closed);

    // The same wallet again, with its password.
    let session = start(config.clone(), &stand_in, WalletSource::Open);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert_eq!(state.receive_address, Some(address));
    assert_eq!(session.recovery_phrase().unwrap().as_str(), phrase.as_str());
    // Dropping a session closes its wallet too.
    drop(session);

    // With another password it does not open, and says that this is why.
    let session = start_with_password(config.clone(), &stand_in, WalletSource::Open, "another");
    let state = opened(&session);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::WrongPassword));
    // A session that never opened takes commands without effect.
    session.sync();
    session.new_address();
    assert!(session.recovery_phrase().is_err());
    drop(session);

    // And a new wallet is not made over it.
    let session = start(config, &stand_in, WalletSource::Create);
    let state = opened(&session);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::WalletExists));
}

#[test]
fn a_wallet_restored_from_its_phrase_has_the_same_address() {
    let dir = TempDir::new("session-restore");
    let Some(original) = config(&dir, "original") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(original, &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address;
    let phrase = session.recovery_phrase().unwrap();
    drop(session);

    // A phrase with a word changed is refused, as the phrase's fault, and
    // leaves nothing behind where the wallet would have been.
    let mut words: Vec<&str> = phrase.words().collect();
    words[3] = if words[3] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    let damaged = veil_wallet::SeedPhrase::parse(&words.join(" ")).unwrap();
    let refused = config(&dir, "refused").unwrap();
    let source = WalletSource::Restore {
        phrase: damaged,
        restore_height: 0,
    };
    let session = start(refused.clone(), &stand_in, source);
    let state = opened(&session);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::InvalidPhrase));
    drop(session);
    assert!(!refused.wallet_path.exists());

    let restored = config(&dir, "restored").unwrap();
    let source = WalletSource::Restore {
        phrase,
        restore_height: 0,
    };
    let session = start(restored, &stand_in, source);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert_eq!(state.receive_address, address);
}

#[test]
fn a_synchronisation_that_fails_says_why_and_the_session_goes_on() {
    let dir = TempDir::new("session-sync");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config, &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address;

    // No node is reachable: both candidates are asked, through the sync
    // listener, and the outcome is in the state.
    session.sync();
    let state = wait(&session, |state| matches!(state.sync, SyncState::Failed(_)));
    let SyncState::Failed(problem) = &state.sync else {
        unreachable!()
    };
    assert_eq!(problem.kind, ProblemKind::NoNode, "{problem:?}");
    let requests = stand_in.requests();
    assert!(requests.iter().any(|request| request.host == SYNC_HOST));
    assert!(requests
        .iter()
        .any(|request| request.host == BROADCAST_HOST));
    assert!(requests
        .iter()
        .all(|request| request.purpose == Purpose::Sync));
    assert!(state.balance.total.is_zero());
    assert_eq!(state.receive_address, address);

    // Tor is not ready. The state shows it without being asked, and a
    // synchronisation fails for that reason, with no node tried.
    stand_in.set_ready(false);
    let state = wait(&session, |state| state.tor.state == State::Bootstrapping);
    assert_eq!(state.syncs_ended, 1);
    stand_in.clear_requests();
    session.sync();
    let state = wait(&session, |state| state.syncs_ended == 2);
    let SyncState::Failed(problem) = &state.sync else {
        unreachable!()
    };
    assert_eq!(problem.kind, ProblemKind::TorNotReady, "{problem:?}");
    assert!(stand_in.requests().is_empty());
    stand_in.set_ready(true);
    wait(&session, |state| state.tor.state == State::Ready);

    // The network goes from under a Tor that is ready. Nothing shows it
    // until a connection is asked for. Then the synchronisation fails for
    // that reason, not for the nodes, which were asked for through Tor;
    // and that it failed and what Tor says now are one change.
    stand_in.set_network(false);
    assert_eq!(session.state().tor.state, State::Ready);
    session.sync();
    let state = wait(&session, |state| {
        state.syncs_ended == 3 || state.tor.state == State::Lost
    });
    assert_eq!(state.syncs_ended, 3, "{state:#?}");
    assert_eq!(state.tor.state, State::Lost, "{state:#?}");
    let SyncState::Failed(problem) = &state.sync else {
        unreachable!()
    };
    assert_eq!(problem.kind, ProblemKind::TorLost, "{problem:?}");
    assert!(!stand_in.requests().is_empty());

    // The session is still there for everything else.
    session.new_address();
    let state = wait(&session, |state| {
        state.receive_address.as_ref().is_some_and(|a| a.index == 2)
    });
    assert_eq!(state.last_problem, None);
    assert_eq!(session.close().phase, Phase::Closed);
}

/// Nobody has to press anything for a wallet to find its network again:
/// while Tor says that it has lost it, the session goes on synchronising
/// on its timer, and the first connection that gets through makes Tor
/// ready again.
#[test]
fn it_goes_on_trying_while_the_network_is_lost_and_sees_it_come_back() {
    let dir = TempDir::new("session-lost");
    let Some(mut config) = config(&dir, "w") else {
        return;
    };
    config.sync_every = Some(Duration::from_millis(300));
    let stand_in = StandIn::start().unwrap();
    let node = FakeNode::start(4_000_000);
    stand_in.route(SYNC_HOST, SYNC_PORT, node.port());
    stand_in.set_network(false);
    let session = start(config, &stand_in, WalletSource::Create);
    opened(&session);

    let state = wait(&session, |state| state.syncs_ended >= 1);
    assert_eq!(state.tor.state, State::Lost, "{state:#?}");
    let SyncState::Failed(problem) = &state.sync else {
        panic!("{state:#?}")
    };
    assert_eq!(problem.kind, ProblemKind::TorLost, "{problem:?}");
    assert!(node.asked().is_empty(), "{:?}", node.asked());

    // It tries again by itself, with the same outcome.
    let tried = state.syncs_ended;
    let state = wait(&session, |state| state.syncs_ended > tried);
    assert_eq!(state.tor.state, State::Lost, "{state:#?}");

    // The network comes back, and nobody tells the session.
    stand_in.set_network(true);
    let state = wait(&session, |state| state.tor.state == State::Ready);
    assert_eq!(state.tor.message, None);
    assert!(!node.asked().is_empty());
    assert_eq!(session.close().phase, Phase::Closed);
}

#[test]
fn it_synchronises_by_itself_when_told_how_often() {
    let dir = TempDir::new("session-timer");
    let Some(mut config) = config(&dir, "w") else {
        return;
    };
    config.sync_every = Some(Duration::from_millis(300));
    let stand_in = StandIn::start().unwrap();
    // Tor is not ready at first: nothing is tried.
    stand_in.set_ready(false);
    let session = start(config, &stand_in, WalletSource::Create);
    opened(&session);
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(session.state().sync, SyncState::NotYet);
    assert!(stand_in.requests().is_empty());

    // Once it is, a synchronisation starts without a command, and again
    // after the first has failed.
    stand_in.set_ready(true);
    wait(&session, |state| matches!(state.sync, SyncState::Failed(_)));
    let first = stand_in.requests().len();
    assert!(first > 0);
    // How soon is not the point, and on a machine busy with other tests
    // the wallet library can take many seconds over a node that is not
    // there.
    let began = std::time::Instant::now();
    let tried_again = (0..SOON.as_millis() / 100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        stand_in.requests().len() > first
    });
    assert!(
        tried_again,
        "no second synchronisation within {SOON:?}: {:#?}",
        session.state()
    );
    eprintln!(
        "the second synchronisation began after {:?}",
        began.elapsed()
    );
}

#[test]
fn the_password_is_checked_and_changed_through_the_session() {
    let dir = TempDir::new("session-password");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let new = "a new and longer password";
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address;

    assert_eq!(session.is_password(PASSWORD), Ok(true));
    assert_eq!(session.is_password("not the password"), Ok(false));
    // With the wrong old password nothing changes.
    let refused = session
        .change_password("not the password", new)
        .unwrap_err();
    assert_eq!(refused.kind, ProblemKind::WrongPassword);
    assert_eq!(session.is_password(PASSWORD), Ok(true));
    session.change_password(PASSWORD, new).unwrap();
    assert_eq!(session.is_password(new), Ok(true));
    assert_eq!(session.is_password(PASSWORD), Ok(false));
    assert_eq!(session.close().phase, Phase::Closed);

    // The old one opens nothing any more; the new one opens the same wallet.
    let session = start(config.clone(), &stand_in, WalletSource::Open);
    assert_eq!(
        failed_with(&opened(&session).phase),
        Some(ProblemKind::WrongPassword)
    );
    let session = start_with_password(config, &stand_in, WalletSource::Open, new);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert_eq!(state.receive_address, address);
    assert!(!state.rebuilt_from_keys);
}

#[test]
fn nodes_of_the_users_own_are_used_and_remembered_in_the_wallet() {
    let dir = TempDir::new("session-own-nodes");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let own = OnionNode::new(OWN_HOST, 18089).unwrap();
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    assert!(opened(&session).own_nodes.is_empty());

    // Set, the node is asked at once, and it alone. Nothing is reachable
    // here, so the synchronisation fails; who was asked is what counts.
    session.set_nodes(vec![own.clone()]);
    let state = wait(&session, |state| state.syncs_ended == 1);
    assert_eq!(state.own_nodes, vec![own.clone()]);
    assert_eq!(state.last_problem, None);
    let requests = stand_in.requests();
    assert!(!requests.is_empty());
    assert!(
        requests.iter().all(|request| request.host == OWN_HOST),
        "{requests:?}"
    );
    assert_eq!(session.close().phase, Phase::Closed);

    // It is not in a file of its own, and not readable in the wallet's.
    for file in std::fs::read_dir(dir.file("")).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap_or_default();
        let name = OWN_HOST.as_bytes();
        assert!(!bytes.windows(name.len()).any(|window| window == name));
    }

    // The next session is started with the same nodes as the first, and
    // goes to the user's own all the same: the wallet remembers it.
    stand_in.clear_requests();
    let session = start(config, &stand_in, WalletSource::Open);
    let state = opened(&session);
    assert_eq!(state.own_nodes, vec![own]);
    session.sync();
    wait(&session, |state| state.syncs_ended == 1);
    assert!(stand_in
        .requests()
        .iter()
        .all(|request| request.host == OWN_HOST));

    // Taken back, the session's nodes are used again.
    stand_in.clear_requests();
    session.set_nodes(Vec::new());
    let state = wait(&session, |state| state.syncs_ended == 2);
    assert!(state.own_nodes.is_empty());
    let requests = stand_in.requests();
    assert!(requests.iter().any(|request| request.host == SYNC_HOST));
    assert!(requests.iter().all(|request| request.host != OWN_HOST));
    assert_eq!(session.close().phase, Phase::Closed);
}

#[test]
fn a_send_left_unknown_by_an_earlier_session_is_taken_up_again() {
    let dir = TempDir::new("session-remembered-send");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address.unwrap().address;
    assert_eq!(session.close().phase, Phase::Closed);

    // What a session writes into the wallet when the answer to a send is
    // lost: when, and the transaction's id. Here it is put there by hand;
    // that a session writes it is shown on a private chain.
    let txid = "ab".repeat(32);
    let left = |note: String| -> Session {
        let transport = stand_in.clone();
        let hooks = Hooks {
            transport: Box::new(move |_| Ok(Box::new(transport) as Box<dyn Transport>)),
            wallet: Box::new(move |wallet| {
                wallet.set_note("veil.unknown_send", &note).unwrap();
            }),
            save_every: crate::session::SAVE_EVERY,
            send_settles_after: crate::session::SEND_SETTLES_AFTER,
        };
        Session::start_with(config.clone(), WalletSource::Open, PASSWORD, hooks)
    };
    let session = left(format!("1795400000 {txid}"));
    let state = opened(&session);
    assert_eq!(
        state.send,
        SendState::OutcomeUnknown {
            txids: vec![txid.clone()],
            settled: false
        }
    );
    // As before the app was closed: no other send is built.
    session.prepare_send(&address, SendAmount::Everything);
    let state = wait(&session, |state| state.last_problem.is_some());
    assert_eq!(
        state.last_problem.map(|problem| problem.kind),
        Some(ProblemKind::NotNow)
    );
    assert!(matches!(state.send, SendState::OutcomeUnknown { .. }));
    assert!(stand_in.requests().is_empty());
    assert_eq!(session.close().phase, Phase::Closed);

    // A record that cannot be read is no record.
    for nonsense in ["", "soon", "1795400000", "1795400000 not-hex", "x abcd"] {
        let session = left(nonsense.to_owned());
        assert_eq!(opened(&session).send, SendState::Idle, "{nonsense:?}");
        assert_eq!(session.close().phase, Phase::Closed);
    }
}

/// A session with two slow fake nodes behind its two candidates, in the
/// middle of a synchronisation: the first node has been asked for blocks
/// and is taking its time.
fn mid_sync(config: SessionConfig, stand_in: &StandIn) -> (Session, FakeNode, FakeNode) {
    let session = start(config, stand_in, WalletSource::Create);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    // The fake nodes report a chain that goes on past where a new wallet
    // starts; the tests in the parent module do the same.
    let height = 4_000_000;
    let first = FakeNode::start_slow(height, SLOW);
    let second = FakeNode::start_slow(height, SLOW);
    stand_in.route(SYNC_HOST, SYNC_PORT, first.port());
    stand_in.route(BROADCAST_HOST, BROADCAST_PORT, second.port());
    session.sync();
    let asked = (0..600).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        asked_for_blocks(&first)
    });
    assert!(asked, "the first node was never asked for blocks");
    (session, first, second)
}

#[test]
fn a_command_does_not_wait_for_a_synchronisation_to_end() {
    let dir = TempDir::new("session-interrupt");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let (session, first, second) = mid_sync(config, &stand_in);

    // Asked for a new address in the middle, the session stops the
    // synchronisation as soon as the library lets it, issues the address,
    // and the synchronisation is still the same one, not ended.
    session.new_address();
    let state = wait(&session, |state| {
        state.receive_address.as_ref().is_some_and(|a| a.index == 2)
    });
    assert_eq!(state.syncs_ended, 0, "{state:#?}");
    assert!(
        matches!(state.sync, SyncState::Running { .. }),
        "{state:#?}"
    );
    let asked_of_first = first.asked().len();

    // Then it goes on by itself where it was: with the second node, the
    // first having failed. These are fakes with no blocks, so this
    // synchronisation fails in the end, and it says so once.
    let state = wait(&session, |state| state.syncs_ended == 1);
    let SyncState::Failed(problem) = &state.sync else {
        panic!("{state:#?}");
    };
    assert_eq!(problem.kind, ProblemKind::NoNode, "{problem:?}");
    assert!(asked_for_blocks(&second), "{:?}", second.asked());
    assert_eq!(first.asked().len(), asked_of_first);
    assert_eq!(session.close().phase, Phase::Closed);
}

#[test]
fn a_send_asked_for_during_a_synchronisation_waits_for_its_end() {
    let dir = TempDir::new("session-send-waits");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let (session, _first, _second) = mid_sync(config, &stand_in);
    let address = session.state().receive_address.unwrap().address;

    // The send is taken at once and shown as being prepared, but nothing
    // is built from a wallet in the middle of a scan.
    session.prepare_send(&address, SendAmount::Everything);
    let state = wait(&session, |state| state.send == SendState::Preparing);
    assert_eq!(state.syncs_ended, 0, "{state:#?}");

    // Cancelled, it is gone, and the synchronisation is still running.
    session.cancel_send();
    let state = wait(&session, |state| state.send == SendState::Idle);
    assert_eq!(state.syncs_ended, 0, "{state:#?}");

    // Asked for again, it is built when the synchronisation has ended.
    // That one failed and the wallet is empty, so the send fails too; what
    // matters here is the order.
    session.prepare_send(&address, SendAmount::Everything);
    let state = wait(&session, |state| matches!(state.send, SendState::Failed(_)));
    assert_eq!(state.syncs_ended, 1, "{state:#?}");
    assert_eq!(session.close().phase, Phase::Closed);
}

#[test]
fn closing_does_not_wait_for_a_synchronisation_to_end() {
    let dir = TempDir::new("session-close-mid-sync");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let (session, _first, second) = mid_sync(config.clone(), &stand_in);
    let address = session.state().receive_address;

    // Closed in the middle, the session lets go of the synchronisation
    // without it ending, and without going on to the second node.
    let state = session.close();
    assert_eq!(state.phase, Phase::Closed, "{state:#?}");
    assert_eq!(state.syncs_ended, 0, "{state:#?}");
    assert!(second.asked().is_empty(), "{:?}", second.asked());

    // The wallet was saved and opens again as it was.
    let session = start(config, &stand_in, WalletSource::Open);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert_eq!(state.receive_address, address);
}

#[test]
fn addresses_are_issued_up_to_the_cap_and_no_further() {
    let dir = TempDir::new("session-addresses");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config, &stand_in, WalletSource::Create);
    opened(&session);

    for _ in 1..MAX_UNUSED_ADDRESSES {
        session.new_address();
    }
    let last = |state: &SessionState| state.receive_address.as_ref().map(|a| a.index);
    let state = wait(&session, |state| last(state) == Some(MAX_UNUSED_ADDRESSES));
    assert_eq!(state.last_problem, None);

    session.new_address();
    let state = wait(&session, |state| state.last_problem.is_some());
    assert_eq!(
        state.last_problem.map(|problem| problem.kind),
        Some(ProblemKind::TooManyUnusedAddresses)
    );
    assert_eq!(last(&session.state()), Some(MAX_UNUSED_ADDRESSES));
}

#[test]
fn a_send_that_cannot_be_built_touches_no_node() {
    let dir = TempDir::new("session-send");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config, &stand_in, WalletSource::Create);
    opened(&session);
    let one: Amount = "1".parse().unwrap();

    session.prepare_send("not an address", SendAmount::Exact(one));
    let state = wait(&session, |state| matches!(state.send, SendState::Failed(_)));
    let SendState::Failed(problem) = &state.send else {
        unreachable!()
    };
    assert_eq!(problem.kind, ProblemKind::InvalidAddress, "{problem:?}");
    assert!(stand_in.requests().is_empty(), "{:?}", stand_in.requests());

    // Confirming with nothing under review does nothing but say so.
    session.confirm_send();
    let state = wait(&session, |state| state.last_problem.is_some());
    assert_eq!(
        state.last_problem.map(|problem| problem.kind),
        Some(ProblemKind::NotNow)
    );
    // Cancelling clears the outcome from the state.
    session.cancel_send();
    wait(&session, |state| state.send == SendState::Idle);
    assert!(stand_in.requests().is_empty());

    // Nor is a node asked about an amount of nothing; and having seen
    // that, the user clears it.
    let address = session.state().receive_address.unwrap().address;
    session.prepare_send(&address, SendAmount::Exact(Amount::ZERO));
    let state = wait(&session, |state| matches!(state.send, SendState::Failed(_)));
    let SendState::Failed(problem) = &state.send else {
        unreachable!()
    };
    assert_eq!(problem.kind, ProblemKind::ZeroAmount, "{problem:?}");
    assert_eq!(state.syncs_ended, 0);
    session.clear_send();
    wait(&session, |state| state.send == SendState::Idle);
    assert!(stand_in.requests().is_empty());
}

#[test]
fn a_transaction_is_built_only_from_a_wallet_level_with_a_node() {
    let dir = TempDir::new("session-send-level");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config, &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address.unwrap().address;
    let one: Amount = "1".parse().unwrap();

    // Nothing has been synchronised in this session, so a send that could
    // be one makes the wallet synchronise first. There is no node here:
    // the synchronisation fails, and the send with it, for the same
    // reason and in the same change of the state. Nothing was built from
    // a wallet that does not know the chain.
    session.prepare_send(&address, SendAmount::Exact(one));
    let state = wait(&session, |state| {
        state.syncs_ended == 1 || matches!(state.send, SendState::Failed(_))
    });
    assert_eq!(state.syncs_ended, 1, "{state:#?}");
    let SyncState::Failed(why) = &state.sync else {
        panic!("{state:#?}");
    };
    assert_eq!(why.kind, ProblemKind::NoNode);
    assert_eq!(state.send, SendState::Failed(why.clone()), "{state:#?}");

    // The next attempt synchronises again: a failure is not remembered as
    // knowing the chain.
    session.clear_send();
    wait(&session, |state| state.send == SendState::Idle);
    session.prepare_send(&address, SendAmount::Exact(one));
    let state = wait(&session, |state| state.syncs_ended == 2);
    assert!(matches!(state.send, SendState::Failed(_)), "{state:#?}");
    assert_eq!(session.close().phase, Phase::Closed);
}

#[test]
fn a_wallet_is_not_made_when_tor_does_not_start() {
    let dir = TempDir::new("session-no-tor");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let keys_file = dir.file("w.keys");
    let without_tor = |source: WalletSource| {
        let hooks = Hooks {
            transport: Box::new(|_| {
                Err(Problem {
                    kind: ProblemKind::TorFailed,
                    detail: "no Tor in this test".into(),
                })
            }),
            wallet: Box::new(|_| {}),
            save_every: crate::session::SAVE_EVERY,
            send_settles_after: crate::session::SEND_SETTLES_AFTER,
        };
        Session::start_with(config.clone(), source, PASSWORD, hooks)
    };

    // Tor fails to start. No wallet is left behind that nobody has seen
    // the recovery phrase of, and that would be in the way of trying
    // again.
    let session = without_tor(WalletSource::Create);
    let state = opened(&session);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::TorFailed));
    drop(session);
    assert!(!config.wallet_path.exists());
    assert!(!keys_file.exists());

    // Trying again, with Tor, makes the wallet.
    let stand_in = StandIn::start().unwrap();
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    assert_eq!(opened(&session).phase, Phase::Ready);
    let phrase = session.recovery_phrase().unwrap();
    assert_eq!(session.close().phase, Phase::Closed);
    assert!(keys_file.exists());

    // A wallet that is there is reported before Tor is asked for: the
    // failure is the wallet's, not Tor's.
    let session = without_tor(WalletSource::Restore {
        phrase,
        restore_height: 0,
    });
    let state = opened(&session);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::WalletExists));
}

#[test]
fn the_node_that_worked_is_the_one_to_start_with() {
    let node = |host: &str| OnionNode::new(host, 18089).unwrap();
    let (a, b, c) = (node(SYNC_HOST), node(BROADCAST_HOST), node(OWN_HOST));

    // Those before it go to the end, in the order they were in.
    let mut nodes = vec![a.clone(), b.clone(), c.clone()];
    crate::session::start_with(&mut nodes, &c);
    assert_eq!(nodes, [c.clone(), a.clone(), b.clone()]);
    crate::session::start_with(&mut nodes, &a);
    assert_eq!(nodes, [a.clone(), b.clone(), c.clone()]);
    // The first stays first, and a node that is not on the list changes
    // nothing.
    crate::session::start_with(&mut nodes, &a);
    assert_eq!(nodes, [a.clone(), b.clone(), c.clone()]);
    let mut two = vec![a.clone(), b.clone()];
    crate::session::start_with(&mut two, &c);
    assert_eq!(two, [a, b]);
}

#[test]
fn a_wallet_that_is_open_is_not_opened_by_a_second_session() {
    let dir = TempDir::new("session-twice");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let first = start(config.clone(), &stand_in, WalletSource::Create);
    let address = opened(&first).receive_address;

    // A second window of the app on the same wallet. It is told that the
    // wallet is open elsewhere, and nothing is started for it: two
    // sessions on one wallet's files would overwrite each other.
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&started);
    let transport = stand_in.clone();
    let hooks = Hooks {
        transport: Box::new(move |_| {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(Box::new(transport) as Box<dyn Transport>)
        }),
        wallet: Box::new(|_| {}),
        save_every: crate::session::SAVE_EVERY,
        send_settles_after: crate::session::SEND_SETTLES_AFTER,
    };
    let second = Session::start_with(config.clone(), WalletSource::Open, PASSWORD, hooks);
    let state = opened(&second);
    assert_eq!(failed_with(&state.phase), Some(ProblemKind::WalletInUse));
    assert!(!started.load(std::sync::atomic::Ordering::SeqCst));
    drop(second);

    // The first goes on as if nothing had happened, and once it is closed
    // the wallet opens again.
    first.new_address();
    wait(&first, |state| {
        state.receive_address.as_ref().is_some_and(|a| a.index == 2)
    });
    assert_eq!(first.close().phase, Phase::Closed);
    let again = start(config, &stand_in, WalletSource::Open);
    let state = opened(&again);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert!(!state.rebuilt_from_keys);
    assert_ne!(state.receive_address, address);
}

#[test]
fn a_wallet_that_cannot_be_saved_says_so_until_it_can() {
    let dir = TempDir::new("session-not-saved");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    assert_eq!(opened(&session).not_saved, None);
    let own = |port| vec![OnionNode::new(OWN_HOST, port).unwrap()];

    // The wallet library writes the data file under another name first
    // and then gives it the wallet's. A folder of that other name is in
    // its way on every system, whoever runs the test.
    let in_the_way = dir.file("w.new");
    std::fs::create_dir(&in_the_way).unwrap();

    // Setting nodes saves the wallet. It cannot, and the state says so
    // for as long as that is true, not only until the next command.
    session.set_nodes(own(18081));
    let state = wait(&session, |state| state.not_saved.is_some());
    assert_eq!(state.own_nodes, own(18081));
    session.new_address();
    let state = wait(&session, |state| {
        state.receive_address.as_ref().is_some_and(|a| a.index == 2)
    });
    assert!(state.not_saved.is_some(), "{state:#?}");

    // Once it can be written again, the next save clears it.
    std::fs::remove_dir(&in_the_way).unwrap();
    session.set_nodes(own(18089));
    let state = wait(&session, |state| state.own_nodes == own(18089));
    let state = if state.not_saved.is_some() {
        wait(&session, |state| state.not_saved.is_none())
    } else {
        state
    };
    assert_eq!(state.not_saved, None);

    // A wallet that cannot be saved on closing says that too.
    std::fs::create_dir(&in_the_way).unwrap();
    let state = session.close();
    assert!(matches!(state.phase, Phase::Failed(_)), "{state:#?}");
    std::fs::remove_dir(&in_the_way).unwrap();
}

#[test]
fn a_damaged_data_file_is_reported_and_the_wallet_opens() {
    let dir = TempDir::new("session-damaged");
    let Some(config) = config(&dir, "w") else {
        return;
    };
    let stand_in = StandIn::start().unwrap();
    let session = start(config.clone(), &stand_in, WalletSource::Create);
    let address = opened(&session).receive_address;
    drop(session);

    let intact = std::fs::read(&config.wallet_path).unwrap();
    std::fs::write(&config.wallet_path, &intact[..intact.len() / 2]).unwrap();

    // Told to synchronise by itself, as the app tells it.
    let mut by_itself = config;
    by_itself.sync_every = Some(Duration::from_millis(100));
    let session = start(by_itself, &stand_in, WalletSource::Open);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert!(state.rebuilt_from_keys);
    assert_eq!(state.receive_address, address);

    // With the data file went the user's own nodes, if there were any.
    // The wallet does not go to the public ones by itself: it waits to be
    // told to synchronise, or to be given nodes.
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(session.state().sync, SyncState::NotYet);
    assert!(stand_in.requests().is_empty(), "{:?}", stand_in.requests());
    session.sync();
    wait(&session, |state| state.syncs_ended >= 1);
    assert!(!stand_in.requests().is_empty());
    // From then on it synchronises by itself again.
    wait(&session, |state| state.syncs_ended >= 2);
}

/// The session with funds, on a private chain. Needs a Monero daemon
/// (`VEIL_MONEROD`), so it runs on the owner's machine and never in hosted
/// CI.
#[cfg(feature = "regtest")]
mod on_a_private_chain {
    use veil_test_guard::RegtestDaemon;
    use veil_tor::{AfterCut, CutPoint};
    use veil_wallet::{Direction, Node, Wallet};

    use super::super::wallet_library;
    use super::*;

    /// As in the core's own scenario: enough blocks to build rings from,
    /// and the confirmations that unlock received funds.
    const INITIAL_BLOCKS: u64 = 130;
    const UNLOCK_BLOCKS: u64 = 10;
    /// More than the wallet sends to open an exchange with a node and to
    /// ask whether it is there, which it does before it hands a
    /// transaction over; less than those and a transaction together.
    const MID_TRANSACTION: CutPoint = CutPoint::AfterRequestBytes(2800);

    /// The same place, for a cut that stops the transaction there: what
    /// came before it, the opening of the exchange and the question
    /// whether the node is there, has arrived, and the transaction has
    /// not.
    const CUT_OFF_MID_TRANSACTION: CutPoint = CutPoint::BeforeMoreRequestBytes(2800);

    /// A session whose wallet scans from the first block of the private
    /// chain and accepts that chain's rules, each time it is opened.
    fn start_on_chain(config: SessionConfig, stand_in: &StandIn, source: WalletSource) -> Session {
        start_on_chain_saving(config, stand_in, source, crate::session::SAVE_EVERY)
    }

    /// Like [`start_on_chain`], with a session that saves this often
    /// during a synchronisation.
    fn start_on_chain_saving(
        config: SessionConfig,
        stand_in: &StandIn,
        source: WalletSource,
        save_every: Duration,
    ) -> Session {
        let stand_in = stand_in.clone();
        let hooks = Hooks {
            transport: Box::new(move |_| Ok(Box::new(stand_in) as Box<dyn Transport>)),
            wallet: Box::new(|wallet| {
                wallet.set_restore_height(0);
                wallet.accept_regtest_chain();
            }),
            save_every,
            // On a private chain a transaction that went out is in the
            // node's pool at once; nothing has to be waited for.
            send_settles_after: Duration::ZERO,
        };
        Session::start_with(config, source, PASSWORD, hooks)
    }

    /// Synchronises and waits for the outcome of this synchronisation.
    fn synchronised(session: &Session) -> SessionState {
        let before = session.state().syncs_ended;
        session.sync();
        let state = wait(session, |state| state.syncs_ended > before);
        assert!(
            matches!(state.sync, SyncState::Done { progress } if progress.blocks_left() == 0),
            "{:?}",
            state.sync
        );
        state
    }

    fn outgoing(state: &SessionState) -> usize {
        state
            .history
            .iter()
            .filter(|transfer| transfer.direction == Direction::Outgoing && !transfer.failed)
            .count()
    }

    /// A chain long enough to arrive in several batches: a node hands out
    /// a thousand blocks at a time.
    const LONG_CHAIN_BLOCKS: u64 = 3300;
    /// What the stand-in lets through in a second while a long scan is
    /// tested. A thousand blocks of this chain are a few hundred kilobytes,
    /// so a batch takes seconds, as it does over Tor.
    const SLOW_LINE: usize = 64 * 1024;
    /// Longer than a scan of the long chain takes over the slow line.
    const A_WHILE: Duration = Duration::from_secs(300);

    fn wait_a_while(session: &Session, wanted: impl Fn(&SessionState) -> bool) -> SessionState {
        match session.wait_for(A_WHILE, wanted) {
            Some(state) => state,
            None => panic!("the session never got there: {:#?}", session.state()),
        }
    }

    /// When a file was last written, and its size; `None` in the moment
    /// in which it is being replaced.
    fn written(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
        let file = std::fs::metadata(path).ok()?;
        Some((file.modified().ok()?, file.len()))
    }

    /// Whether the synchronisation under way has blocks behind it and
    /// blocks ahead of it.
    fn in_the_middle(state: &SessionState) -> bool {
        matches!(
            state.sync,
            SyncState::Running { progress: Some(progress) } if progress.blocks_left() > 0
        )
    }

    #[test]
    fn a_long_scan_is_saved_on_the_way_gives_way_and_misses_nothing() {
        veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
        let Some(lib) = wallet_library() else { return };
        let Some(daemon) = RegtestDaemon::start() else {
            return;
        };
        let dir = TempDir::new("session-long-scan");
        let Some(config) = config(&dir, "user") else {
            return;
        };
        let direct = Node::LocalRegtest {
            port: daemon.rpc_port(),
        };
        let stand_in = StandIn::start().unwrap();
        stand_in.route(SYNC_HOST, SYNC_PORT, daemon.rpc_port());
        stand_in.route(BROADCAST_HOST, BROADCAST_PORT, daemon.rpc_port());

        // The user's wallet, known only by its address and its phrase until
        // the chain has grown.
        let session = start_on_chain(config.clone(), &stand_in, WalletSource::Create);
        let state = opened(&session);
        assert_eq!(state.phase, Phase::Ready, "{state:#?}");
        let address = state.receive_address.clone().unwrap().address;
        let phrase = session.recovery_phrase().unwrap();
        assert_eq!(session.close().phase, Phase::Closed);
        // The keys file is written once, at creation. Its copy for the
        // wallet "killed" further down is taken now: while a wallet is
        // open the library keeps that file locked, and on Windows a locked
        // file cannot even be read by anyone else.
        std::fs::copy(dir.file("user.keys"), dir.file("killed.keys")).unwrap();

        // It is paid early, and then the chain grows long.
        let (mut funder, _) =
            Wallet::create(&lib, &dir.file("funder"), PASSWORD, Network::Mainnet).unwrap();
        funder.set_restore_height(0);
        funder.connect(&direct).unwrap();
        let funder_address = funder.primary_address().unwrap();
        daemon.add_blocks(INITIAL_BLOCKS, &funder_address);
        funder.connect(&direct).unwrap();
        funder.refresh().unwrap();
        let funded: Amount = "5".parse().unwrap();
        funder
            .prepare_send(&address, SendAmount::Exact(funded))
            .unwrap()
            .commit()
            .unwrap();
        daemon.add_blocks(LONG_CHAIN_BLOCKS, &funder_address);
        let chain = INITIAL_BLOCKS + LONG_CHAIN_BLOCKS;

        // --- Saved on the way -------------------------------------------------
        // This session saves as soon as it has scanned anything, again and
        // again, where the app would save every few minutes.
        stand_in.limit_replies(Some(SLOW_LINE));
        let data_file = dir.file("user");
        let at_start = written(&data_file).expect("the wallet's data file");
        let session = start_on_chain_saving(
            config.clone(),
            &stand_in,
            WalletSource::Open,
            Duration::ZERO,
        );
        assert_eq!(opened(&session).phase, Phase::Ready);
        session.sync();
        let saved = (0..A_WHILE.as_millis() / 20).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            written(&data_file).is_some_and(|now| now != at_start)
        });
        assert!(saved, "nothing was saved: {:#?}", session.state());
        // What a kill at this moment would leave behind: this data file,
        // next to the keys file copied above.
        std::fs::copy(&data_file, dir.file("killed")).unwrap();
        let state = session.state();
        assert_eq!(state.syncs_ended, 0, "saved only at the end: {state:#?}");

        // --- Giving way to a command ----------------------------------------
        // The phrase is handed out in the middle of the scan, which then
        // goes on.
        assert_eq!(session.recovery_phrase().unwrap().as_str(), phrase.as_str());
        let state = session.state();
        assert_eq!(state.syncs_ended, 0, "the phrase waited: {state:#?}");
        assert!(
            matches!(state.sync, SyncState::Running { .. }),
            "{state:#?}"
        );

        // --- Nothing missed ---------------------------------------------------
        // Stopped and continued many times over, the scan arrives where an
        // uninterrupted one does.
        let state = wait_a_while(&session, |state| state.syncs_ended == 1);
        assert!(
            matches!(state.sync, SyncState::Done { progress } if progress.blocks_left() == 0),
            "{:?}",
            state.sync
        );
        assert_eq!(state.balance.total, funded);
        assert_eq!(state.balance.unlocked, funded);
        assert_eq!(state.history.len(), 1);
        assert_eq!(session.close().phase, Phase::Closed);

        // The copy taken on the way opens as a wallet that has part of the
        // chain behind it: that much a kill would not have cost.
        let killed = Wallet::open(&lib, &dir.file("killed"), PASSWORD, Network::Mainnet).unwrap();
        assert!(!killed.was_rebuilt_from_keys());
        let kept = killed.sync_progress().wallet_height;
        assert!(
            kept >= 500 && kept < chain,
            "the copy has {kept} of {chain} blocks"
        );
        drop(killed);

        // --- Locked in the middle, continued later ---------------------------
        // The same wallet again, restored from its phrase into a new file.
        let mut again = config.clone();
        again.wallet_path = dir.file("again");
        let session = start_on_chain(
            again.clone(),
            &stand_in,
            WalletSource::Restore {
                phrase,
                restore_height: 0,
            },
        );
        assert_eq!(opened(&session).phase, Phase::Ready);
        session.sync();
        wait_a_while(&session, in_the_middle);
        let state = session.close();
        assert_eq!(state.phase, Phase::Closed, "{state:#?}");
        assert_eq!(state.syncs_ended, 0, "closing waited: {state:#?}");

        // What it had scanned was saved on closing.
        let locked = Wallet::open(&lib, &dir.file("again"), PASSWORD, Network::Mainnet).unwrap();
        let kept = locked.sync_progress().wallet_height;
        assert!(
            kept >= 500 && kept < chain,
            "the locked wallet has {kept} of {chain} blocks"
        );
        drop(locked);

        // Opened again, it scans the rest and has the payment.
        stand_in.limit_replies(None);
        let session = start_on_chain(again, &stand_in, WalletSource::Open);
        assert_eq!(opened(&session).phase, Phase::Ready);
        let state = synchronised(&session);
        assert_eq!(state.balance.total, funded);
        assert_eq!(state.history.len(), 1);
        assert_eq!(session.close().phase, Phase::Closed);
    }

    #[test]
    fn receives_reviews_sends_and_survives_a_lost_answer() {
        veil_test_guard::refuse_in_hosted_ci("starting a Monero daemon");
        let Some(lib) = wallet_library() else { return };
        let Some(daemon) = RegtestDaemon::start() else {
            return;
        };
        let dir = TempDir::new("session-chain");
        let Some(config) = config(&dir, "user") else {
            return;
        };
        let direct = Node::LocalRegtest {
            port: daemon.rpc_port(),
        };
        let stand_in = StandIn::start().unwrap();
        stand_in.route(SYNC_HOST, SYNC_PORT, daemon.rpc_port());
        stand_in.route(BROADCAST_HOST, BROADCAST_PORT, daemon.rpc_port());

        // Whoever pays the user and mines every block, and whoever the user
        // pays: wallets of their own that reach the daemon directly.
        let (mut funder, _) =
            Wallet::create(&lib, &dir.file("funder"), PASSWORD, Network::Mainnet).unwrap();
        funder.set_restore_height(0);
        funder.connect(&direct).unwrap();
        let funder_address = funder.primary_address().unwrap();
        daemon.add_blocks(INITIAL_BLOCKS, &funder_address);
        funder.connect(&direct).unwrap();
        funder.refresh().unwrap();
        let (mut payee, _) =
            Wallet::create(&lib, &dir.file("payee"), PASSWORD, Network::Mainnet).unwrap();
        payee.set_restore_height(0);
        payee.connect(&direct).unwrap();
        let payee_address = payee.receive_address().unwrap().address;

        let session = start_on_chain(config.clone(), &stand_in, WalletSource::Create);
        let state = opened(&session);
        assert_eq!(state.phase, Phase::Ready, "{state:#?}");
        let first_address = state.receive_address.clone().unwrap();

        // --- Receiving -------------------------------------------------------
        let funded: Amount = "5".parse().unwrap();
        funder
            .prepare_send(&first_address.address, SendAmount::Exact(funded))
            .unwrap()
            .commit()
            .unwrap();
        daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);

        let state = synchronised(&session);
        assert_eq!(state.balance.total, funded);
        assert_eq!(state.balance.unlocked, funded);
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].direction, Direction::Incoming);
        assert_eq!(state.history[0].amount, funded);
        assert_eq!(
            state.node,
            Some(OnionNode::new(SYNC_HOST, SYNC_PORT).unwrap())
        );
        // The paid address has been replaced by the next one.
        assert_eq!(state.receive_address.as_ref().map(|a| a.index), Some(2));

        // --- A send the wallet cannot afford ----------------------------------
        session.prepare_send(&payee_address, SendAmount::Exact("50".parse().unwrap()));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        let SendState::Failed(problem) = &state.send else {
            panic!("fifty were offered out of five: {:?}", state.send);
        };
        assert_eq!(problem.kind, ProblemKind::NotEnoughFunds, "{problem:?}");
        session.cancel_send();
        wait(&session, |state| state.send == SendState::Idle);

        // --- A send that is reviewed and cancelled --------------------------
        let amount: Amount = "1.5".parse().unwrap();
        stand_in.clear_requests();
        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        let SendState::Review {
            to,
            amount: shown,
            fee: shown_fee,
            total,
            source_addresses,
        } = state.send.clone()
        else {
            panic!("the transaction was not built: {:?}", state.send);
        };
        assert_eq!(to, payee_address);
        assert_eq!(shown, amount);
        assert!(!shown_fee.is_zero());
        assert_eq!(Some(total), amount.checked_add(shown_fee));
        // Everything the wallet has came in at its first address.
        assert_eq!(source_addresses, [1]);
        // While a transaction is under review, other commands wait.
        session.new_address();
        let state = wait(&session, |state| state.last_problem.is_some());
        assert_eq!(
            state.last_problem.map(|problem| problem.kind),
            Some(ProblemKind::NotNow)
        );
        session.cancel_send();
        let state = wait(&session, |state| state.send == SendState::Idle);
        assert_eq!(state.balance.total, funded);
        let requests = stand_in.requests();
        assert!(
            requests.iter().all(|r| r.purpose == Purpose::Sync),
            "a cancelled send must not reach the broadcast listener: {requests:?}"
        );

        // --- A send that is confirmed ----------------------------------------
        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        let SendState::Review { fee, .. } = state.send else {
            panic!("the transaction was not built: {:?}", state.send);
        };
        stand_in.clear_requests();
        // Confirmed, and cancelled in the same breath, as a second click
        // can do before the screen has caught up: the transaction goes
        // out, and the cancel that comes too late does not take "sent"
        // off the state. The password is asked about only to know that
        // the session has dealt with the cancel.
        session.confirm_send();
        session.cancel_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        let SendState::Sent { txids } = state.send.clone() else {
            panic!("the transaction was not sent: {:?}", state.send);
        };
        assert_eq!(txids.len(), 1);
        assert_eq!(session.is_password(PASSWORD), Ok(true));
        assert_eq!(
            session.state().send,
            SendState::Sent {
                txids: txids.clone()
            },
            "a late cancel took a sent transaction off the state"
        );
        let requests = stand_in.requests();
        assert!(
            requests
                .iter()
                .any(|r| r.purpose == Purpose::Broadcast && r.host == BROADCAST_HOST),
            "{requests:?}"
        );
        session.clear_send();
        wait(&session, |state| state.send == SendState::Idle);

        daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
        let state = synchronised(&session);
        let after_first = funded
            .checked_sub(amount)
            .and_then(|rest| rest.checked_sub(fee))
            .unwrap();
        assert_eq!(state.balance.total, after_first);
        assert_eq!(state.balance.unlocked, after_first);
        let sent = state
            .history
            .iter()
            .find(|transfer| txids.contains(&transfer.txid))
            .expect("the sent transaction is in the history");
        assert_eq!(sent.direction, Direction::Outgoing);
        assert_eq!(sent.amount, amount);
        assert_eq!(sent.fee, fee);

        // --- A send whose answer is lost --------------------------------------
        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        let SendState::Review {
            fee: second_fee, ..
        } = state.send
        else {
            panic!("the transaction was not built: {:?}", state.send);
        };
        stand_in.cut_once(
            BROADCAST_HOST,
            BROADCAST_PORT,
            MID_TRANSACTION,
            AfterCut::NodeReachable,
        );
        session.confirm_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        let SendState::OutcomeUnknown {
            txids: lost,
            settled,
        } = state.send.clone()
        else {
            panic!("the send was not reported as unknown: {:?}", state.send);
        };
        assert!(!settled);
        assert_eq!(lost.len(), 1);
        assert_eq!(stand_in.cuts(), 1, "the send was not cut");
        // Nothing is recorded yet.
        assert_eq!(state.balance.total, after_first);
        assert_eq!(outgoing(&state), 1);

        // Until the wallet has looked for it, it builds no other send: a
        // second one could pay twice.
        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| state.last_problem.is_some());
        assert_eq!(
            state.last_problem.map(|problem| problem.kind),
            Some(ProblemKind::NotNow)
        );
        assert!(matches!(state.send, SendState::OutcomeUnknown { .. }));

        // The next synchronisation finds it among the transactions the
        // node is holding, before any block, and says that it was sent.
        let state = synchronised(&session);
        assert_eq!(
            state.send,
            SendState::Sent {
                txids: lost.clone()
            }
        );
        assert_eq!(outgoing(&state), 2);
        session.clear_send();
        wait(&session, |state| state.send == SendState::Idle);

        // Once it is in a block, the funds are what one payment leaves.
        daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
        let state = synchronised(&session);
        let after_second = after_first
            .checked_sub(amount)
            .and_then(|rest| rest.checked_sub(second_fee))
            .unwrap();
        assert_eq!(state.balance.total, after_second);
        assert_eq!(outgoing(&state), 2);

        // --- A send that never reaches the node -------------------------------
        // The node says that it is there, and the connection is cut while
        // the transaction is on its way to it: part of it arrives, which
        // is nothing a node can use. The wallet library says the same for
        // it as for a lost answer, so it is reported the same way:
        // unknown.
        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        assert!(
            matches!(state.send, SendState::Review { .. }),
            "the transaction was not built: {:?}",
            state.send
        );
        stand_in.cut_once(
            BROADCAST_HOST,
            BROADCAST_PORT,
            CUT_OFF_MID_TRANSACTION,
            AfterCut::NodeReachable,
        );
        session.confirm_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        let SendState::OutcomeUnknown { txids: never, .. } = state.send.clone() else {
            panic!("the send was not reported as unknown: {:?}", state.send);
        };
        assert_eq!(stand_in.cuts(), 2, "the transaction was not cut off");

        // The wallet has written that down already, not only in memory: a
        // copy of its data file as it is on disk now is what a killed app
        // or a power cut would leave. The password is asked about only to
        // know that the session has finished with the send. The keys file
        // is locked while the wallet is open, so its copy is made further
        // down, once the wallet is closed; it does not change.
        assert_eq!(session.is_password(PASSWORD), Ok(true));
        assert_eq!(session.state().not_saved, None);
        std::fs::copy(dir.file("user"), dir.file("killed")).unwrap();

        // Closing the app does not make the wallet forget it: the next
        // session knows of the same send, and that it is still open.
        assert_eq!(session.close().phase, Phase::Closed);
        std::fs::copy(dir.file("user.keys"), dir.file("killed.keys")).unwrap();
        let mut as_killed = config.clone();
        as_killed.wallet_path = dir.file("killed");
        let killed = start_on_chain(as_killed, &stand_in, WalletSource::Open);
        let state = opened(&killed);
        assert_eq!(state.phase, Phase::Ready, "{state:#?}");
        assert!(!state.rebuilt_from_keys);
        assert_eq!(
            state.send,
            SendState::OutcomeUnknown {
                txids: never.clone(),
                settled: false
            },
            "the wallet as a kill would have left it does not know of the send"
        );
        assert_eq!(killed.close().phase, Phase::Closed);

        let session = start_on_chain(config.clone(), &stand_in, WalletSource::Open);
        let state = opened(&session);
        assert_eq!(
            state.send,
            SendState::OutcomeUnknown {
                txids: never.clone(),
                settled: false
            }
        );

        // A synchronisation does not find it. On this chain there is
        // nothing to wait for, so that settles it: still not called failed,
        // but the wallet takes a new send from here on.
        let state = synchronised(&session);
        assert_eq!(
            state.send,
            SendState::OutcomeUnknown {
                txids: never,
                settled: true
            }
        );
        assert_eq!(state.balance.total, after_second);
        assert_eq!(outgoing(&state), 2);

        session.prepare_send(&payee_address, SendAmount::Exact(amount));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        let SendState::Review { fee: third_fee, .. } = state.send else {
            panic!("the transaction was not built: {:?}", state.send);
        };
        // This time the node for sending is down, as public nodes often
        // are. It is asked whether it is there before anything is handed
        // to it, does not answer, and the transaction goes to the next
        // node that does: here the one synchronised with, there being no
        // third, through the broadcast listener all the same. Sent, and
        // nothing unknown about it.
        stand_in.unroute(BROADCAST_HOST, BROADCAST_PORT);
        stand_in.clear_requests();
        session.confirm_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        assert!(
            matches!(state.send, SendState::Sent { .. }),
            "the transaction was not sent: {:?}",
            state.send
        );
        let mut asked: Vec<String> = stand_in
            .requests()
            .into_iter()
            .filter(|request| request.purpose == Purpose::Broadcast)
            .map(|request| request.host)
            .collect();
        asked.dedup();
        assert_eq!(asked, [BROADCAST_HOST, SYNC_HOST]);
        stand_in.route(BROADCAST_HOST, BROADCAST_PORT, daemon.rpc_port());
        daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
        let state = synchronised(&session);
        let in_the_end = after_second
            .checked_sub(amount)
            .and_then(|rest| rest.checked_sub(third_fee))
            .unwrap();
        assert_eq!(state.balance.total, in_the_end);
        assert_eq!(outgoing(&state), 3);
        assert_eq!(session.close().phase, Phase::Closed);

        payee.connect(&direct).unwrap();
        payee.refresh().unwrap();
        assert_eq!(
            payee.balance().total,
            amount
                .checked_add(amount)
                .and_then(|sum| sum.checked_add(amount))
                .unwrap(),
            "the payee was paid three times in all, once per send that went out"
        );

        // --- The same wallet in a new session -------------------------------
        let session = start_on_chain(config, &stand_in, WalletSource::Open);
        let state = opened(&session);
        assert_eq!(state.phase, Phase::Ready, "{state:#?}");
        assert_eq!(state.balance.total, in_the_end);
        assert_eq!(outgoing(&state), 3);
        assert_eq!(state.sync, SyncState::NotYet);

        // --- A first node that is down is not asked every time ---------------
        // The first node is gone. The synchronisation finds that out, goes
        // on to the second, and finishes with it.
        stand_in.unroute(SYNC_HOST, SYNC_PORT);
        let state = synchronised(&session);
        assert_eq!(
            state.node,
            Some(OnionNode::new(BROADCAST_HOST, BROADCAST_PORT).unwrap())
        );
        // The next one starts with the node that worked, and does not
        // spend its first while on the one that did not.
        stand_in.clear_requests();
        let state = synchronised(&session);
        assert!(matches!(state.sync, SyncState::Done { .. }), "{state:#?}");
        let requests = stand_in.requests();
        assert!(!requests.is_empty());
        assert!(
            requests
                .iter()
                .all(|request| request.host == BROADCAST_HOST),
            "{requests:?}"
        );

        // --- The network goes away, and comes back ---------------------------
        // A transaction is under review when the network goes from under
        // Tor. Confirmed, it is handed to no node: each is asked whether
        // it is there, through Tor, and none can be reached. It is reported
        // as not sent, for Tor's reason and not the nodes', and that Tor
        // has lost its network is said in the same change. The amount is
        // one the wallet still has after its three payments.
        let little: Amount = "0.1".parse().unwrap();
        session.prepare_send(&payee_address, SendAmount::Exact(little));
        let state = wait(&session, |state| {
            matches!(state.send, SendState::Review { .. } | SendState::Failed(_))
        });
        assert!(
            matches!(state.send, SendState::Review { .. }),
            "the transaction was not built: {:?}",
            state.send
        );
        stand_in.set_network(false);
        session.confirm_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        let SendState::Failed(problem) = &state.send else {
            panic!(
                "with no network the send was not a failure: {:?}",
                state.send
            );
        };
        assert_eq!(problem.kind, ProblemKind::TorLost, "{problem:?}");
        assert_eq!(state.tor.state, State::Lost, "{state:#?}");
        assert_eq!(outgoing(&state), 3);
        session.clear_send();

        // The network is back. Nobody tells the wallet: the next
        // synchronisation gets through, Tor is ready again with it, and
        // the wallet holds what it held, nothing having been paid.
        stand_in.set_network(true);
        let state = synchronised(&session);
        assert_eq!(state.tor.state, State::Ready, "{state:#?}");
        assert_eq!(state.balance.total, in_the_end);
        assert_eq!(outgoing(&state), 3);
        assert_eq!(session.close().phase, Phase::Closed);
    }
}
