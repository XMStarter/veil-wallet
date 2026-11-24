//! The core of Veil: a Monero wallet joined to the embedded Tor client.
//!
//! [`veil_wallet`] knows how to run a wallet against a node behind a SOCKS
//! proxy, and [`veil_tor`] provides such proxies that lead only into Tor.
//! This crate puts the two together and is the only way the rest of the app
//! reaches a wallet, so the rules below cannot be bypassed from outside:
//!
//! * **Tor or nothing.** A [`Core`] is built from a running [`TorService`].
//!   Its wallet is pointed at that service's loopback listeners and at no
//!   other address. While Tor is not ready, every call that needs a node
//!   fails with [`CoreError::TorNotReady`] before the wallet is asked to do
//!   anything. There is no direct connection to fall back to.
//! * **Onion nodes only.** A node is an [`OnionNode`]. Nothing else can be
//!   named.
//! * **Sync and spend apart.** The wallet synchronises through one listener
//!   and sends transactions through the other, which uses a new Tor circuit
//!   for every connection, and by default through a different node
//!   ([`Nodes::separate`]).

pub mod ffi;
mod node;
mod public_nodes;
mod session;
pub mod transports;

use veil_tor::{Purpose, State, Status, TorService, Transport};
use veil_wallet::{
    Balance, Node, PendingSend, ReceiveAddress, SeedPhrase, SendAmount, SyncProgress, Transfer,
    Wallet,
};

pub use node::{Nodes, OnionNode};
pub use public_nodes::stagenet_candidates;
pub use session::{
    Phase, Problem, ProblemKind, SendState, Session, SessionConfig, SessionState, SyncState,
    WalletSource,
};

/// Why a call on the core failed.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Tor is still connecting. Nothing was sent anywhere.
    #[error("Tor is not ready yet ({percent}%)")]
    TorNotReady {
        /// Bootstrap progress, 0 to 100.
        percent: u8,
        /// Why Tor is stuck, when known. Usually the network blocks Tor.
        message: Option<String>,
    },

    /// Tor could not start. Nothing was sent anywhere.
    #[error("Tor could not start: {}", message.as_deref().unwrap_or("no reason given"))]
    TorFailed { message: Option<String> },

    /// Not `host.onion:port`.
    #[error("a node is written as host.onion:port")]
    InvalidNode,

    /// The sync node and the broadcast node are the same onion service.
    #[error("the node for sending must differ from the node for synchronising")]
    SameNode,

    /// None of the nodes that were tried could be used.
    #[error("none of the {tried} nodes answered; the last one: {last}")]
    NoNodeAnswered {
        tried: usize,
        /// What the last node tried failed with.
        last: veil_wallet::Error,
    },

    /// The wallet reported an error.
    #[error(transparent)]
    Wallet(#[from] veil_wallet::Error),
}

/// An open wallet and the Tor client that carries all of its traffic.
///
/// Like the wallet inside it, a `Core` is not thread-safe: give it to one
/// thread and talk to that thread. Calls that contact a node block until the
/// node has answered.
pub struct Core {
    // Declared before `transport` so that the wallet is closed first.
    wallet: Wallet,
    transport: Box<dyn Transport>,
    nodes: Nodes,
}

/// One synchronisation through a list of nodes, which can be stopped on the
/// way and continued: see [`Core::continue_sync`].
#[derive(Debug)]
pub struct SyncAttempt {
    candidates: Vec<OnionNode>,
    /// The candidate to synchronise through next. Those before it failed.
    position: usize,
    /// What the last candidate that failed, failed with.
    last: veil_wallet::Error,
}

impl SyncAttempt {
    /// An attempt that tries `candidates` in order, as
    /// [`Core::sync_with_any`] does.
    pub fn through(candidates: &[OnionNode]) -> SyncAttempt {
        SyncAttempt {
            candidates: candidates.to_vec(),
            position: 0,
            last: veil_wallet::Error::NodeUnreachable,
        }
    }
}

