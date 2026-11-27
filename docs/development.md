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
  The one test that goes to the main network makes a new, empty wallet
  there and sends nothing.
- **An app holds real money only when it was built to.** The network is
  said when the app is built (`VEIL_NETWORK=mainnet` in front of
  `check.sh`); a build that is told nothing is for stagenet, and so are
  the tests.
- **No way around Tor in an app.** Tests have two: a wallet that goes
  straight to a daemon on the machine (the `regtest` feature) and
  listeners that lead to local ports instead of into Tor (`stand-in`).
  Neither compiles in an optimised build, which is what an app is made
  of, and `check.sh` shows that after it has built the core for an app.
- **Every `unsafe` block says why it is sound**, in a comment that begins
  with `SAFETY:`. The lint that asks for it is on for the whole workspace.

[audit.md](audit.md) is where somebody who is to examine Veil starts. It
names the code that keeps each promise and the test that shows it, so a
change to one of those belongs in that document too.

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
| `tor` | The tests that start the real Tor client, a connection to an onion service, and, on the main network with an empty wallet and no funds: a new wallet synchronising with one of the public nodes an app is built with, where such a wallet begins to scan, and every one of those nodes asked where it is and whether it agrees with the others. | The owner's machine |
| `stagenet` | The command-line tool: a new wallet synchronises with a public stagenet node through Tor. | The owner's machine |
| `bridges` | Tor through obfs4 and Snowflake bridges, through obfs4 again on the same files after Snowflake, a bridge that does not exist, and the tool synchronising through bridges. | The owner's machine |
| `leaks` | The Rust tests of `offline` once more, with every network call of theirs recorded: none may name an address outside the machine. Needs the wallet library and `strace`; Linux only. | Any Linux machine with the wallet library |
| `leaks-chain` | The same for the tests of `regtest`. Starts a Monero daemon, whose own calls are left out. | The owner's machine |
| `fuzz` | The tests on made-up input once more, each for a minute and from a seed of this run's own. Uses the wallet library when it has been built. | Any machine; not hosted CI |

The two `leaks` levels check the rule that nothing reaches the network
except through Tor, from outside the programs. In the tests Tor is a
stand-in on loopback, so every address any of their network calls names
has to be the machine's own. `strace` records the calls of the tests and
of everything they start, the command-line tool included, into
`native/target/check-logs/`, and `veil-trace-check` (`veil_test_guard`)
reads the record. It fails on an address that is not loopback, on a name
looked up (port 53, the system's resolver, or its cache of names asked
for a host), on a socket bound to every interface, and on anything it
cannot read. A question to the cache of names about a user or a group is
no finding: Tor's code asks who owns its files, and the machine's own
lists answer. What these levels do not see: the real Tor client, whose
connections leave the machine and are meant to; the app's window and
Flutter's own code; and anything on Windows or Android.

### Made-up input

Whatever reads text or bytes that come from outside has a test that hands
it made-up input by the thousand: good inputs damaged in one or a few
places, two good ones joined, the characters parsers trip over (nothing,
very long runs, signs and separators, other scripts, zero bytes, text
that is no text), and bytes from nothing. Each test says what has to hold
for every input, which is more than "it does not fall over" wherever more
can be said:

| What is read | What has to hold |
|---|---|
| An amount (`veil_wallet/src/amount.rs`) | It is the amount written, by a second reading with wider numbers, or it is refused; written out, it reads as the same amount. |
| A recovery phrase (`seed.rs`) | 16 or 25 words or refused; the words as typed, in small letters; nothing of it in `Debug` output. |
| A node (`veil_core/src/node.rs`) | An onion name of letters, digits, hyphens and dots, and a port in digits, or refused. |
| A name in a SOCKS request (`veil_tor/src/onion.rs`) | What is let through ends in `.onion`, and Arti itself takes it for an onion service or refuses it: never for a host to reach through an exit relay. |
| The bytes of a SOCKS handshake (`socks.rs`) | The listener goes on serving, ends the connection, and asks Tor for an onion service or for nothing. |
| Bridge lines (`bridges.rs`) | Bridges, or refused with the number of the line; lines that say something are never a direct connection. |
| A start request and a command of the C interface (`veil_core/src/ffi.rs`) | Read as what they say or refused with one of the reasons that exist; a refusal does not repeat the recovery phrase. |
| Bytes handed to the C interface | An answer in JSON every time, and nothing started. |
| What a node answers (`veil_core/src/tests.rs`) | The synchronisation ends, no block counts as scanned, the balance is nothing, and the program is still there. This one reaches the wallet library's own reading of a node. |
| An address, and a phrase of the right length, in the wallet library (`veil_wallet/tests/offline.rs`) | Only a whole address passes; a phrase is refused and leaves no file, or is a wallet. |
| A record of network calls (`veil_test_guard/src/trace.rs`) | It is read, and a connection outside is found after whatever stands before it. |
| A code or a paste, a phrase, lines of bridges and nodes, the settings file (`app/test/made_up_test.dart`) | Nothing is added to an address or an amount; words are letters; a settings file of any content costs settings and nothing else, and its bridges are the ones written in it or are said to be lost, never quietly none. |

