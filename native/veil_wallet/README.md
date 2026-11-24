# veil_wallet

Wallet operations for Veil: create, restore, receive, send. A safe Rust
layer over the official Monero wallet library, which does all cryptography.
See [`../wallet_lib`](../wallet_lib) for the library itself.

## What this crate adds

- **Onion nodes only.** `Wallet::connect` accepts a `.onion` host reached
  through a local SOCKS proxy and nothing else. The direct connection used
  by tests exists only behind the `regtest` feature.
- **A separate node for sending.** With `set_broadcast_node`, a
  transaction is handed to that node and the wallet is pointed back at its
  sync node afterwards, whatever the outcome.
- **A fresh address per payment.** `receive_address` returns the latest
  issued subaddress until something arrives on it, even unconfirmed, and
  then issues the next. The primary address is never handed out.
- **A cap on unused addresses.** At most 20 addresses in a row may be
  unused, far below the 200 a restored wallet looks ahead, so a restore
  always finds every payment.
- **Exact amounts.** `Amount` is an integer number of atomic units. Parsing
  rejects anything ambiguous instead of rounding. A comma is taken as a
  decimal separator, except where it could as well be grouping thousands
  (`1,250`): that is refused, and the same digits with a point are taken
  as they stand.
- **No accidental "send everything".** Sending the whole balance is a
  separate, explicit choice; an amount of zero is an error.
- **Review before sending.** `prepare_send` builds and signs a transaction
  and reports its fee. Nothing is broadcast until `commit`.
- **One payment, one transaction.** A payment that the library would
  split into several transactions is refused with `SeveralTransactions`.
  The library sends such transactions one after another and stops at the
  first that fails, which would leave a payment that went out in part.
- **Secrets are wiped.** Recovery phrases and passwords are zeroed in memory
  after use and never appear in debug output or error messages.
- **On Windows, nothing rides in with the library.** Windows would look for
  the DLLs a library needs in the current folder and along `PATH`, where
  anyone could have put a file with the right name. The wallet library is
  loaded so that only its own folder and the system folder are searched.

## Library behaviours this crate guards against

Found while testing against the real library. Each has a test.

| Behaviour of the library | What this crate does |
|---|---|
| A refresh reports success without scanning when the node is unreachable or still syncing | `refresh` checks both first and returns `NodeUnreachable` or `NodeSyncing` |
| A refresh can report success having scanned nothing | `refresh` fails if blocks are missing and none were scanned |
| Setting a node moves the scan start of any wallet with nothing scanned yet to the node's current height. That includes a restored wallet closed before its first sync, which would then show a zero balance | `connect` keeps the stored start height: the creation date, the phrase's date, or the chosen height |
| An amount of zero means "send everything" | Zero is rejected; `SendAmount::Everything` is the only way to sweep |
| Transaction ids are gone once a transaction is broadcast | They are read before broadcasting |
| An empty list is returned as a static string that must not be freed | Empty strings are never freed |
| On Windows, a wallet in a folder whose name is not ASCII is not recognised as existing | Before creating or restoring, the system is asked as well as the library, and either answer refuses; a Veil patch makes the library read paths as UTF-8 |
| Unloading the library after a wallet was opened waits for ever on Windows: its clean-up joins threads that cannot finish while it runs | The library is loaded once and never unloaded |
| A wallet whose data file is damaged does not open at all, although its keys file is intact | `open` sets the unreadable file aside and opens the wallet from its keys; `was_rebuilt_from_keys` says so, and the next synchronisation finds the funds again |
| A start height changed after the wallet was made is forgotten when the wallet is closed: it lives in the keys file, which is written when the wallet is made and again only when its password is changed | Documented on `set_restore_height`, and nothing in Veil depends on it: the height that counts is given at creation or restore, and a test checks that it is still there after reopening. Tests on a private chain set it again each time they open a wallet |
| A connection lost while a transaction is handed over is reported like a node that was never reached, though the node may have the transaction | `commit` fails with `SendOutcomeUnknown`, which says to synchronise and check the history before sending again. The wallet records nothing; the next synchronisation shows the transaction if it went out |
| A connection lost in the middle of a refresh is reported as a line of text | `refresh` fails with `NodeUnreachable`, the same as for a node that never answered. While blocks are being downloaded the library first asks again three times by itself, so a single lost connection there is not an error at all |
| A refresh cannot be left while it waits for a node: the library's stop takes effect between two batches of blocks. A stop asked for before the refresh has got going is forgotten | `refresh_until` asks for the stop again every fiftieth of a second until the refresh returns, and reports `SyncStopped` if blocks are left; what was scanned is kept. A request the node never answers still has to run into the library's timeout |
| The library's record of the last block scanned is not reset when a refresh starts | Progress is reported only once that record has moved, so a report always means that this refresh has scanned something |
| The password the library keeps for an open wallet is empty when the wallet was created or restored in this session, and the keys file, which could tell, is locked while the wallet is open and on Windows cannot be read then | `is_password` compares with the password the wallet was opened with, which `Wallet` keeps in memory, wiped when it is dropped. `change_password` does not need the old one from the library: the keys are held decrypted, and a test changes the password of a wallet made in the same session and opens it with the new one |
| Building or sending a transaction switches on a thread that synchronises the wallet every ten seconds through whichever node is set | Every wallet is opened with that thread switched off (a Veil patch to the library); the regtest scenario in `veil_core` checks that nothing is contacted |

The library also encrypts its connection to a node with TLS whenever the
node offers it, as `monerod` does by default, on top of what Tor provides.
Nothing depends on it, but a test cannot read what the wallet and a real
node say to each other.

A consequence of keeping the stored start height: a new wallet's first sync
scans about a month of blocks, because the library starts a month before
the creation date to be safe against clock drift.

## Tests

```bash
cargo test -p veil_wallet
```

Tests that need the library are skipped unless `VEIL_WALLET_LIB` points at
it. The scenario on a private chain also needs `VEIL_MONEROD` and the
`regtest` feature:

```bash
export VEIL_WALLET_LIB=~/veil-wallet-lib/out/libmonero_wallet2_api_c.so
export VEIL_MONEROD=~/veil-wallet-lib/out/monerod
cargo test -p veil_wallet --features regtest
```

`tests/crash.rs` kills a process in the middle of saving a wallet, again and
again, and checks each time that the wallet still opens and is the same
wallet. It needs the library and no network. The private-chain tests are a
scenario of receiving, sending and restoring, and a restore of a wallet
whose payments arrived past two full runs of unused addresses, followed by
the same wallet rebuilt from its keys after its data file was damaged.

Set `VEIL_REQUIRE_WALLET_LIB=1` to turn a missing library into a failure
instead of a skip. The private-chain scenario starts a Monero daemon, so it
runs on a machine you control and never in hosted CI; on a CI runner it
fails before the daemon is started. `../check.sh regtest` sets the
variables and keeps a log.

## Not done yet

- A payment to one of the wallet's own addresses appears in the history as
  an outgoing transfer of zero plus the fee once it is confirmed, with no
  incoming entry. That is how the library reports it; the balance is right.
  The interface will have to say "to yourself".

- Changing the start height of an existing wallet so that it lasts, for a
  user who restored with too late a date. It needs the library to rewrite
  the keys file, and a rescan.
- The interface's warning when a payment combines coins from different
  addresses rests on `PendingSend::source_addresses`. Index 0 there is
  where change comes back to, so a payment from change and from one
  address counts as two.
- Choosing the restore height from a date.
- Background sync with progress reporting.
