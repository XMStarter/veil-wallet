# Development

How Veil is written, reviewed, tested, and released.

## Rules that do not bend

- **No cryptography of our own.** Keys, addresses, and transactions come from
  the official Monero wallet library.
- **No direct network access.** Every connection goes through the embedded
  Tor client. A code path that can reach the network without Tor is a bug.
- **No telemetry.** No analytics, crash reporting, or third-party services.
- **No secrets in the repository.** Signing keys, passwords, and real wallet
  files never get committed.
- **No real funds in tests.** Tests use a local chain or the Monero stagenet.

## Repository workflow

- `main` always builds and passes tests.
- One task, one branch. Commits are small and say what changed and why.
- Changes reach `main` through pull requests, after the checks pass.
- Documentation, code comments, and commit messages are in English.

## Testing

| Level | Where it runs | What it checks |
|---|---|---|
| Format, lint, unit and widget tests | Cloud sessions, hosted CI, any machine | Logic, parsing of addresses and amounts, screen layout. Nothing leaves the machine. |
| Platform builds | Hosted CI | The code builds for Linux, Windows, and Android |
| Wallet on a local chain | The owner's machine | Create, restore from phrase, receive on new subaddresses, send. No internet and no real coins. |
| Tor and stagenet | The owner's machine | Connecting through Tor and bridges, syncing and sending on a real test network |
| Real devices | By hand, before a release | Installing and using the app on a phone and a desktop |

The owner's machine runs Windows. The levels that need the wallet library
run there twice: inside WSL2 as Linux programs, and with `--windows` as
Windows programs.

Code that talks to Tor or to a node is written so that its logic can be
tested against a stand-in on the loopback interface. The run on the real
network confirms the result; it is not the only test.

Each step of the build plan ends with an acceptance run on the owner's
machine. The log of that run is the evidence that the step is done.

### One command

`native/check.sh` runs the checks by level and writes a log to
`native/target/check-logs/`.

| Level | What it runs | Where |
|---|---|---|
| `offline` | Format, lint, and every test that needs no network, the reliability tests against stand-ins among them. Uses the wallet library when it has been built. With Flutter installed it also runs the tests of the Dart side and of the app, and builds the app. | Anywhere, hosted CI included |
| `regtest` | The wallet on a private chain, alone and behind a stand-in for Tor, with and without lost connections. Starts a Monero daemon. | The owner's machine |
| `tor` | The tests that start the real Tor client, a connection to an onion service, and a new wallet synchronising with a public node. | The owner's machine |
| `stagenet` | The command-line tool: a new wallet synchronises with a public stagenet node through Tor. | The owner's machine |
| `bridges` | Tor through obfs4 and Snowflake bridges, a bridge that does not exist, and the tool synchronising through bridges. | The owner's machine |

`./check.sh setup` prepares a Debian or Ubuntu machine once: system
packages, among them the transport programs for bridges, Rust, the
wallet library with the daemon, built from source into `~/veil-wallet-lib`
(about half an hour), the libraries the app's Linux build needs, and
Flutter, which brings Dart, into `~/veil-flutter`.
Flutter is one pinned version, checked against its published checksum, and
its tools are told not to report usage. Run it again when the
patches in `native/wallet_lib/patches` change; it rebuilds only what
changed, and the other levels say so when the library is out of date.

`./check.sh daemons` is not a check of Veil. It starts the test daemon in
pairs, many times, to find out why one may fail to come up, and shows how
each one that fails ended. It is run when asked for, on the owner's
machine.

Every level but `offline` refuses to start on a CI runner. The tests behind
those levels refuse as well, through `native/veil_test_guard`, so that
running them some other way in CI fails before anything is started.

### Acceptance run on the owner's machine

On Windows, once: install WSL2 with `wsl --install` in an administrator
PowerShell, restart, and open Ubuntu. Then, inside Ubuntu:

```bash
git clone https://github.com/XMStarter/veil-wallet
```

```bash
cd veil-wallet/native && ./check.sh setup
```

For each acceptance run, on the branch being accepted:

```bash
./check.sh offline regtest tor stagenet bridges
```

The last line of the output names the log file to send.

`./accept.sh` does a whole run in one command, each step only when the
one before it has passed: it takes the branch's latest commits, runs the
checks as Linux programs, runs them as Windows programs (see below),
names the logs to send, and starts the app that the run has built.
Without arguments the levels are `offline` and `regtest`; others are
named as for `check.sh`. `--no-app` stops after the checks, and
`--no-pull` leaves the commits as they are.

### On Windows itself

