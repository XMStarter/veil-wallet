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

// The `stand-in` feature adds listeners that lead to ports on this machine
// instead of into Tor. It is for tests, and tests are built with debug
// assertions. An optimised build, which is what an app is made of, does
// not compile with it.
#[cfg(all(feature = "stand-in", not(debug_assertions)))]
compile_error!("the `stand-in` feature is for tests; an optimised build must not have it");

mod bridges;
pub mod ffi;
mod onion;
mod service;
mod socks;
#[cfg(feature = "stand-in")]
mod stand_in;

pub use bridges::{Bridges, TransportProgram};
pub use service::{Purpose, State, Status, TorConfig, TorError, TorService};
#[cfg(feature = "stand-in")]
pub use stand_in::{AfterCut, CutPoint, Request, StandIn};

/// What the rest of Veil uses to reach the network: loopback SOCKS listeners
/// and their state.
///
/// The trait cannot be implemented outside this crate. In an app build its
/// only implementation is [`TorService`], so code written against it has no
/// way to reach the network except through Tor. Tests add [`StandIn`] with
/// the `stand-in` feature.
pub trait Transport: sealed::Sealed + Send {
    /// The loopback port of the SOCKS listener for `purpose`.
    fn socks_port(&self, purpose: Purpose) -> u16;
    /// Current state, progress, and any message for the user.
    fn status(&self) -> Status;
    /// What kind of failure the last connection ended in, if it failed.
    /// See [`TorService::last_failure`].
    fn last_failure(&self) -> Option<String>;
}

mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for TorService {}

impl Transport for TorService {
    fn socks_port(&self, purpose: Purpose) -> u16 {
        TorService::socks_port(self, purpose)
    }

    fn status(&self) -> Status {
        TorService::status(self)
    }

    fn last_failure(&self) -> Option<String> {
        TorService::last_failure(self)
    }
}

#[cfg(feature = "stand-in")]
impl sealed::Sealed for StandIn {}

#[cfg(feature = "stand-in")]
impl Transport for StandIn {
    fn socks_port(&self, purpose: Purpose) -> u16 {
        StandIn::socks_port(self, purpose)
    }

    fn status(&self) -> Status {
        StandIn::status(self)
    }

    fn last_failure(&self) -> Option<String> {
        // The stand-in has no Arti to quote.
        None
    }
}
