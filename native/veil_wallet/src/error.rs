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

    /// The wallet is open already: in another window of the app, or in
    /// another wallet program. Nothing was touched.
    #[error("the wallet is open in another program; close it there first")]
    WalletInUse,

    /// The wallet could not be marked as open, so it was not opened or
    /// made: without the mark a second program could take the same wallet.
    /// This is about the file the mark is kept in, not about the wallet.
    #[error("the wallet could not be marked as open ({file}): {reason}")]
    NotMarked { file: PathBuf, reason: String },

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

    /// The wallet cannot pay the amount and its fee from what can be spent
    /// now. Funds still waiting for confirmations do not count.
    #[error("not enough funds that can be spent now: {0}")]
    NotEnoughFunds(String),

    /// The payment does not fit into one transaction: the wallet would
    /// have to spend more coins than one transaction takes. The library
    /// would send several, one after another, and stop at the first that
    /// fails, leaving a payment that went out in part. Veil does not send
    /// such a payment at all.
    #[error(
        "the payment needs {transactions} transactions, and only payments that fit \
         into one are sent; send a smaller amount"
    )]
    SeveralTransactions { transactions: usize },

    /// None of the nodes a transaction could be handed to answered when
    /// asked whether it was there. The transaction was handed to nobody:
    /// it has not left the device, and sending again cannot pay twice.
    #[error(
        "none of the {tried} nodes the transaction could be sent through answered; \
         nothing was sent"
    )]
    NoNodeToSendThrough { tried: usize },

    /// The amount to send is zero.
    #[error("the amount must be greater than zero")]
    ZeroAmount,

    /// The wallet library refused the operation. The text comes from the
    /// library.
    #[error("{0}")]
    Wallet(String),
}
