# Veil concept

Veil is a hot Monero wallet for desktop and Android. This document describes
what it does, how it protects the user, and how it will be built.

## Idea

A wallet with four operations: create, restore, receive, send. All traffic
goes through a built-in Tor client. Every incoming payment arrives at a new
address. There are very few settings, because the safe behaviour is the
default and cannot be turned off.

Veil is a hot wallet: the keys live on a device that is connected to the
internet. It is meant for everyday amounts, not for savings.

## Priorities

In this order when they conflict:

1. **Reliability.** The user must not lose funds because of Veil.
2. **Simplicity.** A person who has never used Monero can receive and send.
3. **Anonymity.** Nobody on the network learns who the user is or what they
   own.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Cryptography and transactions | The official Monero `wallet2` library, called through the `monero_c` FFI wrapper | Veil writes no cryptography of its own. Its transactions look the same as those of the official wallet. |
| Core | A Rust library that owns the Tor client and the wallet | The rules that protect the user (Tor only, fresh addresses, exact amounts) live in one memory-safe place that the interface cannot bypass. |
| Tor | Arti, the Rust implementation of Tor, embedded in the app process | No separate Tor app is needed. The code is memory-safe. |
| Bridges | obfs4, Snowflake, WebTunnel | Tor is blocked in a number of countries. Without bridges the wallet cannot connect there. |
| User interface | Flutter: Android, Windows, and Linux from one codebase | The Flutter and `monero_c` combination is already used by other Monero wallets. |

The fallback for Tor is the reference C implementation. It will replace Arti
if, at build time, Arti cannot run bridges on Android.

## Network and anonymity

- **Fail closed.** The app has no direct internet connection. If Tor is not
  running, the wallet does not work. It never falls back to a direct
  connection.
- **Onion nodes only.** The app ships with a list of vetted `.onion` Monero
  nodes and picks one at random on each start. The user can set their own
  node, which is the most private option.
- **Local scanning.** Blocks are downloaded and scanned on the device. The
  view key never leaves it. Veil does not use light-wallet servers, because
  they require the view key.
- **Separate Tor circuits.** Synchronisation uses one node. A transaction is
  broadcast through a different node on a separate circuit, so that no node
  can link the sync history to the spend.
- **No third-party requests.** No analytics, no crash reports, no Google
  services, no push notifications. Fiat rates are off by default; when turned
  on, they are fetched over Tor.
- **Standard transactions.** Fee, ring size, and extra fields come from
  `wallet2` unchanged, so the wallet cannot be recognised by how its
  transactions look.

## Receiving: a new address for every payment

- Veil uses Monero subaddresses (they start with `8`). The primary address
  (starts with `4`) is never shown.
- The Receive screen shows one fresh subaddress as text and as a QR code.
  Once a payment arrives at it, the address is marked as used and the next
  one is shown.
- A new address is issued only after the previous one was used, or when the
  user explicitly asks for one, and the number of unused addresses in a row
  is capped. Otherwise a wallet restored from the phrase could miss payments:
  `wallet2` checks a limited number of addresses ahead (200 by default).
- Old addresses keep working. Funds sent to them still arrive.

## Sending

- The user pastes or scans an address, enters an amount, and sees a review
  screen with the fee and the total before anything is sent.
- The address is shown in groups of four characters with the beginning and
  the end highlighted, so it is easy to compare with the original.
- When choosing which coins to spend, the wallet prefers coins that arrived
  at a single subaddress. If coins from different addresses must be combined,
  it warns the user, because combining them can link those addresses.

## Recovery phrase and storage

- **Creation.** A new wallet gets a 16-word Polyseed phrase with the creation
  date encoded in it. Restoring therefore does not require the user to
  remember a block height, which is the main cause of "restored but the
  balance is zero" complaints.
- **Showing the phrase.** It is shown once and then verified by asking for a
  few of the words. Screenshots are blocked on that screen.
- **Restoring.** Both Polyseed (16 words) and the classic Monero phrase
  (25 words) are accepted. For the classic phrase the app asks for an
  approximate creation date.
- **Encryption.** The wallet file is encrypted with the user's password. On
  Android, biometric unlock is available through the hardware-backed key
  store.
- **On-device protection.** Auto-lock, clipboard clearing, and Android cloud
  backup disabled.

## Screens

1. **Start:** Create wallet, or Restore from recovery phrase.
2. **Home:** balance, sync status, Tor status, Receive and Send buttons,
   recent activity.
3. **Receive:** QR code and address.
4. **Send:** address, amount, then a review screen with the fee.
5. **Settings:** node, Tor bridges, password, auto-lock, show recovery
   phrase, theme, language.

See [design.md](design.md) for the visual language.

## Build plan

Each step is written with tests that need no network, and ends with an
acceptance run on the owner's machine before the next step starts. What may
run where is set out in [development.md](development.md).

1. **Core without a user interface.** `wallet2` plus Tor as a command-line
   tool on the Monero stagenet: create, restore, sync over `.onion`, send.
   1. *Checks in one command.* A script with the levels `offline`,
      `regtest`, `tor`, and `stagenet` that writes a log. The levels that
      touch a network refuse to start inside hosted CI. First hosted CI run,
      on Linux and Windows.
   2. *Wallet through Tor.* The piece that points the wallet library at the
      Tor component's SOCKS listeners and fails closed without them. Tested
      against a stand-in proxy first, then on a private chain and over Tor.
   3. *Command-line tool* on stagenet.
   4. *Bridges:* obfs4, Snowflake, WebTunnel. This moves ahead of step 1.2 if
      the owner's machine cannot reach Tor without them.
   5. *Wallet library for Windows.* Until it exists, wallet tests on Windows
      run inside WSL2. It has to be done before step 3.
2. **Reliability tests.** Automated tests for restoring, and for behaviour
   when Tor drops, the node changes, or the process is killed in the middle
   of a write. The wallet file is saved atomically. The scenarios run
   against stand-ins without a network, and are repeated on the real network
   on the owner's machine.
3. **User interface** on top of the finished core, desktop first.
4. **Android.** Background sync through a foreground service, battery
   restrictions, a build without Google services.
5. **Hardening.** Network-level leak checks, fuzzing of address and URI
   parsing, an external audit.
6. **Release.** Open source, reproducible builds, signed APKs, F-Droid.

## Known limitations

- **Sync over Tor is slow.** A new wallet's first sync covers about a month
  of blocks, as a safety margin around its creation date. Restoring an old
  wallet can take hours.
- **A remote node cannot steal funds,** but it can delay transactions and try
  to feed the wallet distorted data. Choosing randomly among several nodes
  reduces this. Running your own node removes it.
- **A hot wallet is not protected against a compromised device.**
- **Polyseed is not supported by the official Monero GUI wallet.** Feather,
  Cake, and several other wallets do support it.

## Open decisions

- **Name.** "Veil" is also the name of an existing privacy cryptocurrency
  (ticker VEIL). This can cause confusion and possibly trademark objections.
- **Recovery phrase format.** Polyseed (16 words) is planned for new wallets.
  The alternative is the classic 25-word phrase, which every Monero wallet
  understands but which requires a creation date when restoring.
- **Tor implementation.** Arti is used and works on Linux and Windows.
  Bridges and the Android build are still to be verified.
- **Monero protocol upgrade.** Veil builds on Monero 0.18.5, the current
  release line. The FCMP++ and Carrot upgrade, which changes key derivation
  and transaction structure, has not been released; the wallet library will
  have to be updated when it is.
