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
  app calls `sync`, `node_height`, `settle_on_nodes`, `prepare_send`, or
  commits a transaction.

`sync_with_progress` reports about once a second how far scanning has come,
so that an interface can show a long first sync moving.

## For an app: a session

A `Core` waits for its node, sometimes for minutes, and an interface cannot
wait with it. A `Session` runs the core on a thread of its own:

```rust
let session = Session::start(config, WalletSource::Open, password);
// Returns at once. Draw from session.state() whenever needed:
// phase, Tor, synchronisation, balance, receive address, history, send.
session.sync();
session.prepare_send(&to, amount); // state.send becomes Review { fee, .. }
session.confirm_send();            // or cancel_send()
```

Commands return at once and their outcome appears in the state, which
carries a revision number that goes up with every change. Problems are
given as a kind (`WrongPassword`, `TorNotReady`, `NoNode`,
`SendOutcomeUnknown`, and so on) for the interface to put into its own
words, with the English text as a detail.

The session decides when things happen and nothing else: it synchronises
on command, and on a timer if told to; it saves after every
synchronisation and every send; while a transaction is under review it
takes no other command but closing. Closing, or dropping the session,
saves the wallet and stops Tor.

## When a connection is lost

- **In the middle of a synchronisation.** While it downloads blocks, the
  wallet library asks again by itself three times. If the node stays away,
  or the connection is lost at another point, `sync` fails with the wallet's
  `NodeUnreachable`; if it was Tor that dropped, with `TorNotReady`, so that
  an interface can say which. The balance stays the one from the last
  synchronisation that finished, and the next one continues the scan.
- **With other nodes to try.** `sync_with_any` takes the candidates in the
  caller's order and moves to the next when a node cannot be reached, is
  not usable, or stops answering part way. It stops at once when Tor is no
  longer ready.
- **In the middle of a send.** `commit` fails with the wallet's
  `SendOutcomeUnknown`: the node may have the transaction. The interface
  must not offer to send again before the next synchronisation, which shows
  the transaction in the history if it went out.

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
| A connection cut in the middle of a synchronisation, with the node still there, with the node gone, and with Tor gone: what is reported, that the next node is tried, and that the wallet is unharmed. The node is a fake that knows its version and height and has no blocks | The wallet library. No network. | `../check.sh offline` |
| The session: a new wallet, the same wallet opened again, a wrong password, a restore, a synchronisation that fails, the timer, the cap on addresses, a send that cannot be built, a damaged data file | The wallet library. No network. | `../check.sh offline` |
| A wallet behind the stand-in on a private chain: synchronises through one listener, sends through the other, and stays silent in between | The wallet library and a Monero daemon | `../check.sh regtest` |
| The session on a private chain: receives, reviews a send and cancels it, sends, loses the answer to a send and finds it in the history, and shows the same in a new session | The wallet library and a Monero daemon | `../check.sh regtest` |
| On a private chain: a download cut once is retried; a node that disappears is reported and another node finishes; a send whose answer is lost is reported as unknown and turns up in the history, paid once | The wallet library and a Monero daemon | `../check.sh regtest` |
| A new wallet synchronises with a public onion node over the real Tor network | The wallet library and the Tor network | `../check.sh tor` |

The ones that need a daemon or the Tor network run on a machine you control and never in hosted CI; on a CI
runner they fail before starting anything.

## Not done yet

- A vetted node list for the app. `settle_on_nodes` and `sync_with_any` go
  through candidates in the caller's order; what the candidates are is
  still open.
- Keeping a transaction whose outcome is unknown, to show it as "maybe
  sent" and to send the same one again instead of a new one.
- Stopping a synchronisation that is under way. Closing a session waits
  for it, which on a first synchronisation can be minutes.
- Changing the password, and the other things the Settings screen needs.
- A C interface for the app: step 3.2 of the build plan.
