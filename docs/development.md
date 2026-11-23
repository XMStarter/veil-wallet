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
| `offline` | Format, lint, and every test that needs no network, the reliability tests against stand-ins among them. Uses the wallet library when it has been built. | Anywhere, hosted CI included |
| `regtest` | The wallet on a private chain, alone and behind a stand-in for Tor, with and without lost connections. Starts a Monero daemon. | The owner's machine |
| `tor` | The tests that start the real Tor client, a connection to an onion service, and a new wallet synchronising with a public node. | The owner's machine |
| `stagenet` | The command-line tool: a new wallet synchronises with a public stagenet node through Tor. | The owner's machine |
| `bridges` | Tor through obfs4 and Snowflake bridges, a bridge that does not exist, and the tool synchronising through bridges. | The owner's machine |

`./check.sh setup` prepares a Debian or Ubuntu machine once: system
packages, among them the transport programs for bridges, Rust, and the
wallet library with the daemon, built from source into `~/veil-wallet-lib`
(about half an hour). Run it again when the
patches in `native/wallet_lib/patches` change; it rebuilds only what
changed, and the other levels say so when the library is out of date.

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

### Reliability tests

They cover the failures that lose money or confuse users. Each is tested
without a network first, and those that need real blocks are repeated on a
private chain.

| Failure | What must hold | Where it is tested |
|---|---|---|
| The process is killed while the wallet is being saved | The wallet opens afterwards and is the same wallet | `veil_wallet/tests/crash.rs`, in `offline`: a process that saves in a loop is killed at a different moment each round |
| The wallet's data file is damaged: emptied, cut short, or overwritten | The wallet opens from its keys file, which is written once and not touched again; the damaged file is kept; a synchronisation finds the funds again | `veil_wallet/tests/offline.rs`, in `offline`; with funds, in `regtest` |
| The connection is lost in the middle of a synchronisation | The failure is reported, as the node's or as Tor's; the balance is the last one that was complete; the wallet stays usable | `veil_core` against a fake node, in `offline`; on a private chain in `regtest` |
| The node stops answering for good | Another node continues the synchronisation and finds what the first did not deliver | `veil_core`: that the next node is asked, in `offline`; that it finishes, in `regtest` |
| The connection is lost while a transaction is handed to the node | The send is reported as neither done nor failed; the next synchronisation shows it, and the payee is paid once | `veil_core`, in `regtest` |
| Payments went to addresses far apart | A wallet restored from the phrase finds all of them and continues after the last | `veil_wallet/tests/regtest.rs`, in `regtest` |

Connections are lost on purpose by `veil_tor`'s stand-in for Tor, which
can close one at a chosen point. The fake node in `veil_core`'s tests is a
few lines that answer a node's first questions and have no blocks; it is
not a Monero node and starts no daemon.

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

- Formatting, linting, and compiling for Linux, Windows, and Android.
- Tests that touch nothing outside the runner. Loopback connections are
  fine.
- Downloading dependencies from the package registries.

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
- A job runs `check.sh offline` and no other level.
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

## Releases

- Builds are reproducible: anyone can rebuild a release and get the same
  binary.
- Release artifacts are signed. The signing keys stay with the maintainer and
  are provided to CI as secrets.
- Android builds contain no Google services and target F-Droid as well as
  direct APK downloads.
