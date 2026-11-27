# For an audit

This is where somebody who is to examine Veil starts: what it is made of,
what it promises and where each promise is kept, whom it defends against
and whom it does not, where input from outside comes in, how to build it
and run its checks, and what is known to be weak or untried. It points
into the code and the other documents instead of repeating them.

Veil is not released. Its app is built for stagenet unless the build
says the main network, the command-line tool knows stagenet only, and
the app is signed with a debug key. The owner has decided that the first
release is for the main network, without an audit before it. Nothing
here has been examined by anybody outside the project.

The numbers in this document were counted at the commit that last changed
it; the command is given next to each, so that they can be counted again.

## What it is made of

```
 app (Flutter, Dart)            screens, settings file, nothing secret kept
   |  Dart FFI                  requests and state as JSON text
 veil_bridge (Dart)
   |  C interface               native/veil_core/src/ffi.rs
 veil_core (Rust)               one session = one thread that owns Tor and the wallet
   |-- veil_tor (Rust)          Arti in the process; two SOCKS listeners on 127.0.0.1
   |-- veil_wallet (Rust)       policy over the wallet library
         |  C interface, loaded at run time
       wallet library (C++)     Monero's wallet2 behind monero_c; told to use a listener as its proxy
```

On Android the app also has a Kotlin side for what only the system can do
(`app/android/.../veil/`: `MainActivity`, `WalletService`, `Biometrics`),
and brings a transport program for bridges, which Tor starts as a process
of its own.

### Veil's own code

| Part | Language | Lines | Where |
|---|---|---|---|
| Tor component | Rust | 2,900 | `native/veil_tor` |
| Wallet policy | Rust | 3,600, and 1,600 of tests | `native/veil_wallet` |
| Core, session, C interface | Rust | 7,400, of which 4,100 are tests | `native/veil_core` |
| Command-line tool | Rust | 1,100, and 550 of tests | `native/veil_cli` |
| Test helpers | Rust | 1,750 | `native/veil_test_guard` |
| Licence texts for a package | Rust and shell | 600 | `native/veil_notices`, `native/notices.sh` and the two it calls; not in an app |
| Dart side of the core | Dart | 740 | `dart/veil_bridge` |
| App | Dart | 5,000 without generated texts | `app/lib` |
| App, Android side | Kotlin | 770 | `app/android` |
| Build and check scripts | Shell | 1,600 | `native/*.sh`, `native/wallet_lib`, `native/transports` |

Counted with `wc -l` over the source files of each part.

### Code that is not Veil's, and how it is pinned

| What | Version | Pinned by | Veil's changes |
|---|---|---|---|
| Monero wallet library (`wallet2` behind `monero_c`) | `monero_c` at `a59deb81`, tag `v0.18.5.3-RC1`; the Monero release is that repository's submodule | commit, in `native/wallet_lib/build.sh` | Five patches to Monero and two to `monero_c`, a few lines each, and one for the Android build only. `native/wallet_lib/README.md`, "Veil's patches", says what each does and why. |
| Arti, the Tor client | 0.48 | `native/Cargo.lock`, 510 packages in all | None. |
| lyrebird, the transport program in the Android package | 0.10.0, commit `59ac7e40` | commit; its Go packages by its `go.sum`; Go by checksum | None. `native/transports/README.md`. |
| Flutter, with Dart | 3.47.9 | checksum, in `native/check.sh` | None. |
| Android NDK | r28c for the core; Monero's own, of 2018, for the wallet library | checksum; Monero's dependency build | `native/wallet_lib/README.md`, "For Android". |
| ZXing, for reading a code with the camera | `zxing-android-embedded` 4.3.0 | version, in `app/android/app/build.gradle.kts` | None. |
| `qr`, for drawing a code | 4.x | `app/pubspec.lock` | None. |

What the wallet library and Arti compute is theirs: keys, addresses,
transactions, and the Tor protocol. Veil implements none of it, and an
audit of Veil is not an audit of them. In scope, we suggest, is how Veil
uses them: the patches, the build scripts, how the library is loaded, and
everything in the table of Veil's own code.

## What it promises, where that is kept, and what shows it

The promises are those of `docs/concept.md`. Each row names the code that
keeps one and the test that would fail if it did not. "Private chain" and
"Tor" mean a test that only the owner's machine runs
(`docs/development.md`, "One command").

