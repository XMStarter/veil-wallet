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

## Status

The repository holds the concept and design documents. Code has not been
started. The next step is the core without a user interface, described in
the build plan in `docs/concept.md`.