The input comes from `veil_test_guard::fuzz` for Rust and
`app/test/made_up.dart` for Dart, and from a seed. In the `offline` level
the seed is always the same and each test gets three thousand inputs
(fewer where one costs a wallet or a synchronisation), so that level
passes or fails the same way every time. `./check.sh fuzz` runs the same
tests, the ones with `made_up` in their names, for a minute each from a
seed taken from the clock, and writes the seed into the log. A failure
names the seed, the number of the input and the input; the test run by
itself with `VEIL_FUZZ_SEED` and `VEIL_FUZZ_CASES` as the message gives
them fails on it again: a number of cases that is asked for is run in
full, past the limit a costly test sets itself. `VEIL_FUZZ_SECONDS` sets
another time. The test of Tor's listener makes no more than forty
connections a second after its first four hundred: every case is a
connection, and a system gives the port of a closed one out again only
after a while, so a minute at full speed used up all that Windows has.

What this is not: fuzzing guided by coverage, where a tool watches which
code an input reaches and steers towards the rest. For readers of this
size, damaged and random input finds most of what there is; for the
wallet library, which is large and not ours, it only scratches. Not
covered at all: Arti's own reading of what the Tor network sends, the
wallet library's reading of blocks and transactions, and the Kotlin side
of the app.

A new reader of outside input gets a test of this kind in the same
change, with `made_up` in its name.

`./check.sh setup` prepares a Debian or Ubuntu machine once: system
packages, among them the transport programs for bridges, Rust, the
wallet library with the daemon, built from source into `~/veil-wallet-lib`
(about half an hour), the libraries the app's Linux build needs, and
Flutter, which brings Dart, into `~/veil-flutter`.
Flutter is one pinned version, checked against its published checksum, and
its tools are told not to report usage. Run it again when the
patches in `native/wallet_lib/patches` change, or the way the library is
built (`native/wallet_lib/built-how.txt`); the other levels say so when
the library is out of date. The library is built at one path on every
machine, `/srv`, in a view of the folders that the build makes for
itself; `native/wallet_lib/README.md`, "Where it is built", says how and
what that asks of the machine.

`./check.sh core` and `./check.sh app` are no checks either: the first
builds the core as an app carries it and nothing else, the second the
app with its native parts, as `offline` does after its checks.
`native/twice.sh` builds with them; see "The same build twice".

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

