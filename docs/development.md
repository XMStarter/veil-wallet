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
| Unit and widget tests | On a developer machine and in hosted CI | Logic, parsing of addresses and amounts, screen layout |
| Wallet on a local chain | On a developer machine | Create, restore from phrase, receive on new subaddresses, send. No internet and no real coins. |
| Platform builds | Hosted CI | The code builds for Linux, Windows, and Android |
| Tor and stagenet | On a machine the project controls | Connecting through Tor and bridges, syncing and sending on a real test network |
| Real devices | By hand, before a release | Installing and using the app on a phone and a desktop |

Hosted CI (GitHub Actions) only compiles the code and runs tests that touch
nothing outside the runner. No job there connects to the Tor network or
starts a Monero daemon. Hosting providers' automated abuse detection treats
both as suspicious whatever the purpose, so those tests run on machines the
project controls.

Reliability tests cover the failures that lose money or confuse users: Tor
dropping mid-sync, the node changing, the process being killed during a
write, and restoring a wallet whose payments went to many subaddresses.

## Components

| Path | What it is | Check it with |
|---|---|---|
| `native/veil_tor` | Embedded Tor client and loopback SOCKS listeners (Rust) | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |

## Releases

- Builds are reproducible: anyone can rebuild a release and get the same
  binary.
- Release artifacts are signed. The signing keys stay with the maintainer and
  are provided to CI as secrets.
- Android builds contain no Google services and target F-Droid as well as
  direct APK downloads.
