# veil_tor

The embedded Tor client for Veil. It runs [Arti](https://gitlab.torproject.org/tpo/core/arti)
inside the app process and exposes it as SOCKS listeners on the loopback
interface. The Monero wallet library is pointed at a listener and so never
opens a direct connection.

## Guarantees

- **Fail closed.** A connection is carried over Tor or refused. There is no
  direct fallback. Until Tor is ready, every request is refused at once.
- **Onion only.** Only `CONNECT` to a `.onion` hostname is served. IP
  addresses and ordinary hostnames are refused before anything reaches Tor,
  so nothing leaves through an exit relay and no name is resolved outside
  Tor.
- **Loopback only.** Listeners bind to `127.0.0.1` on a port chosen by the
  operating system.
- **Separate circuits per purpose.** There are two listeners. `Sync` is for
  wallet synchronisation. `Broadcast` is for sending transactions and uses a
  new circuit for every connection. The two never share a circuit.

Both SOCKS5 and SOCKS4a are accepted. The Monero wallet library uses SOCKS4a
by default.

## Use from Rust

```rust
use veil_tor::{Purpose, State, TorConfig, TorService};

let tor = TorService::start(TorConfig {
    state_dir: "/path/private/to/app/tor-state".into(),
    cache_dir: "/path/private/to/app/tor-cache".into(),
})?;

let port = tor.socks_port(Purpose::Sync); // known immediately
while tor.status().state != State::Ready {
    // show tor.status().percent and tor.status().message
}
```

`start` returns immediately; Tor bootstraps on its own threads. While Tor is
bootstrapping, a `message` in the status usually means the network blocks
Tor and bridges are needed.

## Use through the C interface

The crate also builds as a shared and a static library. See
[`src/ffi.rs`](src/ffi.rs) for the functions: `veil_tor_start`,
`veil_tor_state`, `veil_tor_bootstrap_percent`, `veil_tor_socks_port`,
`veil_tor_message`, `veil_tor_string_free`, `veil_tor_stop`.

## Build and test

```bash
cargo test -p veil_tor
```

An ordinary test run opens no connection to the Tor network. It covers the
SOCKS handshake, the onion-only rule, and the C interface against a
stand-in for Tor.

Tests that start the real Tor client are ignored by default, because a
started client tries to reach the Tor network. Run them on a machine you
control, never in hosted CI:

```bash
# Lifecycle and fail-closed behaviour; pass with or without a network.
cargo test -p veil_tor --lib -- --include-ignored

# Bootstraps Tor and reaches an onion service; needs the Tor network.
cargo test -p veil_tor --test network -- --ignored --nocapture
```

## Not done yet

- Bridges (obfs4, Snowflake, WebTunnel).
- The Android build is not verified yet. Linux and Windows builds are.