A test program that is ended before it has finished, by a crash or by the
system, ends with something other than the 101 of a failed test, and its
tests have printed nothing by then. A Windows program started from WSL
hands back only the last byte of how it ended. So when `check.sh` sees
such an ending, at the `offline` level in what cargo prints and at the
`regtest` level in how cargo ends, it asks Windows what it recorded about
programs that ended badly in the last half hour (its Application log:
the program, the part of it the fault was in, and the fault's code), and
runs that program's tests again one at a time with their output, to name
the test. Neither turns a failed level into a passed one.

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

builds the app as one file to install, `app-release.apk`, with the core,
the wallet library and the transport program for bridges inside, and
under WSL copies it to the Windows Downloads folder as `veil.apk`. The
transport program is the Tor Project's lyrebird, built from source by
`native/transports/build.sh` (half a minute; `setup` runs it, and `app`
does if it has not been run): its README says what is downloaded and how
each download is checked. Copy it to a phone with a 64-bit ARM
processor and Android 7 or later, and open it there; Android asks once
whether apps may be installed from where the file was opened. The app is
a real wallet on stagenet and connects to Tor.

`VEIL_NETWORK=mainnet ./check.sh --android app` builds it for the main
network instead, where the coins are real money. Either way it is a
build for trying, and Android is told so: the app for stagenet is
`io.github.xmstarter.veil.stagenet`, "Veil stagenet", and the one for
the main network `io.github.xmstarter.veil.preview`, "Veil preview". The
plain identifier and the plain name belong to the app the release step
builds and signs. So a build for trying sits on a phone beside the
released app and beside the other one, each with its own files, and
cannot be installed over either: removing an app removes its wallet, and
nobody should have to remove the app with their money in it to try a
new build. The same variable in front of `./check.sh offline` decides
which network the app for Linux at the end of that level is for; its
wallets lie in a folder for each network.

The file is signed with a key Android's tools make on the machine that
builds it. That is enough to install it on one's own phone; a second
machine has another key, and Android refuses an update signed with a
different key until the app is removed, which removes its wallet too. A
release is built and signed another way: "Signed packages", under
"Releases".

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
  the run's page: the app built for Windows, with the core next to it,
  which has no wallet library in it; and the unsigned release for Android
  of the workflow below, which has. They stay on GitHub, for whoever may
  see the repository, for fourteen days.
- In the one workflow that builds a release for Android a second time
  (`release.yml`, started by hand): compiling the wallet library and the
  transport program for Android from their sources, with what that
  takes downloaded from its publishers and checked against checksums
  kept in this repository: the Android SDK, the older release of the NDK
  that Monero's dependency build pins, the sources that build asks for,
  and Go. The owner allowed this for the release of November 2026; see
  below.

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
terms allow it, but it is a job of some ten minutes on mining-related
code, and it stayed out of hosted CI until the owner decided otherwise.
The owner did, for one purpose: a release for Android is built a second
time on a machine that is nobody's, and the wallet library is in that
package. So `release.yml` builds the library for Android, when it is
started by hand for a release or when a pull request changes that file,
and never otherwise. The workflow that runs on every pull request,
`native.yml`, still builds no wallet library, and neither builds one for
a desktop, which would build a daemon with it. Whether GitHub's abuse
detection minds is not known; the Monero project builds its own code
there.

**Workflow files**

- Triggers are `pull_request` and `workflow_dispatch` only. No `push`, no
  `schedule`.
- A job of `native.yml` runs `check.sh offline` and no other level, for
  the system it runs on and with `--android`, which builds for Android
  and runs nothing. Before them it may run `check.sh flutter` and
  `check.sh --android ndk`, which install the pinned Flutter and the
  pinned Android NDK and do nothing else.
- The job of `release.yml` runs `check.sh --android setup` and, with
  `VEIL_NETWORK=mainnet` in front, `check.sh --android release`, and no
  other level. `check.sh` refuses every other level on a CI runner,
  `setup` without `--android` among them.
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
[`native/wallet_lib/README.md`](../native/wallet_lib/README.md). The
transport program in the app for Android, lyrebird, is under the GPL,
version 3 or later, and is a program of its own that Tor starts
([`native/transports/README.md`](../native/transports/README.md)).

### Licence texts

A package of the app is built from other people's work, and nearly all
of it asks that its licence text goes along. The texts are kept in the
repository, in `app/assets/licences`, so that every build of the app
has them and a change to them is a change somebody reads:

| File | Whose texts | Where they are read from |
|---|---|---|
| `veil.txt` | Veil's own licence | `LICENSE` |
| `core.txt` | The Rust packages in the core, about 380 | Each package's own folder, by what `cargo metadata` says is in the core on each system (`native/veil_notices`) |
| `wallet-library.txt` | monero_c, Monero and what the library is built from | The source the library was built from (`native/wallet_lib/notices.sh`) |
| `transport.txt` | lyrebird, Go, and the Go packages in the program | The source the program was built from (`native/transports/notices.sh`) |
| `typefaces.txt` | The three typefaces | `app/fonts` |
| `android.txt` | The libraries an app for Android is built with, all under the Apache License | A list written by hand in `native/notices.sh` |

Flutter's own texts and those of the Dart packages are put into the app
by Flutter when it is built. The app shows all of them under Settings,
About Veil, with the version and the commit the build was made from.

`native/notices.sh` writes the files and `native/notices.sh --check`
says whether they are still right; the `offline` level runs the check as
Linux programs. After a change to `Cargo.lock`, to a pinned version of
the wallet library or of lyrebird, to a typeface or to the Android
libraries, run it without `--check` and commit what changed. Two of the
files can only be checked on a machine that has the source they are
read from, which is there once the wallet library was built for Windows
or Android and the transport program was built; elsewhere the script
says that it left them as they are. In hosted CI that is always so.

The tool reads no licence and judges none: it does not lose the texts.
Whether what a package does with them is enough is a question for
somebody who knows licences, and has not been put to anybody.

The app built for Windows by hosted CI carries three libraries of
Microsoft's C++ runtime (`msvcp140.dll`, `vcruntime140.dll`,
`vcruntime140_1.dll`), copied from the machine that built it, as Flutter's
instructions for handing an app to somebody say. That build is for trying
the app, not a release. How a release provides them, with the app or by
asking for Microsoft's own installer, is decided at the release step.

