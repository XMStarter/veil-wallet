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
  it, and is the one asked first from then on: a node that is down is not
  tried again every minute. The user can set their own node, which is the most private option.
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
  that has seen only part of the chain. If the last synchronisation
  failed, or there was none since the wallet was opened, the wallet
  synchronises first, and if it cannot, the send fails for that reason.
- **On a phone, an open wallet goes on outside the screen.** Android
  stops an app it cannot see within minutes, and a first synchronisation
  takes longer than anybody holds a phone. While a wallet is open the app
  therefore asks Android to keep it running, which Android does for an
  app that shows a notice. The notice says that a wallet is open, and
  whether it is synchronising, and nothing else: no amount and no
  address, and nothing at all on a locked screen. The phone is kept from
  sleeping only while the wallet connects, synchronises or sends. It all
  ends when the wallet is locked, which it does by itself as on a
  desktop. Going back from the first screen puts the app behind the
  others. Taking it off the list of recent apps ends it the way Android
  ends any app, at once; the wallet is then as it was last saved. From
  Android 15 on, Android gives such an app six hours a day outside the
  screen; after them the wallet waits until the app is opened. A phone
  that is set to stop the app outside the screen is not argued with: Home
  says so, and where in Android's settings that is changed. The app does
  not ask to be left out of the phone's battery saving.
- **Separate Tor circuits.** Synchronisation uses one node. A transaction is
  broadcast through a different node on a separate circuit, so that no node
  can link the sync history to the spend.
- **No third-party requests.** No analytics, no crash reports, no Google
  services, no push notifications. The one notice the app shows on a
  phone, that a wallet is open, is made on the phone and comes from
  nowhere. Fiat rates are off by default; when turned on, they are
  fetched over Tor.
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
  its amount if it names one. On a phone a code is read with the camera,
  on the device itself: the picture goes nowhere, and the camera is asked
  for the first time a code is to be read.
- The address is shown in groups of four characters with the beginning and
  the end highlighted, so it is easy to compare with the original.
- Before a transaction is handed to a node, the node is asked whether it
  is there. Public nodes are often down; one that does not answer is
  passed over, having been given nothing, and the next one is asked. If
  the only node that answers is the one the wallet synchronises with,
  the transaction goes through that one, on a Tor circuit of its own:
  a payment that cannot be sent is worse than a node that can guess that
  somebody spends. If no node answers at all, nothing was sent, and the
  wallet says so plainly; that can be tried again without any risk.
- If the connection is lost while a transaction is being handed to the
  node, the wallet does not know whether it went out. It says so, and
  looks for the transaction after every synchronisation, in blocks and
  among the transactions nodes are still holding. If it finds it, the
  payment is shown as sent. Until then it sends nothing else, because a
  second send could pay twice. If a synchronisation five minutes or more
  after the attempt still does not show it, the wallet says that it most
  likely never went out and lets the user send again, with the warning
  that the recipient is paid twice if the first one arrives after all. It
  never reports such a send as failed by itself. The wallet writes such
  a payment to disk the moment it happens, so that it is remembered when
  the app is closed or killed, and takes it up again the next time it is
  opened.
- A payment goes out as one transaction or not at all. A wallet with
  very many small coins may need several transactions for a large
  amount; these would be sent one after another and could go out in
  part. Such a payment is refused, with the advice to send less.
- Pressing Cancel never takes back a payment that has been sent. If the
  two meet, the payment stays shown as sent.
- An amount is written with a point or a comma for the decimals. A comma
  followed by exactly three digits, as in `1,250`, could also mean a
  thousand and more, and is refused.
- The wallet library chooses which coins to spend and takes them from a
  single subaddress as long as that one holds enough. If coins from
  different addresses have to be combined, the review says so, because
  combining them can link those addresses.

## Recovery phrase and storage

- **Creation.** A new wallet gets a 16-word Polyseed phrase with the creation
  date encoded in it. Its files are written only once Tor has started,
  so a failed start leaves nothing behind. Restoring therefore does not require the user to
  remember a block height, which is the main cause of "restored but the
  balance is zero" complaints.
- **Showing the phrase.** It is shown once and then verified by asking for a
  few of the words. On a phone, no picture can be taken of the app while
  a phrase is shown or typed: no screenshot, no recording, and no preview
  among the recent apps. Every other screen can be photographed, which is
  how a problem gets reported.
- **Restoring.** Both Polyseed (16 words) and the classic Monero phrase
  (25 words) are accepted. For the classic phrase the app asks for an
  approximate creation date. (For now it asks for a block number instead:
  the core cannot turn a date into a block yet.)
- **Damaged files.** The keys are in a file of their own that is written
  when the wallet is made and again only when its password is changed. If
  the file that holds the wallet's view of the chain is damaged, the
  wallet opens from the keys and scans the chain again. The user's own
  nodes were in that file and are gone with it: the wallet says so and
  does not go to the public nodes by itself, but waits to be told to
  synchronise or to be given nodes again. A record of a payment whose
  outcome was unknown is lost the same way.