| Promise | Kept by | Shown by |
|---|---|---|
| The wallet reaches a node only through Tor. | The wallet library is given a SOCKS listener of `veil_tor` as its proxy and an onion name as its node, nothing else (`veil_wallet/src/wallet.rs`, `Node`, `point_at`). `veil_core` builds a wallet's connection only from a running Tor (`Core`). | `without_the_listeners_there_is_no_connection_at_all`, `the_wallet_asks_for_its_sync_node_by_name_through_the_sync_listener` (`veil_core`); the `leaks` levels, which record every network call of the tests. |
| Tor's listeners serve onion services and nothing else. | `veil_tor/src/onion.rs`, `socks.rs`: only `CONNECT` to a name ending in `.onion`; an address or any other name is refused before Tor is asked. | `clearnet_hostname_is_refused_without_connecting`, `ip_literal_is_refused_without_connecting`, `made_up_handshakes_reach_tor_for_onion_services_only`, `made_up_names_reach_arti_as_onion_services_or_not_at_all`. |
| The listeners are on loopback. | `socks.rs`, `bind_loopback`: `127.0.0.1`, a port the system chooses; a peer that is not loopback is dropped. | `listener_is_on_loopback`; the `leaks` levels fail on a socket bound to every interface. |
| No direct connection when Tor is not there. | `TorConnector::connect` refuses until Tor is ready. Apart from Arti's own connections into the Tor network, nothing in the process opens a socket to anywhere but loopback. | `nothing_is_asked_of_a_node_while_tor_is_not_ready`; with Tor: `refuses_onion_connect_before_tor_is_ready`. |
| Bridges, when set, are the only way into Tor. | `veil_tor/src/bridges.rs`, `apply`: bridges switched on explicitly; every line and every transport program checked before Tor starts. | `made_up_bridge_lines_are_bridges_or_refused`; with Tor: `a_bridge_that_cannot_be_reached_is_not_gone_around`. |
| Bridges that cannot be read are not taken for no bridges. | `app/lib/core/settings.dart`: a settings file that is there and cannot be read, or whose bridges cannot, gives settings marked `lost`; `app/lib/app.dart` then shows `SettingsLostScreen` in place of every screen that starts Tor. | `a made-up settings file costs settings and nothing else`; `app/test/bridges_before_test.dart`. |
| A node is an onion service. | `veil_core/src/node.rs`, `OnionNode`; `veil_wallet::is_onion_host`. | `made_up_nodes_are_onion_services_with_a_port_or_refused`, `only_onion_nodes_are_accepted`. |
| Synchronising and sending use different circuits, and different nodes where another answers. | Two listeners (`Purpose::Sync`, `Purpose::Broadcast`), the second with a circuit of its own for every connection (`service.rs`, `TorConnector::new`). | Private chain: `synchronises_through_one_listener_and_sends_through_the_other`. That the circuits differ inside Tor is Arti's `isolated_client` and is not tested. |
| An app holds real money only when it was built to. | The network is said when the app is built and is stagenet where nothing is said (`app/lib/core/network.dart`, `native/check.sh`: `VEIL_NETWORK`); the wallet of each network has a folder of its own (`app/lib/core/paths.dart`); a start request names the network and the core has no default (`veil_core/src/ffi.rs`, `parse_start`), with public nodes for each network apart (`veil_core/src/public_nodes.rs`). On Android a build for trying is another app than the released one (`app/android/app/build.gradle.kts`). | `an app that was told nothing is for stagenet`, `each network has a folder of its own for its wallet`, `a_start_request_is_read_or_refused_with_a_reason`, `made_up_start_requests_are_read_or_refused`, `no_node_is_on_both_networks`. That an APK carries the identifier its network gives it was read off two builds by hand, not by a test. |
| A test-only way around Tor cannot be in an app. | The `regtest` feature of `veil_wallet` and the `stand-in` feature of `veil_tor` do not compile in an optimised build (`compile_error!` in each `lib.rs`). | `check.sh`, `no_test_features_in_a_release`, after every build of the core for an app. |
| Every payment arrives at a fresh address, and never more unused ones in a row than a restore would find. | `veil_wallet`, `receive_address`, `MAX_UNUSED_ADDRESSES`. | `receive_address_stays_the_same_until_it_is_used`, `extra_addresses_are_capped`; private chain: `a_restore_finds_payments_past_long_runs_of_unused_addresses`. |
| An amount is the amount written. | `veil_wallet/src/amount.rs`: integers only. | `made_up_amounts_are_read_exactly_or_refused`, against a second reading with wider numbers. |
| Nothing is sent without a review, and zero never means everything. | `prepare_send` builds, `commit` sends; `SendAmount::Everything` is the only way to send all. | `sending_is_refused_before_anything_is_built`; private chain: `receives_reviews_sends_and_survives_a_lost_answer`. |
| A payment is made once. | A send whose outcome is unknown is written to disk at once, blocks other sends, and is looked for after every synchronisation (`veil_core/src/session.rs`). | Private chain: `lost_connections_lose_no_funds_and_hide_no_send`, `receives_reviews_sends_and_survives_a_lost_answer`; `a_send_left_unknown_by_an_earlier_session_is_taken_up_again`. |
| A killed process or a damaged file does not cost the wallet. | Keys in a file of their own; a data file that cannot be read is set aside and the wallet opens from its keys. | `a_wallet_survives_being_killed_while_it_saves`, `a_wallet_whose_data_file_is_damaged_opens_from_its_keys`. |
| A wallet is open in one place. | `<wallet>.lock`, held with the system's file lock (`veil_wallet/src/wallet.rs`). A lock that is taken is asked for again for half a second before the wallet is refused, because one this program has just let go can count as taken while it starts another program. | `a_wallet_that_is_open_already_is_not_opened_twice_and_not_touched`, `a_mark_let_go_is_free_while_other_programs_are_being_started`. |
| Nothing of a wallet lies outside its folder. | Patch `monero/0005`; the address file the library writes is removed. | `a_wallet_leaves_nothing_outside_its_folder`, `the_wallets_address_is_not_left_as_text_next_to_it`. |
| A recovery phrase and a password are not written to a log or an error. | `SeedPhrase` hides itself in `Debug`; no part of the core writes a log at all. | `made_up_phrases_are_counted_and_never_shown`, `made_up_start_requests_are_read_or_refused`. |
| A locked wallet is closed. | Locking closes the session: the wallet is saved and closed, Tor stopped (`app/lib/app.dart`, `veil_core::Session::close`). | `app/test/wallet_flow_test.dart`, `settings_test.dart`, on a wallet that only pretends. |
| The auto-lock cannot be switched off. | `app/lib/core/settings.dart`, `autoLockChoices`. | `a made-up settings file costs settings and nothing else`. |
| On Android the wallet's files go to no backup, and no picture is taken of a recovery phrase. | `AndroidManifest.xml` (`allowBackup`, `dataExtractionRules`); `FLAG_SECURE` while a phrase is on a screen (`MainActivity`). | The Dart side by tests on a device that only pretends; the Kotlin side by the owner's hand on one phone. |