impl Core {
    /// Joins `wallet` to `tor`. Contacts nothing.
    ///
    /// `tor` may still be bootstrapping; calls that need a node fail with
    /// [`CoreError::TorNotReady`] until it is ready.
    pub fn new(tor: TorService, wallet: Wallet, nodes: Nodes) -> Result<Core, CoreError> {
        Core::with_transport(Box::new(tor), wallet, nodes)
    }

    fn with_transport(
        transport: Box<dyn Transport>,
        wallet: Wallet,
        nodes: Nodes,
    ) -> Result<Core, CoreError> {
        let mut core = Core {
            wallet,
            transport,
            nodes,
        };
        core.point_wallet_at_nodes()?;
        Ok(core)
    }

    /// Tells the wallet which node to reach through which listener.
    fn point_wallet_at_nodes(&mut self) -> Result<(), CoreError> {
        let sync = self.node_behind(Purpose::Sync);
        let broadcast = self.node_behind(Purpose::Broadcast);
        self.wallet.connect(&sync)?;
        self.wallet.set_broadcast_node(Some(broadcast))?;
        Ok(())
    }

    fn node_behind(&self, purpose: Purpose) -> Node {
        let node = match purpose {
            Purpose::Sync => self.nodes.sync(),
            Purpose::Broadcast => self.nodes.broadcast(),
        };
        Node::Onion {
            host: node.host().to_owned(),
            port: node.port(),
            socks_port: self.transport.socks_port(purpose),
        }
    }

    /// Fails unless Tor can carry traffic right now.
    fn require_tor(&self) -> Result<(), CoreError> {
        require_tor(self.transport.as_ref())
    }

    /// State of the Tor client, for display.
    pub fn tor_status(&self) -> Status {
        self.transport.status()
    }

    pub fn nodes(&self) -> &Nodes {
        &self.nodes
    }

    /// Switches to other nodes, for example after one stopped answering.
    /// Contacts nothing.
    pub fn set_nodes(&mut self, nodes: Nodes) -> Result<(), CoreError> {
        self.nodes = nodes;
        self.point_wallet_at_nodes()
    }

    /// The nodes to use when the candidate at `position` synchronises: the
    /// candidate after it, wrapping around, broadcasts. With no second onion
    /// service among the candidates that one node does both.
    fn nodes_from(candidates: &[OnionNode], position: usize) -> Result<Nodes, CoreError> {
        let sync = &candidates[position];
        let broadcast = candidates
            .iter()
            .cycle()
            .skip(position + 1)
            .take(candidates.len())
            .find(|other| other.host() != sync.host());
        match broadcast {
            Some(broadcast) => Nodes::separate(sync.clone(), broadcast.clone()),
            None => Ok(Nodes::single(sync.clone())),
        }
    }

    /// Goes through `candidates` in order and settles on the first node
    /// that answers and has the whole chain. Returns that node's height.
    ///
    /// The candidate after it, wrapping around, becomes the broadcast node;
    /// it is not contacted now. With a single candidate that node does both.
    /// The caller decides the order, which is how a node gets picked at
    /// random.
    pub fn settle_on_nodes(&mut self, candidates: &[OnionNode]) -> Result<u64, CoreError> {
        let mut last = veil_wallet::Error::NodeUnreachable;
        for position in 0..candidates.len() {
            self.require_tor()?;
            self.set_nodes(Self::nodes_from(candidates, position)?)?;
            match self.wallet.node_height() {
                Ok(height) => return Ok(height),
                Err(error) if is_the_nodes_fault(&error) => last = error,
                Err(other) => return Err(other.into()),
            }
        }
        self.require_tor()?;
        Err(CoreError::NoNodeAnswered {
            tried: candidates.len(),
            last,
        })
    }

    /// Asks the sync node for the height of its chain. Scans nothing.
    pub fn node_height(&mut self) -> Result<u64, CoreError> {
        self.require_tor()?;
        let transport = self.transport.as_ref();
        self.wallet
            .node_height()
            .map_err(|error| node_failure(transport, error))
    }

