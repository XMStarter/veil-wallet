//! The session: the core on its own thread, behind a stand-in for Tor.
//!
//! These need the wallet library (`VEIL_WALLET_LIB`) and are skipped
//! without it. Nothing here leaves the machine.

use std::time::Duration;

use veil_tor::{Bridges, Purpose, StandIn, State, TorConfig, Transport};
use veil_wallet::{Amount, Network, SendAmount, MAX_UNUSED_ADDRESSES};

use super::{TempDir, BROADCAST_HOST, BROADCAST_PORT, PASSWORD, SYNC_HOST, SYNC_PORT};
use crate::session::Hooks;
use crate::{
    OnionNode, Phase, ProblemKind, SendState, Session, SessionConfig, SessionState, SyncState,
    WalletSource,
};

/// Longer than anything here takes; a wait that runs this long has failed.
const SOON: Duration = Duration::from_secs(60);

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

    // Tor drops. The state shows it without being asked, and a
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

    // The session is still there for everything else.
    session.new_address();
    let state = wait(&session, |state| {
        state.receive_address.as_ref().is_some_and(|a| a.index == 2)
    });
    assert_eq!(state.last_problem, None);
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
    let tried_again = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        stand_in.requests().len() > first
    });
    assert!(tried_again, "no second synchronisation within ten seconds");
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

    let session = start(config, &stand_in, WalletSource::Open);
    let state = opened(&session);
    assert_eq!(state.phase, Phase::Ready, "{state:#?}");
    assert!(state.rebuilt_from_keys);
    assert_eq!(state.receive_address, address);
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
    /// More than the wallet sends to open an exchange with a node, less
    /// than a transaction.
    const MID_TRANSACTION: CutPoint = CutPoint::AfterRequestBytes(2500);

    /// A session whose wallet scans from the first block of the private
    /// chain and accepts that chain's rules, each time it is opened.
    fn start_on_chain(config: SessionConfig, stand_in: &StandIn, source: WalletSource) -> Session {
        let stand_in = stand_in.clone();
        let hooks = Hooks {
            transport: Box::new(move |_| Ok(Box::new(stand_in) as Box<dyn Transport>)),
            wallet: Box::new(|wallet| {
                wallet.set_restore_height(0);
                wallet.accept_regtest_chain();
            }),
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
        } = state.send.clone()
        else {
            panic!("the transaction was not built: {:?}", state.send);
        };
        assert_eq!(to, payee_address);
        assert_eq!(shown, amount);
        assert!(!shown_fee.is_zero());
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
        session.confirm_send();
        let state = wait(&session, |state| {
            !matches!(state.send, SendState::Review { .. } | SendState::Sending)
        });
        let SendState::Sent { txids } = state.send.clone() else {
            panic!("the transaction was not sent: {:?}", state.send);
        };
        assert_eq!(txids.len(), 1);
        let requests = stand_in.requests();
        assert!(
            requests
                .iter()
                .any(|r| r.purpose == Purpose::Broadcast && r.host == BROADCAST_HOST),
            "{requests:?}"
        );
        session.cancel_send();

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
        assert_eq!(state.send, SendState::OutcomeUnknown);
        assert_eq!(stand_in.cuts(), 1, "the send was not cut");
        // Nothing is recorded yet.
        assert_eq!(state.balance.total, after_first);
        assert_eq!(outgoing(&state), 1);

        // The next synchronisation after a block shows that it went out.
        daemon.add_blocks(UNLOCK_BLOCKS + 1, &funder_address);
        let state = synchronised(&session);
        let after_second = after_first
            .checked_sub(amount)
            .and_then(|rest| rest.checked_sub(second_fee))
            .unwrap();
        assert_eq!(state.balance.total, after_second);
        assert_eq!(outgoing(&state), 2);
        // The outcome stays on show until the interface clears it.
        assert_eq!(state.send, SendState::OutcomeUnknown);
        assert_eq!(session.close().phase, Phase::Closed);

        payee.connect(&direct).unwrap();
        payee.refresh().unwrap();
        assert_eq!(
            payee.balance().total,
            amount.checked_add(amount).unwrap(),
            "the payee was paid twice in all, once per send"
        );

        // --- The same wallet in a new session -------------------------------
        let session = start_on_chain(config, &stand_in, WalletSource::Open);
        let state = opened(&session);
        assert_eq!(state.phase, Phase::Ready, "{state:#?}");
        assert_eq!(state.balance.total, after_second);
        assert_eq!(outgoing(&state), 2);
        assert_eq!(state.sync, SyncState::NotYet);
    }
}
