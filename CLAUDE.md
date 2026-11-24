# Veil: instructions for Claude

Veil is a hot Monero wallet for desktop and Android. Read
[docs/concept.md](docs/concept.md) before making product decisions,
[docs/design.md](docs/design.md) before touching the interface, and
[docs/development.md](docs/development.md) before committing.

## Language

- Talk to the project owner in Russian.
- Everything that goes into the repository is in English: documentation,
  code comments, commit messages, pull request text.
- The app interface ships in English and Russian. It follows the device
  language and falls back to English.

## Priorities

Reliability, then simplicity, then anonymity. When unsure, choose the option
that cannot lose the user's funds.

## Hard rules

- Do not implement cryptography, key derivation, address encoding, or
  transaction construction. Call the official Monero wallet library.
- Do not add any network call that bypasses the embedded Tor client. There is
  no fallback to a direct connection.
- Do not add analytics, crash reporting, or third-party services.
- Do not commit secrets, signing keys, or wallet files.
- Do not use real funds in tests. Use a local chain or stagenet.
- Do not add settings that let the user weaken privacy defaults.
- Do not add a CI job that connects to the Tor network or starts a Monero
  daemon, on GitHub's runners or on a self-hosted one. CI compiles and runs
  offline tests, nothing else. The full list of what a workflow may do is in
  `docs/development.md`, "Hosted CI and the hosting account".
- Do not start a Monero daemon or connect to the Tor network in a cloud
  session unless the owner says so for that session.

## Workflow

- One task, one branch. Do not push to `main` directly.
- Open a pull request only when the owner asks for one. A pull request
  starts hosted CI.
- Workflows start on pull requests and by hand only. Do not add `push` or
  `schedule` triggers, and do not re-run a failed job to get it to pass.
- Group pushes. Do not create bursts of automated activity on GitHub.
- A step of the build plan is done when the owner's acceptance run passes.
  Ask for that run at the end of a step, not after every commit.
- Run the tests before committing and report failures as they are.
- When behaviour changes, update the documents in `docs/` in the same change.
- The project is licensed under MIT. Keep [DISCLAIMER.md](DISCLAIMER.md)
  linked from the README.

## Layout

- `native/` is one Cargo workspace. Run cargo from there.
  - `veil_tor`: embedded Tor client (Arti) with onion-only loopback SOCKS
    listeners and a C interface.
  - `veil_wallet`: wallet operations over the Monero wallet library. Its
    README lists library behaviours the crate guards against; read it
    before changing `wallet.rs`.
  - `veil_core`: joins the two. The app reaches a wallet only through it;
    it points the wallet at Tor's listeners and nowhere else. `Session` is
    what an interface talks to: the core on its own thread. Tests replace
    Tor with `veil_tor`'s stand-in (the `stand-in` feature, never in an app
    build), which can also cut a connection at a chosen point. The fake
    node in its tests is a few lines of test code that answer a node's
    first questions over loopback. It has no chain and is not `monerod`;
    those tests run only where the wallet library is built, so not in
    hosted CI.
  - `veil_cli`: `veil`, the command-line wallet on stagenet. Stagenet
    only; do not add another network to it.
  - `wallet_lib`: pinned build script for the wallet library, Veil's
    patches to it, and the bindings generator. After changing a patch, run
    `build.sh` again and regenerate the bindings.
  - `veil_test_guard`: called first by every test that starts Tor or a
    daemon; fails the test on a CI runner. A new test of that kind calls it
    too. It also starts the daemon for tests, one at a time, and when the
    daemon does not come up it says how it ended and shows the end of its
    log.
  - `check.sh`: the checks by level (`offline`, `regtest`, `tor`,
    `stagenet`, `bridges`) with a log; `setup` prepares a machine;
    `daemons` is a diagnosis of the test daemon, not a check. With
    `--windows` in front, the levels are built for Windows on Linux and run
    as Windows programs through WSL.
- `dart/veil_bridge`: the Dart side of the core. It calls the C interface
  in `veil_core/src/ffi.rs`; the state crosses as JSON, pinned by
  `native/veil_core/tests/fixtures`, which both sides test against.
- `app/`: the app, written with Flutter, for Linux and Windows first. So
  far the way to an open wallet (create, recovery phrase, restore, unlock),
  Home, Receive, Send, and Settings.
  Screens talk to `WalletBackend` (`app/lib/core`), never to the core
  directly; tests use a backend that only pretends, and no test opens a
  real wallet, because that starts Tor. Read
  `app/README.md` and `docs/design.md` before changing it. Colours,
  typefaces and sizes come from `app/lib/theme`; texts from the `.arb`
  files, always in both languages.
- `docs/`: concept, design, development rules.
- `.github/workflows`: hosted CI, builds and offline tests only. Pin
  actions to commit hashes. It installs Flutter with `check.sh flutter`,
  not with an action.

Before committing, in `native/`: `./check.sh offline` (format, lint, and
the tests that need no network; with the wallet library built in
`~/veil-wallet-lib` it also runs the library tests that need no node). The
`regtest` and `tor` levels are the owner's acceptance run, not something a
cloud session runs.

