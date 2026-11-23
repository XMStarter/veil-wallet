# veil_core

The core of Veil: a Monero wallet joined to the embedded Tor client. It puts
[`veil_wallet`](../veil_wallet) and [`veil_tor`](../veil_tor) together and
is the only way the rest of the app reaches a wallet.

## Guarantees

- **Tor or nothing.** A `Core` is built from a running `TorService`. Its
  wallet is pointed at that service's loopback SOCKS listeners and at no
  other address. While Tor is not ready, every call that needs a node fails
  with `TorNotReady` before the wallet is asked to do anything. If Tor stops,
  the listeners are gone and the wallet has nothing else to try.
- **Onion nodes only.** A node is an `OnionNode`: a `.onion` name and a
  port. Nothing else can be named.
- **Sync and spend apart.** The wallet synchronises through the `Sync`
  listener and hands transactions over through the `Broadcast` listener,
  which uses a new Tor circuit for every connection. With `Nodes::separate`
  the two are also different nodes, so the node that sees the wallet
  synchronise never sees it spend. `Nodes::single` is for a user's own node.
- **Nothing happens on its own.** The wallet contacts a node only when the
  app calls `sync`, `node_height`, `prepare_send`, or commits a transaction.

## Use

```rust
use veil_core::{Core, Nodes};
use veil_tor::{TorConfig, TorService};
use veil_wallet::{Network, Wallet, WalletLibrary};

let tor = TorService::start(TorConfig { state_dir, cache_dir })?;
let lib = WalletLibrary::load(library_path)?;
let wallet = Wallet::open(&lib, &wallet_path, password, Network::Mainnet)?;

let nodes = Nodes::separate(sync_node.parse()?, broadcast_node.parse()?)?;
let mut core = Core::new(tor, wallet, nodes)?;

// Show core.tor_status() until Tor is ready, then:
let progress = core.sync()?;
let address = core.receive_address()?;
let pending = core.prepare_send(&to, amount)?; // review pending.fee()
let txids = pending.commit()?;
```

A `Core` is not thread-safe. Give it to one thread; calls that contact a
node block until the node has answered.

## Tests

| What | Needs | Run with |
|---|---|---|
| Node names; Tor not ready; the wallet asks for its node by name through the sync listener only; no listeners, no connection; a web server is not a node | The wallet library for all but the first. No network: Tor is replaced by `veil_tor`'s stand-in. | `../check.sh offline` |
| A wallet behind the stand-in on a private chain: synchronises through one listener, sends through the other, and stays silent in between | The wallet library and a Monero daemon | `../check.sh regtest` |
| A new wallet synchronises with a public onion node over the real Tor network | The wallet library and the Tor network | `../check.sh tor` |

The last two run on a machine you control and never in hosted CI; on a CI
runner they fail before starting anything.

## Not done yet

- Choosing nodes: a vetted list, picking at random, moving on when a node
  stops answering. `set_nodes` is there for it.
- A C interface for the app.
- Progress while a long synchronisation runs.