WSL runs the Linux build. The same checks also run as Windows programs,
still started from the Ubuntu shell: everything is compiled for Windows on
Linux, and WSL starts the resulting programs on the Windows side. Nothing
is installed on Windows.

```bash
./check.sh --windows setup
```

installs the cross compiler and builds the wallet library and the daemon
for Windows (see `native/wallet_lib/README.md`). Then:

```bash
./check.sh --windows offline regtest tor stagenet
```

The `bridges` level has no Windows form yet: there are no transport
programs for Windows to test with.

Two builds of the Rust code exist for Windows. Hosted CI compiles it with
Microsoft's toolchain; this run compiles it with the GNU one, because that
is the one that can be used from Linux. The wallet library is the same DLL
for both.

### For Android

Nothing built for Android runs on a desktop, so for Android the checks
build and do not test:

```bash
./check.sh --android setup
```

downloads the Android NDK, the compiler for Android, in the release the
pinned Flutter builds apps with (about 700 megabytes, checked against its
checksum, into `~/veil-android`), and builds the wallet library for
Android on 64-bit ARM (see `native/wallet_lib/README.md`; about ten
minutes). Then:

```bash
./check.sh --android offline
```

lints the Rust code for Android, builds the core as the library an app
loads, and compiles every test without running one. The tests themselves
run on Linux and Windows as before; that the same code behaves the same on
a phone is seen only on a phone, from the app.

