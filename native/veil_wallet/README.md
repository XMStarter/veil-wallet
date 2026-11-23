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
  rejects anything ambiguous instead of rounding.
- **No accidental "send everything".** Sending the whole balance is a
  separate, explicit choice; an amount of zero is an error.
- **Review before sending.** `prepare_send` builds and signs a transaction
  and reports its fee. Nothing is broadcast until `commit`.
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
| Building or sending a transaction switches on a thread that synchronises the wallet every ten seconds through whichever node is set | Every wallet is opened with that thread switched off (a Veil patch to the library); the regtest scenario in `veil_core` checks that nothing is contacted |

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

- Warning when a payment has to combine coins from different addresses.
- Choosing the restore height from a date.
- Background sync with progress reporting.