## Releases

- Builds are reproducible: anyone can rebuild a release and get the same
  binary.
- Release artifacts are signed. The signing keys stay on the maintainer's
  machine and are used there. Hosted CI is never given one: it compiles
  and runs offline tests, and what it builds is for trying, not for
  handing on. See "Signed packages" below.
- A package carries the licence texts of everything it is built from
  (`app/assets/licences`, see "Licence texts" below) and says which
  commit it was built from.
- Android builds contain no Google services. A release is a file to
  download from the repository's releases page; a listing in F-Droid was
  planned and is left out for now, by the owner's decision.

### Signed packages

A release of the app for Android is built in one place and signed in
another, and the two meet in a file.

```bash
cd native
VEIL_NETWORK=mainnet ./check.sh --android release
```

builds the app as a release carries it: under its plain identifier,
`io.github.xmstarter.veil`, and its plain name, for the main network,
which has to be said as for any build for it. It refuses a checkout with
changes that are in no commit, since a release names the commit it was
built from. What it leaves is
`target/packages/veil-<version>-<commit>-unsigned.apk`, and its
checksum. That file is the package with no signature in it. The app's
build signs whatever it makes with the building machine's own key, and
the command takes that signature off again, so the file is the same for
everybody who builds that commit, and no phone installs it: under the
app's own name and with some machine's key it would sit in the released
app's place.

```bash
./release.sh key
./release.sh sign target/packages/veil-<version>-<commit>-unsigned.apk
```

are for the owner's machine. The first is run once. It makes the key for
releases, a file outside the checkout (`~/veil-release-key`,
`VEIL_RELEASE_KEY`) locked with a password that is typed in and kept
nowhere, and prints the checksum of the key's certificate, which is what
a release is known by and goes into `native/release-key.sha256`. Android
takes an update of an app only from the key the app was first signed
with: whoever has the file and the password can hand out a Veil that
every phone takes as an update, and whoever loses either can never
update Veil again. So the file is copied to something that is not
connected to anything, and it is given to no other machine, hosted CI
least of all. The second signs: it refuses a package that is not a
release, asks for the password, has Android's own tool sign
(`apksigner`, in the way Android has verified since version 7), and then
looks at what came out before it keeps it: that the signature holds,
that it is the key the repository names, and that the signed package is
the one it was given but for the signature. The result is
`~/veil-release/veil-<version>.apk`.

The package may be built on another machine than the one that signs it.
What the owner signs is then whatever that machine built, so the owner
builds the same commit too, or has somebody who is not that machine do
it, and holds the checksums of the two unsigned packages against each
other first.

Both scripts use the distribution's packages and need nothing from
Google: `sudo apt install apksigner aapt`.

#### What a download is compared with

Two things, the second for whoever wants to trust nobody.

Who signed it:

```bash
./release.sh who veil-<version>.apk
```

says what the package calls itself, its checksum, and the checksum of
its signer's certificate, and whether that is the key for releases this
repository names (`native/release-key.sha256`):

```
82ab766ac1dfaf805df1aa5af37d4c869a64260a92474dee5076cc25e0b08fcd
```

Android's own `apksigner verify --print-certs` says the same without
this repository.
A phone that has Veil does this by itself at every update; it is the
first installation that nobody checks but the person installing.

What is in it:

```bash
VEIL_NETWORK=mainnet ./check.sh --android release
./release.sh compare veil-<version>.apk target/packages/veil-<version>-<commit>-unsigned.apk
```

