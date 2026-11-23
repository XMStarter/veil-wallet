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
- Do not add a hosted CI job that connects to the Tor network or starts a
  Monero daemon. Hosted CI compiles and runs offline tests, nothing else.

## Workflow

- One task, one branch. Do not push to `main` directly.
- Open a pull request only when the owner asks for one.
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
- `docs/`: concept, design, development rules.
- `.github/workflows`: hosted CI, builds and offline tests only. Pin
  actions to commit hashes.

Before committing, in `native/`: `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, and `cargo test -p veil_tor --lib -- --include-ignored`
for the tests that start a Tor client. With the wallet library built, also
`cargo test -p veil_wallet --features regtest` with `VEIL_WALLET_LIB` and
`VEIL_MONEROD` set.

## Status

Done: the Tor component, and the wallet core (create, restore, receive on
fresh addresses, send) tested on a private chain. Not done: the piece that
connects the two and the command-line tool on stagenet, bridges, the app.
See the build plan in `docs/concept.md`.

## Environment notes

- Cloud sessions cannot reach the Tor network, `downloads.getmonero.org`, or
  GitHub release downloads of other repositories. Anything that needs them
  runs on a machine the project controls, not in hosted CI.
- The wallet library can be built in a cloud session from source with
  `native/wallet_lib/build.sh` (about half an hour; install the packages
  listed at the top of the script first).
- CI results are readable from cloud sessions with `gh api`:
  `repos/XMStarter/veil-wallet/commits/<ref>/check-runs` for the outcome,
  `repos/XMStarter/veil-wallet/actions/jobs/<id>/logs` for a job's log.
- A GitHub account was suspended shortly after a CI job of this project
  connected to the Tor network. Treat the hosted-CI rule above as firm.