## Whom it defends against

**Somebody who watches the network** the device is on. They see a
connection to Tor, or with bridges a connection that is made to look like
something else. They do not see a Monero node, an address or an amount.
That somebody uses Tor at all is hidden only by bridges, and how well is
the transport's business.

**A Monero node.** It is an onion service, so it does not learn where the
wallet is. It cannot take funds. It can refuse to serve, serve an old
chain, or leave transactions out, and the wallet then shows less than is
there; nodes are public and chosen among several. It learns roughly when
a wallet was made, from where the wallet starts scanning, and when it is
online. See "Known to be weak" for what the node learns when a
transaction is built.

**Somebody who gets hold of the device while the wallet is locked.** The
keys file and the data file are encrypted by the wallet library with the
password; Veil adds nothing to that and takes nothing away. How well it
holds is the library's key derivation and the password's strength. What
they can read without the password is listed under "What lies on the
disk".

**Somebody on a network that blocks Tor.** Bridges, with obfs4, Snowflake
or WebTunnel.

## Whom it does not defend against

- **A program that runs as the user**, or anything above it. It can read
  the screen, the keys the user types, and the memory of the wallet while
  it is open, where the keys are. A hot wallet is this by definition.
- **Another program on the same machine using the listeners.** While a
  wallet is open, any program on the machine can connect to the two SOCKS
  ports on loopback and reach onion services through the wallet's Tor.
  It reaches nothing but onion services, and it shares no circuit with
  the wallet's own connection on the listener for sending; on the
  listener for synchronising it may share one. There is no password on
  the listeners.
- **Somebody who can watch both ends of a Tor connection.**
- **The phone's recognition of a finger or a face**, where the user has
  switched that on: whoever the phone takes for the user opens the
  wallet and can send.