builds the commit the release names and holds the download against it. A
signature lies between a package's files and the list of them and
changes neither, so the two are the same byte for byte in their files
and in their list (`native/signature.sh` says where each part lies;
`signature-test.sh` tests it on archives made up for it, in the
`offline` level). The signing tool adds up to 4095 empty bytes after the
last file, which is allowed for and nothing else is.

What this does not give: packages for the desktop. Nothing signs the app
for Windows or for Linux, and neither is built in a way a release could
be made of (`docs/concept.md`, "Open decisions"). And the key itself is
only as good as the machine it lies on and the password it has.

### Making a release

A release for Android, in the order it is made. Each step has to hold
before the next is taken, and what a step showed goes into the release's
entry in [releases.md](releases.md).

1. **The commit.** A commit on `main`, with hosted CI passed on the code
   as it is in that commit: a pull request that changes documents only
   starts no CI, and the code is then the one an earlier pull request
   passed with. The version is the one in `app/pubspec.yaml`; its number
   after the `+` is what Android compares to tell an update from what is
   installed, and it never goes down.
2. **The owner's run on that commit**, as Linux and as Windows programs:
   `./accept.sh --no-app offline regtest tor`. The `tor` level asks each
   public node of the main network an app goes to whether it is there
   and whether it agrees with the rest.
3. **The package, built twice.** `VEIL_NETWORK=mainnet ./check.sh
   --android release` on two machines that have nothing to do with each
   other; the two unsigned packages have one checksum. One of the two is
   hosted CI: the owner starts the workflow `release` by hand on the
   commit (the repository's Actions page, "Run workflow", on `main`),
   and its last step prints the checksum. A package only one machine has
   built is signed on that machine's word alone, and a release that was
   made so says so.
4. **The signature.** `./release.sh sign` on the owner's machine, which
   looks at its own result before it keeps it.
5. **The signed package, held against the unsigned one** by somebody who
   is not the signer: `./release.sh compare`.
6. **On a phone**, installed over the release before it, which Android
   allows only for the same key: the wallet that was there opens,
   synchronises, and a payment goes out.
7. **Published**: a tag `v<version>` on the commit, and a release on the
   repository's releases page with the signed package, the commit, the
   checksum of the signed package, the checksum of the unsigned package,
   which is the one a build of one's own has to have, and the checksum of
   the signing certificate. It says that nobody outside the project has
   audited Veil.
8. **Recorded** in `releases.md`, in a commit after the release.

A release cannot be taken back. A package that is out is out, and a
version number that was used is used: a fault found afterwards is put
right in the next release, with a higher number.

### The same build twice

Whoever builds a release from its commit should get the files that were
released. The first thing in the way of that is the place of a build:
compilers and build tools write the folders they work in into what they
make. `native/twice.sh` looks for exactly that. It builds a part of a
package twice, from the files of the checkout as git knows them, in two
folders that have nothing in common, another path, another depth and for
cargo another home, and compares what comes out byte for byte:

```bash
cd native
./twice.sh core core-android transport-android
./twice.sh wallet wallet-windows wallet-android
./twice.sh app apk
```

It starts nothing it has built. It takes long, since every part is
compiled twice from nothing: the three wallet libraries about two hours
together on four cores. The folders are made in `~/veil-twice`
(`VEIL_TWICE_DIR`) and kept, for a look at what differs.

| Part | What made two builds differ | What is done about it | Two builds now |
|---|---|---|---|
| The core, for Linux and for Android | The path of every source file that can report an error, 528 of them, most under cargo's folder in the home of whoever builds. Two builds differed in 19.6 of 24.3 million bytes, and in size. | Every optimised build of the core names those folders `/cargo`, `/veil/native` and `/veil/target` to the compiler (`release_build` in `check.sh`). | The same |
| The transport program for Android | Nothing: it is built without paths and without the build's own mark (`transports/build.sh`). | | The same, and the same as the file in the packages built before this step |
| The wallet library, for Android | The paths of some 770 source files, and the folder Monero's dependency build installs into. Then, with the paths the same, eight bytes: the name of the commit monero_c makes of its patches, which had the time of the build in it. | It is built at one path on every machine, and those commits have one date (`wallet_lib/README.md`, "Where it is built"). | The same |
| The wallet library, for Windows | The same two things, and then six bytes: the time of linking, which a program for Windows has a place for in its first bytes, and a sum over the file that follows from it. And one that only a second machine could show: the name of Monero's commit, which differed with the builder's settings for git. | The same; the linker is told to leave the time out; and the build reads none of the machine's settings for git. | The same, after both were linked again without the time; and the same as the file a second machine built from nothing |
| The wallet library, for Linux | The same two things, and one more that was no difference but a fault: it looked for Polyseed in the folder it was built in. And one that only a second machine could show: it was compiled for the processor of the machine that built it. | The same; Polyseed lies beside the library and is looked for there; and the build is told to compile for no processor in particular, as the other two are. | The same, both files |
| The app for Android | Nothing in the 80 files of the package, given the same core, wallet library and transport program. The package as a whole differs in its signature, which has chance in it. | `twice.sh` finds where the signature lies in a package and holds everything else to being the same. | The same but for the signature |
| The app for Linux | One file of the bundle, the program itself, in 483 bytes: it was linked with the folder it was built in as the place to look for its libraries, and the final place was written over that afterwards, in the room the folder's name had taken. | It is linked with the final place from the start (`app/linux/CMakeLists.txt`). | The same, all 25 files of the bundle |

All of that is from a cloud session, on 26 November 2026, with the two
builds of a part made one after the other on one machine.

A second machine built the wallet library the same day, the owner's, from
commit `d39e09f`: Ubuntu 24.04 under WSL2, where the build got its view of
the folders as an ordinary user. Its sums were held against the cloud
session's. Polyseed for Linux was the same file. The wallet library for
Linux was not, and not of the same size: left to itself, Monero's build
compiles for the processor it runs on (`-march=native`), so each machine
had made a library for its own, the cloud session's with some 3,400
instructions that only a server processor of the last years has. That was
more than a difference. A package for Linux would have carried a library
that ends with an illegal instruction on a processor older than its
builder's. The build now names no processor (`ARCH=default` in
`wallet_lib/build.sh`), which is what the builds for Windows and Android
always had from Monero's dependency build; the library built that way has
none of those instructions.
The wallet library for Windows came out of the same size on both machines
and with another sum. The two files, laid side by side, differed in
sixteen bytes of thirty-two million: the name of Monero's commit, which
the library carries in two places, and the sum over the file. The cloud
session's machine is set to sign the commits it makes, the build makes
commits of monero_c's patches, and a signature is part of what a commit's
name comes from. The build now reads none of the machine's settings for
git but those it needs for the network. Built again from nothing in a
cloud session, the library for Windows is the owner's file byte for byte
(`c8e9b13b1fdd76e2…`): with Polyseed, the first parts of a package seen
to be the same on two machines.

