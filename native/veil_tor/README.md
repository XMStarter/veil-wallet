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
use veil_tor::{Bridges, Purpose, State, TorConfig, TorService};

let tor = TorService::start(TorConfig {
    state_dir: "/path/private/to/app/tor-state".into(),
    cache_dir: "/path/private/to/app/tor-cache".into(),
    bridges: Bridges::none(),
})?;

let port = tor.socks_port(Purpose::Sync); // known immediately
while tor.status().state != State::Ready {
    // show tor.status().percent and tor.status().message
}
```

`start` returns immediately; Tor bootstraps on its own threads. While Tor is
bootstrapping, a `message` in the status usually means the network blocks
Tor and bridges are needed.

## Bridges

On a network that blocks Tor, the way in is a bridge: a relay that is not
in the public list, usually reached through a pluggable transport that
disguises the connection.

```rust
let bridges = Bridges {
    // Lines as Tor hands them out, one bridge each.
    lines: Bridges::lines_from_text(&text_from_the_user),
    // The programs that speak the transports those lines name.
    transports: vec![TransportProgram {
        protocols: vec!["obfs4".into(), "webtunnel".into()],
        path: "/path/to/lyrebird".into(),
        arguments: vec![],
    }],
};
```

- **Bridges are the only way in.** With bridges set, Tor connects through
  them or not at all. It does not go around a bridge it cannot reach.
- **Checked before anything starts.** Every line must be a bridge, every
  transport a line names must have a program, and every program must exist.
  Tor itself would be content with one usable bridge and drop the rest
  quietly; here a half-wrong setting is an error.
- **Transports are separate programs.** Tor does not speak obfs4, Snowflake
  or WebTunnel itself. It starts a program that does when a bridge needs
  it. The Tor Project's `lyrebird` speaks all three; Debian and Ubuntu
  package the older `obfs4proxy` and `snowflake-client`.
- **Bridge lines stay out of logs.** They are handed out one person at a
  time; debug output and error messages do not repeat them.

Tried on Linux with the bridges built into Tor Browser: Tor was ready in
about a quarter of a minute through obfs4 and through Snowflake, and with a
bridge that does not exist it stayed unconnected. A wallet's first sync
through obfs4 took about three and a half times as long as a direct one.

## For the rest of Veil

`Transport` is what other crates hold: the two ports and the status. It
cannot be implemented outside this crate, and in an app build `TorService`
is its only implementation.

The `stand-in` feature adds `StandIn`: the same listeners with the same
rules, leading to ports on this machine instead of into Tor. Tests use it to
run code that needs Tor without a network, and to read which onion names
were asked for and through which listener. No app build enables it.

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
control, never in hosted CI; on a CI runner they fail before starting
anything. `../check.sh tor` runs both commands below and keeps a log.

```bash
# Lifecycle and fail-closed behaviour; pass with or without a network.
cargo test -p veil_tor --lib -- --include-ignored

# Bootstraps Tor and reaches an onion service; needs the Tor network.
cargo test -p veil_tor --test network -- --ignored --nocapture

# The same through obfs4 and Snowflake bridges, and a bridge that does not
# exist; needs the Tor network and the transport programs.
cargo test -p veil_tor --test bridges -- --ignored --nocapture --test-threads=1
```

`../check.sh bridges` runs the last one and keeps a log.

## Not done yet

- WebTunnel is configured like the other two and its bridge lines are
  understood, but it has not carried a connection: there is no public
  WebTunnel bridge to test with and no packaged program.
- Shipping a transport program with the app. On Android in particular,
  whether an app may start one is the question the concept's fallback to
  the reference Tor implementation is about.
- Bridges through the C interface.
- The Android build is not verified yet. Linux and Windows are: the tests
  that start Tor and reach an onion service pass on both.
