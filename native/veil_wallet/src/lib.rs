//! Wallet operations for Veil.
//!
//! This crate is a thin, safe layer over the official Monero wallet library
//! (`wallet2`, reached through the `monero_c` C interface). Keys, addresses,
//! and transactions are produced by that library. Nothing here implements
//! cryptography.
//!
//! What this crate adds is policy:
//!
//! * a wallet never talks to a node except through a SOCKS proxy to an onion
//!   address (see [`Node`]);
//! * every incoming payment gets a fresh subaddress (see
//!   [`Wallet::receive_address`]);
//! * amounts are exact integers and "send everything" can never happen by
//!   accident (see [`Amount`] and [`SendAmount`]).
//!
//! A [`Wallet`] is not thread-safe. Give it to one thread and talk to that
//! thread.

mod amount;
mod error;
mod seed;
mod sys;
mod wallet;

pub use amount::{Amount, ParseAmountError};
pub use error::Error;
pub use seed::{SeedKind, SeedPhrase};
pub use sys::WalletLibrary;
pub use wallet::{
    Balance, Direction, Network, Node, PendingSend, ReceiveAddress, SendAmount, SyncProgress,
    Transfer, Wallet, MAX_UNUSED_ADDRESSES,
};