On commit `bc6c898`, which has all of that and a changed patch, the
owner built both libraries from nothing once more, and all three files
are the cloud session's: the library for Windows (`3a33f0d609df79d4…`),
the library for Linux (`2815398e3e401946…`) and Polyseed
(`93c8dd53fb487964…`). For Linux that says that the two machines had the
same packages of Ubuntu 24.04 that day, and no more than that.

What it does not show, and what is known to stand in the way of the same
files on another machine:

- **Another machine.** The two builds share the machine's compilers.
  Rust is pinned to a version, Go and both Android NDKs by checksum, and
  Flutter by checksum; the C and C++ compilers for Linux and the cross
  compiler for Windows are the distribution's, whatever version it has
  that day. One second machine has built the wallet library, as told
  above; no other part has been built on one and compared.
- **The wallet library for Linux** is linked against the distribution's
  Boost, OpenSSL and the rest, so it is the same only where those are the
  same, and a package for Linux needs them installed. Monero's dependency
  build does not compile for a current Linux
  (`wallet_lib/README.md`). A package for Linux that is complete in
  itself is not there yet.
- **The app for Windows** is built by hosted CI with Microsoft's compiler
  and is not compared with anything.
- **What is downloaded already built**: Flutter's engine, the Android
  libraries Gradle fetches, and the compilers themselves. They are pinned
  or checked by the tools that fetch them, not built here.
- **The commit a build names** is given to the app's build on the command
  line. The folders `twice.sh` builds in are no checkouts, so its two
  apps name none; a release is built from a checkout without changes and
  names its commit.
- **A signature** is made with a key, and no two keys are alike. What a
  download is compared with is the package without it: "Signed
  packages" below.
