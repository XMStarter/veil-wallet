//! Embedded Tor for the Veil wallet.
//!
//! This crate runs an Arti Tor client inside the app process and exposes it
//! to the rest of the wallet as SOCKS listeners on the loopback interface.
//! The Monero wallet library is pointed at one of those listeners, so it
//! never opens a direct connection to the network.
//!
//! Three properties matter more than anything else here:
//!
//! * **Fail closed.** A connection is either carried over Tor or refused.
//!   There is no direct fallback.
//! * **Onion only.** The listeners refuse every target that is not a
//!   `.onion` hostname. Nothing leaves through an exit relay and no hostname
//!   is ever resolved outside Tor.
//! * **Loopback only.** The listeners bind to `127.0.0.1` on a port chosen by
//!   the operating system.

pub mod ffi;
mod onion;
mod service;
mod socks;

pub use service::{Purpose, State, Status, TorConfig, TorError, TorService};
