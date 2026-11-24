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
  nodes and picks one at random on each start. If that node stops answering
  in the middle of a synchronisation, another one from the list continues
  it. The user can set their own node, which is the most private option.
- **Local scanning.** Blocks are downloaded and scanned on the device. The
  view key never leaves it. Veil does not use light-wallet servers, because
  they require the view key.
- **A long scan does not hold the wallet.** The first synchronisation of a
  restored wallet can take an hour. Every two minutes of it the wallet
  saves what it has scanned, so that a crash or a killed app costs
  minutes and not the hour. Locking the wallet, showing an
  address or the recovery phrase do not wait for it either: the scan stops
  after the blocks it is working on, normally within seconds, and goes on
  afterwards or the next time the wallet is opened. A send asked for
  during a scan is built when the scan has finished, never from a wallet
  that has seen only part of the chain.
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
  one is shown. The code holds a payment link, `monero:` followed by the
  address, which other Monero wallets read.
- A new address is issued only after the previous one was used, or when the
  user explicitly asks for one, and the number of unused addresses in a row
  is capped. Otherwise a wallet restored from the phrase could miss payments:
  `wallet2` checks a limited number of addresses ahead (200 by default).
- Old addresses keep working. Funds sent to them still arrive.

## Sending

- The user pastes or scans an address, enters an amount, and sees a review
  screen with the fee and the total before anything is sent. A payment
  link, `monero:` followed by an address, is read like an address, with
  its amount if it names one. (Scanning comes with the phone.)
- The address is shown in groups of four characters with the beginning and
  the end highlighted, so it is easy to compare with the original.
- If the connection is lost while a transaction is being handed to the
  node, the wallet does not know whether it went out. It says so, and
  looks for the transaction after every synchronisation, in blocks and
  among the transactions nodes are still holding. If it finds it, the
  payment is shown as sent. Until then it sends nothing else, because a
  second send could pay twice. If a synchronisation five minutes or more
  after the attempt still does not show it, the wallet says that it most
  likely never went out and lets the user send again, with the warning
  that the recipient is paid twice if the first one arrives after all. It
  never reports such a send as failed by itself.
- The wallet library chooses which coins to spend and takes them from a
  single subaddress as long as that one holds enough. If coins from
  different addresses have to be combined, the review says so, because
  combining them can link those addresses.

## Recovery phrase and storage

- **Creation.** A new wallet gets a 16-word Polyseed phrase with the creation
  date encoded in it. Restoring therefore does not require the user to
  remember a block height, which is the main cause of "restored but the
  balance is zero" complaints.
- **Showing the phrase.** It is shown once and then verified by asking for a
  few of the words. Screenshots are blocked on that screen.
- **Restoring.** Both Polyseed (16 words) and the classic Monero phrase
  (25 words) are accepted. For the classic phrase the app asks for an
  approximate creation date. (For now it asks for a block number instead:
  the core cannot turn a date into a block yet.)
- **Damaged files.** The keys are in a file of their own that is written
  once. If the file that holds the wallet's view of the chain is damaged,
  the wallet opens from the keys and scans the chain again.
- **Encryption.** The wallet file is encrypted with the user's password. On
  Android, biometric unlock is available through the hardware-backed key
  store.
- **On-device protection.** A way to lock the wallet at any time, auto-lock,
  clipboard clearing, and Android cloud backup disabled. A locked wallet
  is closed: Tor is stopped, and only the password opens it again.

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
3. **User interface** on top of the finished core, desktop first. Until
   the release steps the app works on stagenet only, like the command-line
   tool.
   1. *The core for an app.* The core is one thread that waits for a node.
      A layer on top runs it on a thread of its own, takes commands, and
      keeps a picture of the state an interface shows: Tor, synchronisation,
      balance, addresses, history, a send under review. Tested against the
      stand-ins and on a private chain.
   2. *Bridge to the interface.* A small C interface to that layer, like the
      Tor component's, and the Dart side of it. Commands and state cross it
      as text, so nothing is generated and no bridging package is needed.
   3. *App shell.* The Flutter project, the visual language of
      [design.md](design.md), English and Russian, tests of the screens,
      and hosted CI compiling it for Linux and Windows.
   4. *Start:* create a wallet, show and check the recovery phrase, restore,
      set a password.
   5. *Home and Receive.* The balance, Tor and synchronisation in words,
      recent activity; the address as a QR code and as text, copied or
      replaced by a new one. The app synchronises by itself while the
      wallet is open. With it, in the core: a synchronisation that can be
      stopped and that saves along the way.
   6. *Send:* form, review, confirmation, and the case where the outcome is
      not known. With it, in the core: the total and the addresses whose
      coins are spent, for the review; a lack of funds as a problem of its
      own kind; and the search for a transaction whose outcome is unknown.
   7. *Settings:* node, bridges, password, auto-lock, recovery phrase,
      theme, language.
   8. *The app on Windows:* the build from hosted CI with the wallet library
      next to it, run on the owner's machine.
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
  Bridges work through it on Linux for obfs4 and Snowflake; WebTunnel is
  set up the same way but has not been tried. A transport is a separate
  program that Tor starts, and whether an app can ship and start one on
  Android is still to be verified, as is the Android build.
- **Antivirus on Windows.** An antivirus on the test machine reported the
  wallet library as a potentially unwanted application, a "coin miner", the
  first time a program opened it, and it has to be allowed by hand. The
  library does not mine; the detection is of Monero code as such, and the
  official Monero wallet meets the same. A Windows release needs an answer:
  signed files, reporting the false positive to the vendors, and a page
  that tells the user what they are looking at.
- **Monero protocol upgrade.** Veil builds on Monero 0.18.5, the current
  release line. The FCMP++ and Carrot upgrade, which changes key derivation
  and transaction structure, has not been released; the wallet library will
  have to be updated when it is.
