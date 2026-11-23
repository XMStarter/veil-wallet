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
  - `wallet_lib`: pinned build script for the wallet library, Veil's
    patches to it, and the bindings generator.
  - `veil_test_guard`: called first by every test that starts Tor or a
    daemon; fails the test on a CI runner. A new test of that kind calls it
    too.
  - `check.sh`: the checks by level (`offline`, `regtest`, `tor`,
    `stagenet`) with a log; `setup` prepares a machine.
- `docs/`: concept, design, development rules.
- `.github/workflows`: hosted CI, builds and offline tests only. Pin
  actions to commit hashes.

Before committing, in `native/`: `./check.sh offline` (format, lint, and
the tests that need no network; with the wallet library built in
`~/veil-wallet-lib` it also runs the library tests that need no node). The
`regtest` and `tor` levels are the owner's acceptance run, not something a
cloud session runs.

## Status

Done: the Tor component, and the wallet core (create, restore, receive on
fresh addresses, send) tested on a private chain. Not done: the piece that
connects the two and the command-line tool on stagenet, bridges, the app.
See the build plan in `docs/concept.md`. Next is step 1.1: the check script
and the first hosted CI run. Hosted CI has not run in this repository yet.

## Environment notes

- Tests on a private chain, over Tor, and on stagenet run on the owner's
  machine: Windows, inside WSL2 until the wallet library builds for Windows.
  The owner runs them and sends the log.
- What a cloud session can reach depends on its environment's network
  settings, which the owner can change. The rule above about Tor and the
  daemon holds either way.
- The wallet library can be built in a cloud session from source with
  `native/wallet_lib/build.sh` (about half an hour; install the packages
  listed at the top of the script first). Building it is fine; `monerod`
  from that build is not started there.
- CI results are readable from cloud sessions with `gh api`:
  `repos/XMStarter/veil-wallet/commits/<ref>/check-runs` for the outcome,
  `repos/XMStarter/veil-wallet/actions/jobs/<id>/logs` for a job's log.
- A GitHub account was suspended shortly after a CI job of this project
  connected to the Tor network. Treat the hosted-CI rule above as firm.