    /// Downloads and scans new blocks until the wallet has caught up with
    /// the sync node.
    ///
    /// If the connection is lost on the way, this fails with the wallet's
    /// [`NodeUnreachable`](veil_wallet::Error::NodeUnreachable), or with
    /// [`CoreError::TorNotReady`] when it was Tor that dropped. What was
    /// scanned until then is kept, and the next synchronisation continues
    /// from there.
    pub fn sync(&mut self) -> Result<SyncProgress, CoreError> {
        self.sync_with_progress(&mut |_| {})
    }

    /// Like [`Core::sync`], and calls `report` about once a second with how
    /// far scanning has come. `report` runs on another thread.
    pub fn sync_with_progress(
        &mut self,
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, CoreError> {
        self.require_tor()?;
        let transport = self.transport.as_ref();
        self.wallet
            .refresh_with_progress(report)
            .map_err(|error| node_failure(transport, error))
    }

    /// Synchronises through the first of `candidates` that carries the
    /// synchronisation to its end, with progress reports as in
    /// [`Core::sync_with_progress`].
    ///
    /// A node that cannot be reached, is not usable, or stops answering part
    /// way is left for the next candidate, which continues where the scan
    /// stopped. Nodes are chosen as in [`Core::settle_on_nodes`], and the
    /// core stays on the node that finished. If Tor itself drops, no further
    /// node is tried and the outcome is [`CoreError::TorNotReady`].
    pub fn sync_with_any(
        &mut self,
        candidates: &[OnionNode],
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, CoreError> {
        self.sync_with_any_until(candidates, &|| false, report)
    }

    /// Like [`Core::sync_with_any`], and gives up as soon as `stop` says
    /// so, with the wallet's
    /// [`SyncStopped`](veil_wallet::Error::SyncStopped). See
    /// [`Core::continue_sync`], which this is one call of.
    pub fn sync_with_any_until(
        &mut self,
        candidates: &[OnionNode],
        stop: &(dyn Fn() -> bool + Sync),
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, CoreError> {
        self.continue_sync(&mut SyncAttempt::through(candidates), stop, report)
    }

    /// Goes on with `attempt` until the wallet is level with a node, every
    /// candidate left has failed, or `stop` says to stop.
    ///
    /// `stop` is asked often, on another thread. When it says yes, this
    /// ends with the wallet's
    /// [`SyncStopped`](veil_wallet::Error::SyncStopped) and tries no
    /// further node; see [`Wallet::refresh_until`] for how soon. Everything
    /// scanned so far is kept in the open wallet, and [`Core::save`] writes
    /// it to disk. Calling this again with the same `attempt` continues
    /// through the node it was stopped at: candidates that have failed are
    /// not gone back to, so an attempt that is stopped again and again
    /// still comes to an end.
    pub fn continue_sync(
        &mut self,
        attempt: &mut SyncAttempt,
        stop: &(dyn Fn() -> bool + Sync),
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, CoreError> {
        while attempt.position < attempt.candidates.len() {
            self.require_tor()?;
            self.set_nodes(Self::nodes_from(&attempt.candidates, attempt.position)?)?;
            match self.wallet.refresh_until(stop, report) {
                Ok(progress) => return Ok(progress),
                Err(error) if is_the_nodes_fault(&error) => {
                    attempt.last = error;
                    attempt.position += 1;
                }
                Err(other) => return Err(other.into()),
            }
        }
        self.require_tor()?;
        Err(CoreError::NoNodeAnswered {
            tried: attempt.candidates.len(),
            last: std::mem::replace(&mut attempt.last, veil_wallet::Error::NodeUnreachable),
        })
    }

    /// How far the last synchronisation got. Contacts nothing.
    pub fn sync_progress(&self) -> SyncProgress {
        self.wallet.sync_progress()
    }

    /// Funds as of the last synchronisation.
    pub fn balance(&self) -> Balance {
        self.wallet.balance()
    }

    /// The address to show on the Receive screen. See
    /// [`Wallet::receive_address`].
    pub fn receive_address(&mut self) -> Result<ReceiveAddress, CoreError> {
        Ok(self.wallet.receive_address()?)
    }

    /// Issues an additional address although the current one is unused. See
    /// [`Wallet::new_receive_address`].
    pub fn new_receive_address(&mut self) -> Result<ReceiveAddress, CoreError> {
        Ok(self.wallet.new_receive_address()?)
    }

    /// The wallet's transfers as of the last synchronisation, oldest first.
    pub fn history(&mut self) -> Result<Vec<Transfer>, CoreError> {
        Ok(self.wallet.history()?)
    }

    pub fn is_valid_address(&self, address: &str) -> bool {
        self.wallet.is_valid_address(address)
    }

    /// Builds and signs a transaction without sending it, so the user can
    /// review the fee first. Asks the sync node for what a transaction needs.
    ///
    /// [`PendingSend::commit`] then hands the transaction to the broadcast
    /// node through the broadcast listener, and
    /// [`PendingSend::commit_through`] to the first node of
    /// [`Core::broadcast_route`] that answers. If Tor has stopped in the
    /// meantime, that listener refuses the connection and the commit fails;
    /// nothing leaves any other way.
    pub fn prepare_send(
        &mut self,
        to: &str,
        amount: SendAmount,
    ) -> Result<PendingSend<'_>, CoreError> {
        self.require_tor()?;
        let transport = self.transport.as_ref();
        self.wallet
            .prepare_send(to, amount)
            .map_err(|error| node_failure(transport, error))
    }

    /// The nodes a transaction may be handed to, in the order to try
    /// them, each behind the broadcast listener: for
    /// [`PendingSend::commit_through`], which hands the transaction to
    /// the first that answers.
    ///
    /// `preferred` comes first, if there is one: the node the last send
    /// of the session went through. Then the broadcast node in place,
    /// then every other candidate that is another service than the sync
    /// node. The sync node itself comes last: if nothing else answers,
    /// the transaction goes through it, on a circuit of its own, rather
    /// than not at all. A node synchronised with can tell that somebody
    /// spends, which is what the separate node is there to prevent, but
    /// a payment that cannot be sent is the worse failure.
    pub fn broadcast_route(
        &self,
        candidates: &[OnionNode],
        preferred: Option<&OnionNode>,
    ) -> Vec<(OnionNode, Node)> {
        let sync = self.nodes.sync();
        let mut route: Vec<OnionNode> = Vec::new();
        let others = preferred
            .into_iter()
            .chain(std::iter::once(self.nodes.broadcast()))
            .chain(candidates)
            .filter(|node| node.host() != sync.host());
        for node in others.chain(std::iter::once(sync)) {
            if !route.contains(node) {
                route.push(node.clone());
            }
        }
        let socks_port = self.transport.socks_port(Purpose::Broadcast);
        route
            .into_iter()
            .map(|node| {
                let behind = Node::Onion {
                    host: node.host().to_owned(),
                    port: node.port(),
                    socks_port,
                };
                (node, behind)
            })
            .collect()
    }

    /// Which node of [`Core::broadcast_route`] a transaction would be
    /// handed to right now: the first that answers through the broadcast
    /// listener. `None` if none does. Sends nothing.
    pub fn broadcast_node_that_answers(
        &mut self,
        candidates: &[OnionNode],
    ) -> Result<Option<OnionNode>, CoreError> {
        self.require_tor()?;
        let (names, nodes): (Vec<OnionNode>, Vec<Node>) =
            self.broadcast_route(candidates, None).into_iter().unzip();
        let found = self.wallet.first_that_answers(&nodes)?;
        Ok(found.map(|position| names[position].clone()))
    }

    /// Whether the wallet was opened from its keys alone because its data
    /// file could not be read. See [`Wallet::was_rebuilt_from_keys`].
    pub fn was_rebuilt_from_keys(&self) -> bool {
        self.wallet.was_rebuilt_from_keys()
    }

    /// The height scanning starts from.
    pub fn restore_height(&self) -> u64 {
        self.wallet.restore_height()
    }

    /// Changes where scanning starts, until the wallet is closed. See
    /// [`Wallet::set_restore_height`]: the change is not written to disk.
    pub fn set_restore_height(&mut self, height: u64) {
        self.wallet.set_restore_height(height);
    }

    /// Whether `password` is the wallet's. See [`Wallet::is_password`].
    pub fn is_password(&self, password: &str) -> bool {
        self.wallet.is_password(password)
    }

    /// Gives the wallet a new password. See [`Wallet::change_password`].
    pub fn change_password(&mut self, new: &str) -> Result<(), CoreError> {
        Ok(self.wallet.change_password(new)?)
    }

    /// A short text kept in the wallet's data file. See [`Wallet::note`].
    pub fn note(&self, key: &str) -> Result<String, CoreError> {
        Ok(self.wallet.note(key)?)
    }

    /// Sets the text kept under `key`; the next [`Core::save`] writes it to
    /// disk. See [`Wallet::set_note`].
    pub fn set_note(&mut self, key: &str, value: &str) -> Result<(), CoreError> {
        Ok(self.wallet.set_note(key, value)?)
    }

    /// The wallet's recovery phrase, in the format it was created with.
    pub fn seed_phrase(&self) -> Result<SeedPhrase, CoreError> {
        Ok(self.wallet.seed_phrase()?)
    }

    /// The wallet's keys as a classic 25-word Monero phrase.
    pub fn legacy_seed_phrase(&self) -> Result<SeedPhrase, CoreError> {
        Ok(self.wallet.legacy_seed_phrase()?)
    }

    /// Writes the wallet to disk.
    pub fn save(&mut self) -> Result<(), CoreError> {
        Ok(self.wallet.save()?)
    }

    /// Saves and closes the wallet, then stops Tor.
    pub fn close(self) -> Result<(), CoreError> {
        let Core {
            wallet, transport, ..
        } = self;
        let closed = wallet.close();
        drop(transport);
        Ok(closed?)
    }

    /// Makes the wallet forget what it remembers about its node.
    ///
    /// The wallet library keeps a node's height for half a minute. On a
    /// public network that is harmless; on a private chain blocks appear on
    /// request and a test needs the new height at once.
    #[cfg(feature = "regtest")]
    pub fn forget_node_state(&mut self) -> Result<(), CoreError> {
        self.point_wallet_at_nodes()
    }
}

/// Fails unless Tor can carry traffic right now.
fn require_tor(transport: &dyn Transport) -> Result<(), CoreError> {
    let status = transport.status();
    match status.state {
        State::Ready => Ok(()),
        State::Bootstrapping => Err(CoreError::TorNotReady {
            percent: status.percent,
            message: status.message,
        }),
        State::Failed => Err(CoreError::TorFailed {
            message: status.message,
        }),
    }
}

/// What to report when the wallet failed to get something from a node. If
/// Tor has stopped being ready, that is the reason, and the node is not to
/// blame.
fn node_failure(transport: &dyn Transport, error: veil_wallet::Error) -> CoreError {
    if matches!(error, veil_wallet::Error::NodeUnreachable) {
        if let Err(tor) = require_tor(transport) {
            return tor;
        }
    }
    error.into()
}

/// Whether `error` is a reason to try another node.
fn is_the_nodes_fault(error: &veil_wallet::Error) -> bool {
    matches!(
        error,
        veil_wallet::Error::NodeUnreachable
            | veil_wallet::Error::NodeIncompatible
            | veil_wallet::Error::NodeSyncing { .. }
    )
}

#[cfg(test)]
mod tests;
