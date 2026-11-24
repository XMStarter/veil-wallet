//! Errors reported by the wallet layer.

use std::path::PathBuf;

/// Everything that can go wrong in this crate.
///
/// Messages never contain a seed phrase or a password.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The wallet library could not be loaded, or is not the build Veil
    /// expects.
    #[error("cannot load the wallet library {path}: {reason}")]
    Library { path: PathBuf, reason: String },

    /// An argument cannot be passed to the wallet library.
    #[error("invalid {what}")]
    InvalidArgument { what: &'static str },

    /// The recovery phrase has a word count no supported format uses.
    #[error("a recovery phrase has 16 or 25 words, this one has {words}")]
    SeedWordCount { words: usize },

    /// The destination is not a valid address on this network.
    #[error("not a valid Monero address for this network")]
    InvalidAddress,

    /// The node is not an onion service.
    #[error("the node must be a .onion address")]
    NodeNotOnion,

    /// The node did not answer.
    #[error("the node cannot be reached")]
    NodeUnreachable,

    /// The node runs a protocol version this wallet cannot work with.
    #[error("the node runs an incompatible version")]
    NodeIncompatible,

    /// The node has not finished downloading the chain itself. Scanning
    /// against it would miss payments.
    #[error("the node is still synchronising ({height} of {target})")]
    NodeSyncing { height: u64, target: u64 },

    /// The password does not open this wallet.
    #[error("wrong password")]
    WrongPassword,

    /// A wallet file already exists at the target path.
    #[error("a wallet already exists at {0}")]
    WalletExists(PathBuf),

    /// Too many addresses were issued without receiving anything. Issuing
    /// more could make payments invisible after a restore.
    #[error("{unused} unused addresses already exist; use one of them first")]
    TooManyUnusedAddresses { unused: u32 },

    /// The connection was lost while a transaction was being handed to the
    /// node. The node may have received it, or not; the wallet has not
    /// recorded it either way. The next synchronisation shows it in the
    /// history if it went out, and a second send before that could pay
    /// twice.
    #[error(
        "the connection was lost while the transaction was being sent, and it may \
         have gone out; synchronise and check the history before sending again"
    )]
    SendOutcomeUnknown,

    /// The synchronisation was told to stop before the wallet had caught up
    /// with the node. Nothing is wrong: what was scanned is kept, and the
    /// next synchronisation continues from there.
    #[error("the synchronisation was stopped before it finished")]
    SyncStopped,

    /// The amount to send is zero.
    #[error("the amount must be greater than zero")]
    ZeroAmount,

    /// The wallet library refused the operation. The text comes from the
    /// library.
    #[error("{0}")]
    Wallet(String),
}