## Status

Done: the Tor component, and the wallet core (create, restore, receive on
fresh addresses, send) tested on a private chain. Step 1.1 of the build plan
is done too: the check script, hosted CI passing on Linux and Windows, and
the owner's acceptance run passing at the `offline`, `regtest`, and `tor`
levels. Step 1.2 is done as well: `veil_core` connects the wallet to Tor,
and the owner's acceptance run passed at all three levels, including a
send through a separate node on a private chain and a new wallet
synchronising with a public onion node over Tor. Step 1.3 is done: `veil`,
the command-line tool on stagenet (`veil_cli`), passed the owner's
acceptance run at all four levels, and a send tried by hand on stagenet
went through a separate node and confirmed. A new wallet's first sync on
stagenet took about two minutes over Tor. Step 1.4, bridges, is done
(`veil_tor::Bridges`, `veil sync --bridges`): the owner's acceptance run
passed at all five levels. Tor connected through obfs4 and through
Snowflake, did not connect when its only bridge did not exist, and the
tool synchronised a wallet through obfs4 bridges, about three and a half
times slower than directly. WebTunnel is configured like the other two but
cannot be tried yet. Step 1.5, the wallet library for Windows, is done:
`wallet_lib/build.sh --windows` cross-builds the DLL and `monerod.exe`, and
`./check.sh --windows` runs the checks as Windows programs. On the owner's
machine the `offline`, `regtest`, `tor` and `stagenet` levels pass that
way, after three fixes those runs found (never unloading the library,
UTF-8 paths, and not trusting the library alone about an existing wallet)
and after the antivirus was told to allow the DLL and `monerod.exe`. That
completes step 1 of the build plan, the core without a user interface.
Step 2, reliability tests, is done: the owner's acceptance run passed as
Linux and as Windows programs. Tested there: a process killed while
saving, a damaged data file (the wallet opens from its keys), a
connection cut in the middle of a synchronisation (retried, reported, or
taken over by another node), a send whose answer is lost (reported as
unknown, found in the history, paid once), and a restore past long runs
of unused addresses. The same run closed what step 1.5 had left open: the
`bridges` level after the library was rebuilt, and the `tor` level as
Windows programs. In the first run of this step one of two test daemons
did not come up on Linux. The cause was never seen. `./check.sh daemons`
then started eighty daemons in pairs there, with and without loopback
ports being taken around them: all came up, the slowest in 4.2 seconds
against a limit of sixty, so neither a race for ports nor a slow start
showed. It has not happened again, and the daemon helper reports it if it
does. The table of what
is tested where is in `docs/development.md`, "Reliability tests". Next is
step 3, the user interface, in the eight parts listed in the build plan.
Step 3.1 is done: `veil_core::Session` runs the core on a thread of its
own for an app, with commands that return at once and a state to draw
from. The owner's acceptance run passed at the `offline` and `regtest`
levels as Linux and as Windows programs, the scenario with funds
included: receiving, a send reviewed and cancelled, a send confirmed, a
send whose answer is lost, and the same wallet in a new session. Step 3.2
is done: the C interface to the session (`veil_core/src/ffi.rs`) and its
Dart side (`dart/veil_bridge`). The owner's acceptance run passed at the
`offline` and `regtest` levels as Linux and as Windows programs, with
Flutter installed by `./check.sh setup` and the Dart tests calling the
real library on Linux. The Dart side has not been run against the Windows
build of the library: there is no Dart for Windows in that setup. Next is
step 3.3, the app shell, which also brought Flutter to hosted CI. It is
done: the Flutter project in `app/` with the visual language of the
design document, English and Russian, and the start screen. On the
owner's machine the `offline` level passed with the app's checks and its
Linux build, and the built app opened its window under WSL. In hosted CI
the app was built for Windows and the Dart side was tested against the
Windows build of the core, both for the first time, and passed. Step 3.4,
the start flow, is done: creating a wallet with its password, showing and
checking the recovery phrase, restoring from a phrase, opening the wallet
on the device, and locking it, on stagenet. Its tests walk that way on a
wallet that only pretends. On the owner's machine the `offline` and
`regtest` levels passed with it, and the owner walked through the app
with a real wallet and reported, after the step had been merged, that the
check of three words worked, Tor connected on the screen after opening,
the wallet opened with its password after the app was started again, and
locking worked. Restoring in the app was not part of that report. Step 3.5, home and
receive, is done. In the core a synchronisation can now be stopped: the
session saves every two minutes of a scan, and any command, closing
included, stops the scan, is dealt with, and the scan goes on. In the app:
Home (balance, Tor and synchronisation in words, recent activity), Receive
(QR code, address in groups of four, copy, new address), and the wallet
synchronises by itself while open. The owner's acceptance run passed at
the `offline` and `regtest` levels as Linux and as Windows programs, the
test that stops a scan with real blocks behind it included
(`a_long_scan_is_saved_on_the_way_gives_way_and_misses_nothing` in
`veil_core`). As Windows programs that test first failed on a step of its
own: it copied the keys file of an open wallet, which the wallet library
keeps locked, and Windows lets nobody else read a locked file. It now
copies that file while the wallet is closed. In the app with a real
wallet the owner confirmed that the synchronisation starts by itself once
Tor is connected, that Receive shows the code and the address and Copy
works, and that locking in the middle of a synchronisation takes seconds.
The owner also reported that the QR code works. No payment has been
received in the app yet. Step 3.6, send, is done.
In the app: a form (address, with paste and payment links, and an amount
or everything), a review (address, amount, fee, total, a warning when
coins from several addresses are combined), and the outcome. In the core:
the review's total and source addresses, a lack of funds as a problem of
its own, and a send whose outcome is unknown is looked for after every
synchronisation, becomes sent when found, blocks other sends until then,
and is settled, never failed, when a synchronisation five minutes later
does not show it. The owner's acceptance run passed: the `offline` level
as Linux and as Windows programs on commit `3c9a853`, and the `regtest`
level both ways on commit `22f44f8`, which differs from it in one test of
`veil_wallet`, one test of `veil_core`, and the change described next; the
`offline` level was run on that last commit in a cloud session only. The
runs found three things. An older test still expected the library's bare
refusal where a lack of funds now has its own error. The session reported
the end of a synchronisation in one change of the state and what it said
about a lost send in the next, and a test, like an interface, could read
the state in between; both are one change now. And the test of the
synchronisation timer failed once as Windows programs while the machine
was busy with the daemons, having passed on the same code before; its
cause was not proven, it now waits as long as the other tests, and it
passed in the next run. Shown on a private chain, on both systems: a
transaction whose answer was lost is found among the node's waiting
transactions before any block and reported as sent; no other send is built
meanwhile; a send to a node that cannot be reached is reported as unknown,
settled by a synchronisation, and sent again, paid once. The owner
reported that sending works in the app. Not tried: a lost connection in
the app itself, which cannot be brought about by hand. Step 3.7,
settings, is written and not yet accepted. In the app: a Settings screen
with the user's own node, Tor bridges, the password, the auto-lock (one to
sixty minutes, never off, and never in the middle of a synchronisation),
the recovery phrase shown again for the password, language, a dark theme,
and the wallet removed from the device for the password; colours now come
from `VeilPalette.of(context)`. In the core: the password checked and
changed (`Wallet` keeps the one it was opened with in memory, because the
library's own copy is empty for a wallet made in the same session), the
user's own nodes and a send whose outcome is unknown kept in the wallet's
data file as encrypted notes, bridges through the C interface with their
programs looked for by `veil_core::transports` (moved from `veil_cli`),
and a check of bridge lines that starts nothing. The owner asked for the
removal of the wallet and for the unknown send to be remembered on disk
as part of this step. The offline tests pass in a cloud session. Not run:
the scenario on a private chain, which now closes and reopens the session
while the outcome of a send is unknown; the Windows build; and anything in
the app with a real wallet, a changed password above all. Bridges changed
in Settings take effect the next time the wallet is opened. Next after
its acceptance is step 3.8, the app on Windows. Not done: the app. See the build plan in
`docs/concept.md`.

## Environment notes

- Tests on a private chain, over Tor, and on stagenet run on the owner's
  machine: Windows, inside WSL2 (Ubuntu 24.04), as Linux programs and with
  `--windows` as Windows programs. The owner runs them and sends the log.
  A cloud session can build for Windows but cannot run the result: Wine
  does not start there. The machine's antivirus (ESET) flags the wallet DLL
  and `monerod.exe` as a coin miner; the owner has to allow each after it
  is rebuilt, and a run made while its question is open fails on loading.
  Each run of `veil.exe` takes 15 to 20 seconds there before it does
  anything, from loading over the WSL file share and being scanned. Tor connects
  from that machine without bridges, so the `bridges` level shows there
  that a bridged connection works, not that it gets through a block.
- What a cloud session can reach depends on its environment's network
  settings, which the owner can change. The rule above about Tor and the
  daemon holds either way.
- The wallet library encrypts its connection to a node (TLS) when the node
  offers it, and `monerod` does. A test cannot recognise a request to a
  real node by its text; the stand-in's cuts count bytes for that reason.
- Flutter in a cloud session: the version pinned in `native/check.sh`,
  unpacked where `check.sh` looks for it (`~/veil-flutter`), with usage
  reporting switched off (`flutter --disable-analytics`,
  `dart --disable-analytics`) before anything else is run. The tools
  complain about running as root; that is harmless there.
- The wallet library can be built in a cloud session from source with
  `native/wallet_lib/build.sh` (about half an hour; install the packages
  listed at the top of the script first). Building it is fine; `monerod`
  from that build is not started there.
- CI results are readable from cloud sessions with `gh api`:
  `repos/XMStarter/veil-wallet/commits/<ref>/check-runs` for the outcome,
  `repos/XMStarter/veil-wallet/actions/jobs/<id>/logs` for a job's log.
- A GitHub account was suspended shortly after a CI job of this project
  connected to the Tor network. Treat the hosted-CI rule above as firm.
