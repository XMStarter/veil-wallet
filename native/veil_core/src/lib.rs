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

mod node;

use veil_tor::{Purpose, State, Status, TorService, Transport};
use veil_wallet::{
    Balance, Node, PendingSend, ReceiveAddress, SeedPhrase, SendAmount, SyncProgress, Transfer,
    Wallet,
};

pub use node::{Nodes, OnionNode};

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
        let status = self.transport.status();
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

    /// Goes through `candidates` in order and settles on the first node
    /// that answers and has the whole chain. Returns that node's height.
    ///
    /// The candidate after it, wrapping around, becomes the broadcast node;
    /// it is not contacted now. With a single candidate that node does both.
    /// The caller decides the order, which is how a node gets picked at
    /// random.
    pub fn settle_on_nodes(&mut self, candidates: &[OnionNode]) -> Result<u64, CoreError> {
        self.require_tor()?;
        let mut last = veil_wallet::Error::NodeUnreachable;
        for (position, sync) in candidates.iter().enumerate() {
            let broadcast = candidates
                .iter()
                .cycle()
                .skip(position + 1)
                .take(candidates.len())
                .find(|other| other.host() != sync.host());
            let nodes = match broadcast {
                Some(broadcast) => Nodes::separate(sync.clone(), broadcast.clone())?,
                None => Nodes::single(sync.clone()),
            };
            self.set_nodes(nodes)?;
            match self.wallet.node_height() {
                Ok(height) => return Ok(height),
                Err(
                    error @ (veil_wallet::Error::NodeUnreachable
                    | veil_wallet::Error::NodeIncompatible
                    | veil_wallet::Error::NodeSyncing { .. }),
                ) => last = error,
                Err(other) => return Err(other.into()),
            }
        }
        Err(CoreError::NoNodeAnswered {
            tried: candidates.len(),
            last,
        })
    }

    /// Asks the sync node for the height of its chain. Scans nothing.
    pub fn node_height(&mut self) -> Result<u64, CoreError> {
        self.require_tor()?;
        Ok(self.wallet.node_height()?)
    }

    /// Downloads and scans new blocks until the wallet has caught up with
    /// the sync node.
    pub fn sync(&mut self) -> Result<SyncProgress, CoreError> {
        self.require_tor()?;
        Ok(self.wallet.refresh()?)
    }

    /// Like [`Core::sync`], and calls `report` about once a second with how
    /// far scanning has come. `report` runs on another thread.
    pub fn sync_with_progress(
        &mut self,
        report: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncProgress, CoreError> {
        self.require_tor()?;
        Ok(self.wallet.refresh_with_progress(report)?)
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
    /// node through the broadcast listener. If Tor has stopped in the
    /// meantime, that listener refuses the connection and the commit fails;
    /// nothing leaves any other way.
    pub fn prepare_send(
        &mut self,
        to: &str,
        amount: SendAmount,
    ) -> Result<PendingSend<'_>, CoreError> {
        self.require_tor()?;
        Ok(self.wallet.prepare_send(to, amount)?)
    }

    /// The height scanning starts from.
    pub fn restore_height(&self) -> u64 {
        self.wallet.restore_height()
    }

    /// Changes where scanning starts. Lower it to find older payments.
    pub fn set_restore_height(&mut self, height: u64) {
        self.wallet.set_restore_height(height);
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

#[cfg(test)]
mod tests;
