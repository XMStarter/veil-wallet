# veil_core

The core of Veil: a Monero wallet joined to the embedded Tor client. It puts
[`veil_wallet`](../veil_wallet) and [`veil_tor`](../veil_tor) together and
is the only way the rest of the app reaches a wallet.

## Guarantees

- **Tor or nothing.** A `Core` is built from a running `TorService`. Its
  wallet is pointed at that service's loopback SOCKS listeners and at no
  other address. While Tor is not ready, every call that needs a node fails
  with `TorNotReady` before the wallet is asked to do anything. If Tor stops,
  the listeners are gone and the wallet has nothing else to try. If Tor
  loses its network, calls fail with `TorLost` and are made through Tor
  all the same: that is how the return of the network is found.
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
session.confirm_send();            // Sending { the same }; or cancel_send()
session.clear_send();              // once the user has seen how it went
```

While a confirmed transaction is being handed to a node, the state
still says where it goes, how much, the fee and the total, as it did
under review: the screen with those lines stays up until a node has
answered, which over Tor can take a while.

Commands return at once and their outcome appears in the state, which
carries a revision number that goes up with every change. Problems are
given as a kind (`WrongPassword`, `TorNotReady`, `TorLost`, `NoNode`,
`SendOutcomeUnknown`, and so on) for the interface to put into its own
words, with the English text as a detail.

The session decides when things happen and nothing else: it synchronises
on command, and on a timer if told to; it saves after every
synchronisation and every send; while a transaction is under review it
takes no other command but closing. Closing, or dropping the session,
saves the wallet and stops Tor.

What the session holds to, whatever the interface does:

- **A transaction is built only from a wallet that is level with a node.**
  If the last synchronisation of the session did not finish, or there was
  none, a send makes the session synchronise first; if that fails, the
  send fails with the same reason, in the same change of the state. An
  address that is not one and an amount of zero are refused at once,
  without a node.
- **A payment that needs several transactions is refused**
  (`SeveralTransactions`). The wallet library would hand them over one by
  one and stop at the first failure, and a payment could go out in part.
- **A cancel never takes back what was sent.** `cancel_send` drops a
  transaction under review, a send waiting to be built, or a failure. A
  cancel that arrives when the transaction has just gone out leaves it
  shown as sent. `clear_send` is what takes an outcome off the state.
- **A wallet that cannot be written to disk says so.** `not_saved` in the
  state holds the reason from the first save that fails until one works
  again, and a session that cannot save on closing ends as `Failed`.
- **Changes that belong together are one change.** The end of a
  synchronisation, the balance and history after it, and what it means
  for a send; the outcome of a send and the balance it left.
- **The node that worked is asked first.** A session tries its nodes in
  order for every synchronisation. Once one has carried a
  synchronisation to its end, the list is turned so that it comes first;
  the nodes before it, which did not answer, are tried again only when
  it stops answering too.
- **One wallet, one session.** A wallet that is open is marked as such
  by a lock on a file next to it (`veil_wallet`), and a second session on
  it, a second window of the app for one, fails with `WalletInUse` before
  anything is started.
- **A new wallet is made after Tor has started**, so a Tor that does not
  start leaves no wallet behind that nobody has seen the phrase of. A
  wallet that exists is opened before Tor is started, so a wrong password
  starts nothing.
- **A wallet opened from its keys alone does not synchronise by itself.**
  Its data file is gone, and with it the user's own nodes. It waits for
  `sync`, `set_nodes` or a send before it goes to any node.

A synchronisation does not hold the session up:

- Once blocks are arriving, the session stops it every two minutes, saves
  what was scanned, and goes on. A wallet killed in the middle of a long
  scan loses no more than that.
- Any command but another request to synchronise stops it too. The session
  saves, does what was asked, and continues the same synchronisation: the
  state shows it as running throughout, and nodes that had already failed
  are not gone back to. Closing is such a command, so locking a wallet does
  not wait for a scan to end.
- A send asked for meanwhile is shown as being prepared and is built when
  the synchronisation has ended.

How soon a synchronisation stops is up to the wallet library: after the
batch of blocks it is working on, normally within seconds. A request that
a node never answers has to run into the library's timeout first, which
takes minutes.

## From another language: the C interface

The app is written in Dart, so the session is also reachable through a few
C functions (`src/ffi.rs`). What crosses them is text:

| Function | What it does |
|---|---|
| `veil_session_start(config, password, phrase, &problem)` | Starts a session from a JSON configuration; returns a handle at once |
| `veil_session_revision(handle)` | The state's revision, cheap enough to ask on every frame |
| `veil_session_state(handle)` | The state as JSON |
| `veil_session_command(handle, command)` | A command as JSON: `sync`, `new_address`, `prepare_send`, `confirm_send`, `cancel_send`, `clear_send`, `set_nodes` |
| `veil_session_recovery_phrase(handle)` | The recovery phrase, freed with `veil_secret_free`, which wipes it |
| `veil_session_is_password(handle, password)` | Whether that is the wallet's password |
| `veil_session_change_password(handle, old, new)` | A new password for whoever knows the current one |
| `veil_check_bridges(request)` | Whether bridge lines can be used, without starting anything |
| `veil_session_close(handle)` | Saves, closes, stops Tor, and returns the last state |
| `veil_string_free(text)` | Frees any other text the interface returned |

Reading the revision or the state and giving a command never wait: not
for the network, and not for a call on another thread that does wait for
the session's thread (the phrase, the password, closing). The handle is
locked only for as long as it takes to get hold of the session.

The password and a recovery phrase are arguments of their own and never
part of the JSON, and the phrase never appears in the state. Amounts cross
as text, exact to the last digit, so that the other side shows them and
never calculates with them. A request that cannot be read is refused with a
problem of the kind `bad_request`; a comma in an amount is a decimal
separator, as people type it.

The network is named in every start request, `mainnet` or `stagenet`,
and a request without it or with any other name opens nothing: whether a
wallet holds real money is what the app was built to say there, never a
default. Bridges are given at the start as
a list of bridge lines; the programs their transports need are looked for
in the folder the request names as `transport_dir`, if the app brings any
along, and then among those installed on the machine
(`src/transports.rs`), and a line that
is not a bridge or a transport without a program refuses the start, as
`veil_check_bridges` would have said beforehand.

The user's own nodes are set with `set_nodes` and kept in the wallet's data
file, so a later session uses them without being told; the state's
`own_nodes` shows them.
Nodes are the user's own, tried in the order given; without any, the
public nodes of the wallet's network in `src/public_nodes.rs` are used, in
an order nobody can predict. That file has a list for each network and
says where each comes from; `../check.sh tor` asks every node of the one
for the main network where it is.

The state as it crosses the interface is pinned by `tests/fixtures/*.json`.
The core's tests compare what they produce with those files, and the Dart
side reads the same files in its tests, so the two cannot drift apart
unnoticed. `VEIL_WRITE_FIXTURES=1 cargo test -p veil_core --lib ffi` writes
them again after a change that is meant.

## When a connection is lost

- **In the middle of a synchronisation.** While it downloads blocks, the
  wallet library asks again by itself three times. If the node stays away,
  or the connection is lost at another point, `sync` fails with the wallet's
  `NodeUnreachable`; if Tor is not ready, with `TorNotReady`. The balance
  stays the one from the last synchronisation that finished, and the next
  one continues the scan.
- **When the network under Tor goes.** Tor says so (`State::Lost`) once a
  connection it is asked for cannot get to the first relay; see
  `veil_tor`'s README for how that is read and what it cannot tell.
  From then on a call that fails for want of a node fails with `TorLost`,
  and the session reports the problem as Tor's (`tor_lost`), not as "no
  node answered". Nothing is skipped for it: `sync_with_any` asks every
  candidate, a send asks every node of its route, and a session goes on
  synchronising on its timer, because the only sign of the network being
  back is a connection that gets through. The first that does makes Tor
  ready again. A node that is down cannot keep that from happening: its
  failure says nothing about the network, and the next candidate is
  asked. The call during which the network went can still end as the
  node's failure, since a connection that breaks does not say why; the
  next one names Tor. A send that no node took is reported as not sent,
  with Tor as the reason; a send whose outcome is unknown stays unknown
  whatever Tor says.
- **With other nodes to try.** `sync_with_any` takes the candidates in the
  caller's order and moves to the next when a node cannot be reached, is
  not usable, or stops answering part way. It stops at once when Tor is no
  longer ready.
- **Stopped on purpose.** `continue_sync` does the same and can be told to
  stop. It then ends with the wallet's `SyncStopped`, keeps what was
  scanned, and remembers in its `SyncAttempt` which nodes have failed, so
  that calling it again continues where it was and an attempt that is
  stopped again and again still comes to an end.
- **Before a send.** A session hands a transaction to the first node of
  `Core::broadcast_route` that answers when asked whether it is there:
  the node the last send went through, the broadcast node in place, the
  other candidates, and last the sync node itself, all through the
  broadcast listener. A node that is down is given nothing. If none
  answers, the send fails with `NoNode`: nothing was sent, and it can be
  tried again.
- **In the middle of a send.** `commit` fails with the wallet's
  `SendOutcomeUnknown`: the node may have the transaction. A session
  keeps the transaction's ids and looks for them in the history after
  every synchronisation; the wallet library also notices a transaction of
  its own among those a node is still holding. Found, the send becomes
  `Sent`. Until then the session builds no other send and does not let
  the outcome be cleared. A synchronisation that finishes without showing
  it, having begun five minutes or more after the attempt, settles it:
  the state says so, and from then on a new send or a cancel is taken.
  The session never calls such a send failed. It writes the send into
  the wallet's data file, encrypted with the rest, and saves the wallet
  at once, so that an app killed a moment later has it on disk; a session
  that opens the wallet later takes it up where the first left it.

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
| A connection cut in the middle of a synchronisation, with the node still there, with the node gone, and with the network gone from under Tor: what is reported, that the next node is tried, and that the wallet is unharmed. With the network gone: Tor is named from the next connection on, every node is still asked, a node that is down does not hide the network's return, and a session finds the return by itself. The node is a fake that knows its version and height and has no blocks | The wallet library. No network. | `../check.sh offline` |
| The session: a new wallet, the same wallet opened again, a wrong password, a restore, a synchronisation that fails, the timer, the cap on addresses, a send that cannot be built, a damaged data file | The wallet library. No network. | `../check.sh offline` |
| A synchronisation told to stop: no further node is asked, and it continues where it was. In the session: a command and closing do not wait for a synchronisation, which goes on afterwards; a send asked for meanwhile waits for its end. The nodes are fakes that take their time over a request for blocks | The wallet library. No network. | `../check.sh offline` |
| The session: the password checked and changed, and the wallet opening with the new one only; the user's own nodes used at once, kept out of plain sight, used again by the next session, and taken back; a send left unknown by an earlier session taken up again | The wallet library. No network. | `../check.sh offline` |
| Finding the programs bridges need among those installed, and bridges checked on their way in | No network | `../check.sh offline` |
| The C interface: requests read or refused, the state against the fixtures, null handles, a session driven through it from start to close | The wallet library for the last. No network. | `../check.sh offline` |
| A wallet behind the stand-in on a private chain: synchronises through one listener, sends through the other, and stays silent in between | The wallet library and a Monero daemon | `../check.sh regtest` |
| The session on a private chain: receives, is refused a send it cannot afford, reviews a send and cancels it, sends, loses the answer to a send, builds no other meanwhile and finds it before any block, fails to reach the node with a send, has that settled and sends again, shows the same in a new session, and with the network gone from under Tor reports a confirmed send as not sent for Tor's reason and pays nothing | The wallet library and a Monero daemon | `../check.sh regtest` |
| The session on a long private chain over a slowed line: the scan is saved on the way, and the copy taken then opens with part of the chain behind it; the recovery phrase is handed out in the middle; the wallet is closed in the middle and a later session scans the rest; the payment is there each time | The wallet library and a Monero daemon | `../check.sh regtest` |
| On a private chain: a download cut once is retried; a node that disappears is reported and another node finishes; a send whose answer is lost is reported as unknown and turns up in the history, paid once | The wallet library and a Monero daemon | `../check.sh regtest` |
| A new wallet synchronises with a public onion node over the real Tor network | The wallet library and the Tor network | `../check.sh tor` |

The ones that need a daemon or the Tor network run on a machine you control and never in hosted CI; on a CI
runner they fail before starting anything.

## Not done yet

- A vetted node list for the app. `settle_on_nodes` and `sync_with_any` go
  through candidates in the caller's order; what the candidates are is
  still open.
- Sending the same transaction again when the outcome of a send is
  unknown, instead of building a new one once it is settled.
- Bridges changed while a wallet is open. They are given at the start, so
  new ones take effect the next time the wallet is opened.
- The programs bridges need on a desktop. They are looked for among
  those installed; only the app for Android brings one along.