- **The supply of the code it is built from**, beyond pinning it. The
  wallet library, Arti, and 510 Rust packages are taken as they are at
  the pinned versions. No list of known vulnerabilities is checked
  against them by any script here.

## Where input from outside comes in

Each of these has a test on made-up input; `docs/development.md`,
"Made-up input", has the table of what has to hold.

| From | What | Read by |
|---|---|---|
| The user, or a clipboard, or a code seen by the camera | An address or a `monero:` link | `app/lib/core/payment.dart`, then the wallet library |
| The user | An amount | `veil_wallet/src/amount.rs` |
| The user | A recovery phrase | `app/lib/core/phrase.dart`, `veil_wallet/src/seed.rs`, then the wallet library |
| The user | A node, bridge lines | `veil_core/src/node.rs`, `veil_tor/src/bridges.rs`, then Arti |
| The disk | The app's settings file | `app/lib/core/settings.dart` |
| The disk | The wallet's files | The wallet library. The notes Veil keeps in the data file (the user's nodes, a send whose outcome is unknown) are read back in `veil_core/src/session.rs`. |
| Any program on the machine | A SOCKS request to a listener | `tor-socksproto`, then `veil_tor/src/onion.rs` |
| A Monero node, through Tor | Answers to the wallet's requests | The wallet library, entirely |
| The Tor network | Everything Tor says | Arti, entirely |
| The app | Requests and commands as JSON | `veil_core/src/ffi.rs` |

The two rows that end in "entirely" are where input from the network
meets code that is not Veil's. The wallet library is C++; one test hands
it a node's answers made up (`made_up_answers_from_a_node_...`), and none
hands it blocks or transactions.

## Code that the compiler does not check

Rust's `unsafe` is used in four files, all of them at a boundary to
another language:

| File | Blocks | For |
|---|---|---|
| `veil_wallet/src/wallet.rs` | 43, and 3 `unsafe impl` | Calls into the wallet library; the file lock; pointers handed to the thread that watches a synchronisation |
| `veil_wallet/src/sys/mod.rs` | 6 | Loading the library and reading the strings it returns |
| `veil_core/src/ffi.rs` | 17, in 13 functions that are `unsafe` to call | The C interface the app uses |
| `veil_tor/src/ffi.rs` | 7, in 9 such functions | A C interface to Tor alone. The app does not call it, but its seven functions (`veil_tor_start` and the rest) are exported from the app's core library all the same, because the crate is part of it. |

Counted with `grep -c 'unsafe {'`. Every block has a comment that begins
with `SAFETY:` and says why it is sound; clippy's
`undocumented_unsafe_blocks` makes that a rule for the workspace.
`veil_wallet/src/sys/bindings.rs` is generated from the library's header
for the functions Veil calls and nothing else.

What to know about these boundaries:

- **The wallet library is loaded once and never unloaded**, from a file
  the app names. On Windows only that file's folder and the system folder
  are searched for what it needs. A library that lacks one of the
  functions Veil calls does not load.
- **The C interface takes a panic at its border** (`catch_unwind`) and
  answers with a problem or a fallback. A call takes its own reference to
  the session and lets go of the handle before it waits for anything.
- **The wallet is not thread-safe** and lives on the session's thread.
  A second thread watches a synchronisation: it reads the record of
  progress the library writes and asks the library to stop, which is the
  library's own way to end a synchronisation from outside.
- **The Dart side** (`dart/veil_bridge/lib/src/session.dart`) frees every
  text the core hands out with the function made for it, and overwrites
  its copies of a password and a phrase before freeing them.

## Secrets, and how long they live

| Secret | Where | For how long |
|---|---|---|
| The keys | Inside the wallet library, decrypted | While the wallet is open |
| The password | `veil_wallet::Wallet` keeps the one the wallet was opened with, to check a password against it; wiped when dropped (`zeroize`) | While the wallet is open |
| The password, on its way in | A copy on the Dart side for the call, overwritten and freed; a `String` in the screen that asked for it, which Dart frees when it likes and which cannot be wiped | The call; the screen's lifetime and after |
| The recovery phrase | Asked of the library when it is to be shown; wiped on the Rust side (`veil_secret_free`); a Dart `String` on the screen | While the screen shows it, and until Dart frees it |
| The password, kept for a fingerprint | Encrypted by a key in Android's key store that works once after a recognition (`Biometrics.kt`); the plain bytes are overwritten, the `String` made of them is not | Until switched off, a new password, new fingerprints, or the wallet's removal |