`setup` also installs what building the app takes: Java from the
distribution and Google's Android SDK, its command-line tools checked
against their checksum. Installing the SDK means accepting its
[licence](https://developer.android.com/studio/terms); `setup` does that
for whoever runs it and says so.

```bash
./check.sh --android app
```

builds the app as one file to install, `app-release.apk`, with the core
and the wallet library inside, and under WSL copies it to the Windows
Downloads folder as `veil.apk`. Copy it to a phone with a 64-bit ARM
processor and Android 7 or later, and open it there; Android asks once
whether apps may be installed from where the file was opened. The app is
a real wallet on stagenet and connects to Tor.

The file is signed with a key Android's tools make on the machine that
builds it. That is enough to install it on one's own phone; a second
machine has another key, and Android refuses an update signed with a
different key until the app is removed, which removes its wallet too. A
key for releases belongs to the release step.

### The app on Windows

An app for Windows can only be built on Windows, with Microsoft's tools,
so hosted CI builds it: every run of the workflow keeps the folder with
`veil.exe`, the core (`veil_core.dll`) and the three libraries of
Microsoft's C++ runtime the two need, as the file `veil-windows` under
the run's Artifacts. It has no wallet library, which hosted CI does not
build. To try it, download that file with the browser (on the pull
request: Checks, the run named `native`, Artifacts) and, in the Ubuntu
shell:

```bash
./windows-app.sh
```

It takes the newest `veil-windows*.zip` from the Windows Downloads folder,
unpacks it into `Veil-app` in the Windows user's home, puts the wallet
library from `./check.sh --windows setup` next to it, and starts it. A
library built before the patches in `native/wallet_lib/patches` last
changed is refused, with the command that rebuilds it. The
app runs from a Windows disk, not over the share to WSL, as an installed
app would. Its wallet is its own, in `%APPDATA%\Veil`; the app started
inside WSL keeps another.

### Reliability tests

They cover the failures that lose money or confuse users. Each is tested
without a network first, and those that need real blocks are repeated on a
private chain.

| Failure | What must hold | Where it is tested |
|---|---|---|
| The process is killed while the wallet is being saved | The wallet opens afterwards and is the same wallet | `veil_wallet/tests/crash.rs`, in `offline`: a process that saves in a loop is killed at a different moment each round |
| The wallet's data file is damaged: emptied, cut short, or overwritten | The wallet opens from its keys file, which is written when the wallet is made and again only when its password is changed; the damaged file is kept; a synchronisation finds the funds again | `veil_wallet/tests/offline.rs`, in `offline`; with funds, in `regtest` |
| The connection is lost in the middle of a synchronisation | The failure is reported; the balance is the last one that was complete; the wallet stays usable | `veil_core` against a fake node, in `offline`; on a private chain in `regtest` |
| The device loses its network while the wallet is open | Tor says that it has lost its network once a connection it is asked for cannot get to the first relay, and the failure is reported as Tor's, not the nodes'. Nodes go on being asked for, through Tor, and the first connection that gets through ends it; nobody has to press anything. A send that no node took is reported as not sent. A loss that shows only as timeouts looks like nodes that do not answer | `veil_tor`: how each kind of failure is read, in `offline`. `veil_core` with the stand-in's network taken away, in `offline`; a confirmed send with it taken away, in `regtest`. With a real network: by hand on the owner's machine, once, in the app under WSL: Home said that Tor had lost its connection and, with the network back, that it was connected and synchronised, with nothing pressed. Not tried in the app for Windows |
| The node stops answering for good | Another node continues the synchronisation and finds what the first did not deliver | `veil_core`: that the next node is asked, in `offline`; that it finishes, in `regtest` |
| The connection is lost while a transaction is handed to the node | The send is reported as neither done nor failed, and no other send is built; the next synchronisation finds it among the node's waiting transactions and shows it as sent; the payee is paid once | `veil_core`, in `regtest` |
| The app is closed, or killed, while the outcome of a send is unknown | The next session knows of the same send, still builds no other, and settles it as the first would have. The record is on disk as soon as the send has ended that way, not only when the app is closed | `veil_core`: that a record in the wallet is taken up, in `offline`; that a session writes it at once (a copy of the data file taken while the session is still open has it) and the next one reads it, in `regtest` |
| The wallet cannot be written to disk | The state says so until a save works again; closing reports it | `veil_core`, in `offline`: something is put in the way of the file the library writes |
| A cancel arrives when the transaction has just been sent | The send stays shown as sent | `veil_core`, in `regtest` |
| Tor does not start while a wallet is being created | No wallet is left on disk, and the next attempt makes one | `veil_core`, in `offline` |
| A send is asked for when the last synchronisation failed, or before any | The wallet synchronises first; if it cannot, the send fails for that reason and nothing is built | `veil_core`, in `offline` |
| A transaction is cut off on its way to a node that had answered | It is reported the same way, as unknown; a synchronisation that does not show it settles it without calling it failed, and a new send goes out once | `veil_core`, in `regtest` |
| The node a transaction would go to is down | It is asked first whether it is there and given nothing; the transaction goes to the next node that answers, the node synchronised with if no other does. If none answers, the send fails as not sent | `veil_core`: which node would be asked and chosen, against fake nodes in `offline`; a payment that goes out that way, in `regtest` |
| Payments went to addresses far apart | A wallet restored from the phrase finds all of them and continues after the last | `veil_wallet/tests/regtest.rs`, in `regtest` |
| The app is killed, or the wallet locked, in the middle of a long scan | What was scanned up to the last save is on disk, at most minutes old; locking does not wait for the scan; the next session scans the rest and misses nothing | `veil_core`: that a command and closing stop a synchronisation and that it goes on afterwards, against fake nodes in `offline`; with blocks and a payment over a slowed line, in `regtest` |

Connections are lost on purpose by `veil_tor`'s stand-in for Tor, which
can close one at a chosen point, and can let a node's answers pass slowly,
as Tor does. The fake node in `veil_core`'s tests is a few lines that
answer a node's first questions and have no blocks; it is not a Monero
node and starts no daemon.

A scan that is stopped in the middle with blocks behind it needs real
blocks, so that part runs only on a private chain. Its timing rests on an
estimate of how large a thousand blocks of such a chain are; if a run
shows the estimate wrong, the line's speed in the test is what to change.

Not covered: a power cut. The wallet library replaces the data file by
writing a new one and renaming it, which a killed process cannot break,
but it does not ask the system to put the new file on the disk first.
After a power cut the file may be damaged; the wallet then opens from its
keys as described above.

## Hosted CI and the hosting account

A GitHub account that hosted this project was suspended shortly after a CI
job connected to the Tor network. GitHub does not publish what its abuse
detection reacts to, may suspend an account without notice, and applies the
same limits to free and paid plans. The rules below keep the project well
inside those limits.

GitHub's [terms for Actions](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features#actions)
forbid cryptomining on any runner, self-hosted ones included, and allow
GitHub-hosted runners only for building, testing, and publishing the
repository's own software. Generating blocks on a private chain is mining as
far as that wording goes. The terms do not mention Tor; that rule comes from
experience.

**Allowed in a workflow**

- Formatting, linting, and compiling for Linux, Windows, and Android, the
  app included.
- Tests that touch nothing outside the runner. Loopback connections are
  fine.
- Downloading dependencies from the package registries, the system
  libraries a build needs from the distribution, and the one pinned
  version of Flutter from its publisher, checked against its checksum.
  The first build with Flutter fetches the matching engine files from the
  same publisher. The same for the one pinned release of the Android NDK,
  the compiler the core is built for Android with: from its publisher,
  checked against its checksum.
- Keeping what a job has built with its run, as a file to download from
  the run's page: the app built for Windows, with the core next to it. It
  stays on GitHub, for whoever may see the repository, for fourteen days.
  The wallet library is never in it: hosted CI does not build it.

**Never in a workflow, on GitHub's runners or on a self-hosted one**

- Connecting to the Tor network: starting the Tor client, or running tests
  with `--include-ignored`.
- Starting a Monero daemon in any mode, a private chain included, and so
  never `--features regtest` in a test run.
- Connecting to stagenet, mainnet, or anyone's node.
- Downloading prebuilt daemon or Tor binaries.
- Hashing benchmarks, or long jobs that use the processor for anything but
  the build.

Building the wallet library from the Monero sources is compilation and the
terms allow it, but it is a half-hour job on mining-related code. It stays
out of hosted CI until the owner decides otherwise.

**Workflow files**

- Triggers are `pull_request` and `workflow_dispatch` only. No `push`, no
  `schedule`.
- A job runs `check.sh offline` and no other level, for the system it
  runs on and with `--android`, which builds for Android and runs nothing.
  Before them it may run `check.sh flutter` and `check.sh --android ndk`,
  which install the pinned Flutter and the pinned Android NDK and do
  nothing else.
- Actions are pinned to commit hashes and runner images to a version.
- `permissions` stays at `contents: read`.

**Repository settings, kept by the owner**

- Workflows from all outside contributors need approval before they run.
  The repository is public, and a stranger's pull request can otherwise run
  anything under the project's name.
- Only the actions the workflows use are allowed.
- The workflow token is read-only.

**Account habits**

- One account, with two-factor authentication.
- No bursts of automated activity: pushes are grouped, pull requests are
  opened when the owner asks, and a failed run is fixed, not re-run until it
  passes.
- After each merged step the owner keeps a bundle of the repository
  (`git bundle create`) outside GitHub.

**Cloud sessions**

Cloud development sessions compile and run the tests that need no network.
They do not start a Monero daemon or connect to the Tor network unless the
owner says so for that session, whatever the session's network settings
allow.

## Components

Run every command from `native/`.

| Path | What it is | Check it with |
|---|---|---|
| `native/veil_tor` | Embedded Tor client and loopback SOCKS listeners (Rust) | `cargo test -p veil_tor --lib -- --include-ignored` |
| `native/veil_wallet` | Wallet operations on top of the Monero wallet library (Rust) | `cargo test -p veil_wallet --features regtest` with the library built |
| `native/veil_core` | The wallet joined to Tor: the only way the app reaches a wallet (Rust) | `cargo test -p veil_core`, and `--features regtest` with the library built |
| `native/veil_cli` | `veil`, a command-line wallet for trying the core on stagenet (Rust) | `cargo test -p veil_cli` with the library built |
| `native/wallet_lib` | Pinned build of the Monero wallet library, Veil's patches to it, and the bindings generator | `native/wallet_lib/build.sh <dir> --with-daemon` |
| `native/veil_test_guard` | Makes the tests that start Tor or a daemon fail at once on a CI runner | `cargo test -p veil_test_guard` |
| `dart/veil_bridge` | The Dart side of the core: starts a session in the native library and reads its state | `dart test` there, after `cargo build -p veil_core` |
| `app` | The app, written with Flutter: the way to an open wallet, Home, Receive, Send and Settings | `flutter analyze` and `flutter test` there |

Before committing: `./check.sh offline`.

Hosted CI lives in `.github/workflows`. It builds on Linux and Windows and
runs the tests that need neither a network nor the wallet library. It starts
on pull requests and by hand. Its limits are in
[Hosted CI and the hosting account](#hosted-ci-and-the-hosting-account).

## Third-party code

Veil's own code is MIT. The Monero wallet library it loads is LGPL-3.0
(monero_c) and BSD-3-Clause (Monero). It must be shipped as a separate
shared library with its licence texts and source. Details are in
[`native/wallet_lib/README.md`](../native/wallet_lib/README.md).

The app built for Windows by hosted CI carries three libraries of
Microsoft's C++ runtime (`msvcp140.dll`, `vcruntime140.dll`,
`vcruntime140_1.dll`), copied from the machine that built it, as Flutter's
instructions for handing an app to somebody say. That build is for trying
the app, not a release. How a release provides them, with the app or by
asking for Microsoft's own installer, is decided at the release step.

## Releases

- Builds are reproducible: anyone can rebuild a release and get the same
  binary.
- Release artifacts are signed. The signing keys stay with the maintainer and
  are provided to CI as secrets.
- Android builds contain no Google services and target F-Droid as well as
  direct APK downloads.