- **One wallet, one window.** While a wallet is open, a second window of
  the app, or the command-line tool, does not open it: two programs on
  the same files would overwrite each other's view of the chain. The
  second is told that the wallet is open elsewhere.
- **A wallet that cannot be saved.** If the wallet cannot be written to
  disk, it says so for as long as that lasts, and again when it is
  closed, instead of looking like any other wallet until what it learned
  is gone.
- **Changing the password.** Whoever knows the current password can set a
  new one. The keys file is written again under it, as a new file that
  takes the old one's place, so that an interruption leaves a wallet that
  opens with one password or the other.
- **Removing the wallet.** The wallet can be taken off the device: its
  keys, its data, and what the app remembered about it. It asks for the
  password and says first that only the recovery phrase brings the wallet
  back. The funds are not touched; they belong to whoever has the phrase.
  Nothing of the wallet lies anywhere but in its own folder: the wallet
  library would keep a database of its own outside it, shared by every
  Monero wallet on the machine, and Veil's build of the library does not
  make it.
- **Encryption.** The wallet file is encrypted with the user's password. On
  Android, biometric unlock is available through the hardware-backed key
  store.
- **On-device protection.** A way to lock the wallet at any time and
  auto-lock. An address that was copied is taken out of the clipboard a
  minute later, if it is still what the clipboard holds; a phone lets an
  app look into the clipboard only while it is in front, so there it can
  take until the app is opened again. On Android the wallet's files are
  copied to no backup and to no other device. A locked wallet
  is closed: Tor is stopped, and only the password opens it again. The
  wallet locks itself after five minutes without use; that can be set to
  one, fifteen or sixty minutes, and not switched off. It does not lock in
  the middle of a synchronisation, which a long first scan would otherwise
  never finish, and locks as soon as that is over. A phone that slept
  through that time locks the wallet the moment the app is in front
  again: the time is the clock's, and a sleeping phone's timers stand
  still. The recovery phrase is shown again only for the password.
- **What the wallet remembers besides funds.** The user's own nodes and a
  payment whose outcome is unknown are kept inside the wallet's data
  file, encrypted with it: which node somebody uses and which
  transaction is theirs is nobody else's business. Language, theme,
  auto-lock and bridges are kept in a plain file: they are no secret, and
  bridges have to be known before a wallet is open.

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
      theme, language, and removing the wallet. With it, in the core: the
      password checked and changed, the user's own nodes and a payment
      whose outcome is unknown kept in the wallet, and bridges through the
      C interface.
   8. *The app on Windows:* the build from hosted CI with the wallet library
      next to it, run on the owner's machine.
4. **Android.** The same core and the same app on a phone, on 64-bit ARM.
   1. *The core for Android.* The core and the wallet library built for
      Android from a desktop, with `check.sh --android`, and hosted CI
      building the core for Android. Nothing built this way is run yet.
   2. *The app on a phone.* The Android side of the app with the core and
      the wallet library inside the package, the wallet in the app's own
      storage, a build without Google services, installed on the owner's
      phone: from a new wallet to a payment on stagenet.
   3. *What a phone asks for.* No screenshots of the recovery phrase, no
      cloud backup of the wallet, the clipboard cleared after an address
      was copied, Share on Receive, scanning a code on Send.
   4. *Background synchronisation.* While a wallet is open the app is
      kept running outside the screen by a service with a notice, and the
      phone awake while the wallet is busy; the auto-lock by the clock;
      and what Android's battery restrictions do to all of it.
   5. *Biometric unlock* through the hardware-backed key store.
   6. *Bridges on Android:* whether an app can ship and start a transport
      program.
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
- **A payment that is built twice can show a node which coin it spends.**
  A transaction hides the coin it spends among decoys, which the wallet
  asks its node for. The official wallet writes down which decoys it
  chose, in a database outside the wallet's folder, and takes the same
  ones again. Veil keeps no such database: it would stay behind when the
  wallet is removed and show that a Monero wallet was used on the device,
  and a damaged one would keep every wallet from opening. So when a
  payment is reviewed, cancelled and made again, or sent again after a
  failure, the node is asked twice, and the coin is what the two requests
  have in common. The node learns which earlier transaction the money
  came from. It learns no address and no amount.
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
  Android is still to be verified. Arti itself runs there: inside the app
  on a phone it connected and carried a synchronisation and a payment.
- **The app's name on Android.** Android knows an app by an identifier
  that can never change once people have the app. For now it is
  `io.github.xmstarter.veil`, after the project's place on GitHub. It is
  to be settled, with the name, before anything is published.
- **The compiler of the wallet library for Android.** Monero's dependency
  build brings its own Android compiler, a release of 2018, and the
  library is built with it; the core and the app are built with the
  current one. Each of the two libraries is complete in itself, so they
  work side by side. Building Monero's dependencies with the current
  compiler would mean maintaining that build ourselves, which has not
  been tried.
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