Nothing in the core writes a log: no logger is set up, and neither
`veil_tor`, `veil_wallet` nor `veil_core` prints. Bridge lines are hidden
in `Debug` output like a phrase. An error can carry what the wallet
library said in its own words.

## What lies on the disk

On a desktop under `~/.local/share/veil` or `%APPDATA%\Veil`; on Android
in the app's own storage.

| File | Holds | Readable without the password |
|---|---|---|
| `mainnet/wallet.keys`, `stagenet/wallet.keys` | The keys, in the folder of the network the app was built for | No: encrypted by the library |
| `mainnet/wallet`, `stagenet/wallet` | The wallet's view of the chain, the user's own nodes, a send whose outcome is unknown | No: encrypted by the library |
| `mainnet/wallet.lock`, `stagenet/wallet.lock` | Nothing; marks an open wallet | Yes, and it says only that |
| `settings.json` | Language, theme, auto-lock, bridge lines | Yes. Bridge lines are handed out one person at a time. |
| `tor/state`, `tor/cache` | Arti's files: its guards, the network's directory | Yes. They show that Tor is used and through which first relays, not what for. Made private to the user on Unix. |
| `tor/state/bridged`, `tor/state/bridged.lines` | Arti's state while bridges are set, and the bridge lines that state belongs to (`veil_tor/src/state.rs`) | Yes. The lines are the ones in `settings.json`, and Arti's state names the same bridges. Emptied when the bridges change. |
| Android only: the app's preferences | The password, encrypted for a fingerprint | Not without the phone's secure hardware and a recognised finger or face |

That these files exist shows that Veil is installed and a wallet was
made. A wallet that is removed takes its folder along, and on a phone
the password kept for a fingerprint; the settings file and Tor's files
stay.

## Building it and running its checks

`docs/development.md` has all of it. In short, on Debian or Ubuntu:

```bash
cd native && ./check.sh setup
```

That installs the packages, Rust at the pinned version, the wallet
library built from source into `~/veil-wallet-lib` (about half an hour),
and Flutter. Then:

```bash
./check.sh offline
```

```bash
./check.sh leaks
```

```bash
./check.sh fuzz
```

These three need no network once `setup` is done and start nothing that
leaves the machine. `offline` is format, lint, and every test that needs
no network, about ten minutes; `leaks` runs the Rust tests again with
every network call recorded; `fuzz` runs the tests on made-up input for a
minute each from a new seed.

The levels `regtest` and `leaks-chain` start a Monero daemon on a private
chain on the machine; `tor`, `stagenet` and `bridges` connect to the Tor
network. They are refused in hosted CI, for the reason given in
`docs/development.md`, "Hosted CI and the hosting account", and whoever
runs them should read that first.

What can be built the same twice: `native/twice.sh` builds a part of a
package in two folders that have nothing in common and compares the two
byte for byte. On one machine the core, the transport program, the wallet
library and the app come out the same that way, a package for Android but
for its signature; `docs/development.md`, "The same build twice", has
each part, what had to be done for it, and what that does not show.
On a second machine only the wallet library has been built, and it is
the same file there as in a cloud session, byte for byte, for Windows
and for Linux, once the build stopped reading the builder's settings
for git and stopped compiling for the builder's processor. For Linux
that holds only where the distribution's libraries are the same, as
they were on those two machines that day. The app for Windows is not
compared at all.

A release for Android is signed on the owner's machine with a key that
no other machine holds (`native/release.sh`; `docs/development.md`,
"Signed packages"), and what is signed is a package anybody can build
from the commit and compare, signature apart. The checksum of that
key's certificate is in `native/release-key.sha256` once the key
exists; until then nothing is signed with a key that means anything.
Nothing signs a package for the desktop.

## Known to be weak

Each of these is known to the project and not put right.

- **The node a wallet synchronises with is also asked for what a
  transaction is built from**, the decoys, although the transaction then
  goes out through another node. The concept says no node can link the
  two; this one can see that somebody it serves is about to spend.
- **A transaction built twice for the same coin gets new decoys the
  second time**, which shows the node which coin is spent
  (`docs/concept.md`, "Known limitations").
- **A password change that fails after the keys file was rewritten** is
  reported as failed although the new password is in force.
- **`VEIL_CORE_LIB` and `VEIL_WALLET_LIB`** name the two libraries to
  load in a build for release as well as in a test. Whoever can set the
  app's environment can make it load another library; they can also do
  worse.
- **The wallet library decides from a variable nothing has set** when a
  node does not list the protocol upgrades with its version
  (`wallet2::check_hard_fork_version`). Not patched;
  `native/veil_wallet/README.md` has the row.
- **A loss of the network that shows only as timeouts** is read as nodes
  that do not answer, not as Tor's.
- **With bridges, a bridge that does not answer** is read as the network
  being lost, although another bridge may carry the next connection. The
  app's words allow for it; the state itself cannot tell the two apart.
- **Arti does not take a bridge back that was out of its configuration
  in between**, if it had once read that bridge's description
  (`tor-guardmgr` 0.48, `Guard::update_from_universe`). Not patched and
  not reported to the Tor Project by us. Veil goes around it: Tor's state
  with bridges is kept for one set of bridges and starts empty with
  another (`veil_tor/src/state.rs`, with offline tests of the folder; the
  way of the fault itself is walked only at the `bridges` level, on the
  real network).
- **The wallet library used from several threads at once.** The app has
  one thread that calls it; the tests have many. One fault of that kind
  was Veil's own: the patch that makes the library read paths as UTF-8
  on Windows put its setting where the library sets itself up on its
  first call, behind a check that several threads can pass together, and
  made twice the setting destroys a locale other threads are reading
  paths with. The patch now makes that work happen once
  (`monero/0004`), and a test makes the first call from sixteen threads
  at once. Windows recorded a damaged heap (`0xC0000374`) for a test
  program of `veil_core` twice, in the program's first second. That was
  put down to this fault and is not shown to come from it: the new test
  passed on the library as it was before, thirty processes long. So the
  cause of the damaged heap is open, and whether there are other faults
  of the kind is not known: nothing runs the library under a tool that
  would see them.
- **The listeners take any program on the machine**, as said above.
- **Dart and Kotlin strings cannot be wiped**, so a password and a phrase
  stay in the app's memory until it is freed and reused.
- **How the packages meet other people's licences** has been looked at
  by nobody who knows licences. The wallet library is in part under the
  GNU Lesser General Public License and the transport program in the
  Android package under the GNU General Public License; a package
  carries their texts and names its source (`native/notices.sh`,
  `docs/concept.md`, "Decided for the release").

## Never tried

- Money on the main network. A new, empty wallet has synchronised with
  public nodes there in the owner's runs, and that is all: no payment
  was ever received or sent on it by anything here. The public nodes an
  app for the main network goes to are five from the list at
  monero.fail that the owner chose and nobody of this project has
  vetted (`veil_core/src/public_nodes.rs`, `docs/concept.md`, "Decided
  for the release").
- A network that really blocks Tor. Bridges were seen to carry Tor from
  a network that does not block it. WebTunnel was not tried at all.
- The Kotlin side beyond the owner's hand on one phone, a Samsung Galaxy
  A17.
- Blocks and transactions made up for the wallet library; anything made
  up for Arti.
- Fuzzing guided by coverage, a sanitizer, or Miri on the `unsafe` code.
- The leak check on the real Tor client, on the app's window, on Windows
  or on Android.
- Things that happened once on the owner's machine as Windows programs
  and whose cause was not found: `CLAUDE.md`, "Environment notes".
- What the concept names and nobody has built: fiat rates, a date in
  place of a block number when a 25-word phrase is restored.

## Where we would look first

1. `veil_tor/src/socks.rs` and `onion.rs`: the one place that decides
   what reaches Tor.
2. `veil_wallet/src/wallet.rs`, from `point_at` on: the one place that
   tells the wallet library where to connect, and every `unsafe` call
   into it.
3. `veil_core/src/session.rs`: the send whose outcome is unknown, which
   is what stands between a lost connection and a payment made twice.
4. `veil_core/src/ffi.rs` with `dart/veil_bridge/lib/src/session.dart`:
   pointers, lifetimes, and who frees what.
5. `native/wallet_lib`: the patches and the build, which decide what the
   library that holds the keys is.
6. `app/android/.../Biometrics.kt` and the manifest.

## Findings

To the project's owner, and not in public before the owner has answered.
How is to be agreed when an audit is commissioned; the repository has no
address for it yet.
