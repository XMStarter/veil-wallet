# Veil: instructions for Claude

Veil is a hot Monero wallet for desktop and Android. Read
[docs/concept.md](docs/concept.md) before making product decisions,
[docs/design.md](docs/design.md) before touching the interface, and
[docs/development.md](docs/development.md) before committing.
[docs/audit.md](docs/audit.md) names, for each thing Veil promises, the
code that keeps it and the test that shows it: when one of those changes,
that document changes with it.

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
- Do not make the test-only features (`regtest` in `veil_wallet`,
  `stand-in` in `veil_tor`) compile in an optimised build. They are ways
  around Tor.
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
    listeners and a C interface. It reads the loss of its network from
    the connections it is asked for (`State::Lost`); its README says how,
    and what that cannot tell. With bridges it keeps Tor's state in a
    folder of its own that starts empty when the bridges change
    (`state.rs`), because Arti does not take a bridge back.
  - `veil_wallet`: wallet operations over the Monero wallet library. Its
    README lists library behaviours the crate guards against; read it
    before changing `wallet.rs`.
  - `veil_core`: joins the two. The app reaches a wallet only through it;
    it points the wallet at Tor's listeners and nowhere else. `Session` is
    what an interface talks to: the core on its own thread. Tests replace
    Tor with `veil_tor`'s stand-in (the `stand-in` feature, never in an app
    build), which can also cut a connection at a chosen point. The fake
    node in its tests is a few lines of test code that answer a node's
    first questions over loopback. It has no chain and is not `monerod`;
    those tests run only where the wallet library is built, so not in
    hosted CI.
  - `veil_cli`: `veil`, the command-line wallet on stagenet. Stagenet
    only; do not add another network to it. The app is built for
    stagenet too unless `VEIL_NETWORK=mainnet` stands in front of
    `check.sh`; no test, script or default may make it the main
    network's.
  - `wallet_lib`: pinned build script for the wallet library, Veil's
    patches to it, and the bindings generator. After changing a patch, run
    `build.sh` again and regenerate the bindings. It builds at one path
    on every machine, `/srv`, in a view of the folders it makes for
    itself, so that the library does not differ with the folder it was
    built in; when the way it builds changes, say so in `built-how.txt`,
    and every work folder is built again from nothing. `--android` builds it
    for Android on 64-bit ARM from a source tree of its own, with a patch
    only that build has (`patches/android`); the README says why.
  - `transports`: pinned build script for the transport program the app
    for Android brings along for bridges (the Tor Project's lyrebird,
    for obfs4, Snowflake and WebTunnel), and why it is built the way it
    is. It is under the GPL; the README says what that means.
  - `veil_notices`, `notices.sh`: the licence texts a package of the app
    carries for everything it is built from (`app/assets/licences`).
    `./notices.sh` writes them and `--check` says whether they are still
    right; after a change to `Cargo.lock` or to a pinned version of the
    wallet library or of lyrebird, write them again and commit them.
    `docs/development.md`, "Licence texts".
  - `veil_test_guard`: called first by every test that starts Tor or a
    daemon; fails the test on a CI runner. A new test of that kind calls it
    too. It also starts the daemon for tests, one at a time, and when the
    daemon does not come up it says how it ended and shows the end of its
    log. And it reads a record of network calls for anything that leaves
    the machine (`trace.rs`, `veil-trace-check`), for the `leaks` levels.
    And it makes up input for the tests of whatever reads text or bytes
    from outside (`fuzz.rs`); such a test has `made_up` in its name, and
    a new reader of outside input gets one.
  - `check.sh`: the checks by level (`offline`, `regtest`, `tor`,
    `stagenet`, `bridges`, `leaks` and `leaks-chain`, which run the
    tests of the first two with every network call recorded, and `fuzz`,
    which runs the tests on made-up input for a minute each from a seed
    of the run's own) with a log;
    `setup` prepares a machine;
    `daemons` is a diagnosis of the test daemon, not a check. With
    `--windows` in front, the levels are built for Windows on Linux and run
    as Windows programs through WSL. With `--android` in front there are
    four: `ndk` installs the pinned Android NDK, `setup` adds the Android
    SDK, the wallet library and the transport program for Android,
    `offline` lints and builds for Android and runs nothing, and `app`
    builds the app as an APK with the core, the wallet library and the
    transport program inside. `release`, which wants
    `VEIL_NETWORK=mainnet` in front and a checkout without changes,
    builds the app under its plain identifier and name and leaves the
    package without a signature.
  - `release.sh`: makes the key for releases and signs with it, on the
    owner's machine only, and says for anybody whether two packages
    for Android are the same but for their signatures and who signed
    them. The key is a file outside the checkout; its password is typed
    and kept nowhere. Never make, ask for, or handle the real key or its
    password in a cloud session; a throwaway key in a scratch folder
    (`VEIL_RELEASE_KEY`, `VEIL_RELEASE_DIR`) is how the script is tried
    there. `signature.sh` finds where a signature lies in a package and
    is read by it and by `twice.sh`; `signature-test.sh` tests that on
    made-up archives in the `offline` level. `docs/development.md`,
    "Signed packages".
  - `twice.sh`: builds a part of a package twice, in two folders that
    have nothing in common, and says whether the two came out the same
    byte for byte: the core, the transport program, the wallet library,
    the app. It starts nothing it has built. An optimised build of the
    core goes through `release_build` in `check.sh`, which keeps this
    machine's folders out of it; build the core for an app no other way.
    `docs/development.md`, "The same build twice".
  - `accept.sh`: the owner's acceptance run in one command, step after
    step: the latest commits, `check.sh` as Linux and as Windows programs,
    the logs to send, then the app. It starts a daemon, so it is not for
    a cloud session, where only `--no-pull --no-app offline` may be tried.
  - `windows-app.sh`: for the owner's machine, inside WSL. Takes the app
    hosted CI built for Windows (`veil-windows.zip`, downloaded from the
    run's Artifacts), puts the wallet library next to it on a Windows
    disk, and starts it. It refuses a wallet library built from an older
    set of patches. It starts Tor, so in a cloud session only its
    refusals and `--no-start` on made-up folders (`VEIL_WINDOWS_HOME`) may
    be tried.
- `dart/veil_bridge`: the Dart side of the core. It calls the C interface
  in `veil_core/src/ffi.rs`; the state crosses as JSON, pinned by
  `native/veil_core/tests/fixtures`, which both sides test against.
- `app/`: the app, written with Flutter, for Linux, Windows and Android
  (`app/android`; the libraries are found there by name, and what only
  the system can do goes through `lib/core/device.dart` to
  `MainActivity`: where the app's files go, no picture of a screen with
  a phrase, Share, a code read with the camera, the clipboard emptied,
  the app kept running while a wallet is open, which is
  `WalletService`, and the password kept by Android for a fingerprint or
  a face, which is `Biometrics`).
  So
  far the way to an open wallet (create, recovery phrase, restore, unlock),
  Home, Receive, Send, and Settings, whose last line leads to About
  Veil: the version, the commit, and the licence texts.
  Screens talk to `WalletBackend` (`app/lib/core`), never to the core
  directly; tests use a backend that only pretends, and no test opens a
  real wallet, because that starts Tor. Read
  `app/README.md` and `docs/design.md` before changing it. Colours,
  typefaces and sizes come from `app/lib/theme`; texts from the `.arb`
  files, always in both languages.
- `docs/`: concept, design, development rules, what an audit starts
  from, and `releases.md`, what has been released, from which commit and
  with which checksums; an entry is written after its release.
  `SECURITY.md` at the top says where a finding goes.
- `.github/workflows`: hosted CI, builds and offline tests only. Pin
  actions to commit hashes. `native.yml` runs on pull requests that
  change code; it installs Flutter with `check.sh flutter`, not with an
  action, and its Windows job keeps the app it built as the artifact
  `veil-windows`. `release.yml` is started by hand, by the owner, on the
  commit a release is built from: it builds the unsigned release for
  Android a second time and prints its checksum, which means compiling
  the wallet library for Android there, and that the owner allowed for
  this and for nothing else. The owner has to allow a new action in the
  repository's settings before a workflow may use it.

Before committing, in `native/`: `./check.sh offline` (format, lint, and
the tests that need no network; with the wallet library built in
`~/veil-wallet-lib` it also runs the library tests that need no node). The
`regtest` and `tor` levels are the owner's acceptance run, not something a
cloud session runs.

## Status

Done: the Tor component, and the wallet core (create, restore, receive on
fresh addresses, send) tested on a private chain. Step 1.1 of the build plan
is done too: the check script, hosted CI passing on Linux and Windows, and
the owner's acceptance run passing at the `offline`, `regtest`, and `tor`
levels. Step 1.2 is done as well: `veil_core` connects the wallet to Tor,
and the owner's acceptance run passed at all three levels, including a
send through a separate node on a private chain and a new wallet
synchronising with a public onion node over Tor. Step 1.3 is done: `veil`,
the command-line tool on stagenet (`veil_cli`), passed the owner's
acceptance run at all four levels, and a send tried by hand on stagenet
went through a separate node and confirmed. A new wallet's first sync on
stagenet took about two minutes over Tor. Step 1.4, bridges, is done
(`veil_tor::Bridges`, `veil sync --bridges`): the owner's acceptance run
passed at all five levels. Tor connected through obfs4 and through
Snowflake, did not connect when its only bridge did not exist, and the
tool synchronised a wallet through obfs4 bridges, about three and a half
times slower than directly. WebTunnel is configured like the other two but
cannot be tried yet. Step 1.5, the wallet library for Windows, is done:
`wallet_lib/build.sh --windows` cross-builds the DLL and `monerod.exe`, and
`./check.sh --windows` runs the checks as Windows programs. On the owner's
machine the `offline`, `regtest`, `tor` and `stagenet` levels pass that
way, after three fixes those runs found (never unloading the library,
UTF-8 paths, and not trusting the library alone about an existing wallet)
and after the antivirus was told to allow the DLL and `monerod.exe`. That
completes step 1 of the build plan, the core without a user interface.
Step 2, reliability tests, is done: the owner's acceptance run passed as
Linux and as Windows programs. Tested there: a process killed while
saving, a damaged data file (the wallet opens from its keys), a
connection cut in the middle of a synchronisation (retried, reported, or
taken over by another node), a send whose answer is lost (reported as
unknown, found in the history, paid once), and a restore past long runs
of unused addresses. The same run closed what step 1.5 had left open: the
`bridges` level after the library was rebuilt, and the `tor` level as
Windows programs. In the first run of this step one of two test daemons
did not come up on Linux. The cause was never seen. `./check.sh daemons`
then started eighty daemons in pairs there, with and without loopback
ports being taken around them: all came up, the slowest in 4.2 seconds
against a limit of sixty, so neither a race for ports nor a slow start
showed. It has not happened again, and the daemon helper reports it if it
does. The table of what
is tested where is in `docs/development.md`, "Reliability tests". Next is
step 3, the user interface, in the eight parts listed in the build plan.
Step 3.1 is done: `veil_core::Session` runs the core on a thread of its
own for an app, with commands that return at once and a state to draw
from. The owner's acceptance run passed at the `offline` and `regtest`
levels as Linux and as Windows programs, the scenario with funds
included: receiving, a send reviewed and cancelled, a send confirmed, a
send whose answer is lost, and the same wallet in a new session. Step 3.2
is done: the C interface to the session (`veil_core/src/ffi.rs`) and its
Dart side (`dart/veil_bridge`). The owner's acceptance run passed at the
`offline` and `regtest` levels as Linux and as Windows programs, with
Flutter installed by `./check.sh setup` and the Dart tests calling the
real library on Linux. The Dart side has not been run against the Windows
build of the library: there is no Dart for Windows in that setup. Next is
step 3.3, the app shell, which also brought Flutter to hosted CI. It is
done: the Flutter project in `app/` with the visual language of the
design document, English and Russian, and the start screen. On the
owner's machine the `offline` level passed with the app's checks and its
Linux build, and the built app opened its window under WSL. In hosted CI
the app was built for Windows and the Dart side was tested against the
Windows build of the core, both for the first time, and passed. Step 3.4,
the start flow, is done: creating a wallet with its password, showing and
checking the recovery phrase, restoring from a phrase, opening the wallet
on the device, and locking it, on stagenet. Its tests walk that way on a
wallet that only pretends. On the owner's machine the `offline` and
`regtest` levels passed with it, and the owner walked through the app
with a real wallet and reported, after the step had been merged, that the
check of three words worked, Tor connected on the screen after opening,
the wallet opened with its password after the app was started again, and
locking worked. Restoring in the app was not part of that report. Step 3.5, home and
receive, is done. In the core a synchronisation can now be stopped: the
session saves every two minutes of a scan, and any command, closing
included, stops the scan, is dealt with, and the scan goes on. In the app:
Home (balance, Tor and synchronisation in words, recent activity), Receive
(QR code, address in groups of four, copy, new address), and the wallet
synchronises by itself while open. The owner's acceptance run passed at
the `offline` and `regtest` levels as Linux and as Windows programs, the
test that stops a scan with real blocks behind it included
(`a_long_scan_is_saved_on_the_way_gives_way_and_misses_nothing` in
`veil_core`). As Windows programs that test first failed on a step of its
own: it copied the keys file of an open wallet, which the wallet library
keeps locked, and Windows lets nobody else read a locked file. It now
copies that file while the wallet is closed. In the app with a real
wallet the owner confirmed that the synchronisation starts by itself once
Tor is connected, that Receive shows the code and the address and Copy
works, and that locking in the middle of a synchronisation takes seconds.
The owner also reported that the QR code works. No payment has been
received in the app yet. Step 3.6, send, is done.
In the app: a form (address, with paste and payment links, and an amount
or everything), a review (address, amount, fee, total, a warning when
coins from several addresses are combined), and the outcome. In the core:
the review's total and source addresses, a lack of funds as a problem of
its own, and a send whose outcome is unknown is looked for after every
synchronisation, becomes sent when found, blocks other sends until then,
and is settled, never failed, when a synchronisation five minutes later
does not show it. The owner's acceptance run passed: the `offline` level
as Linux and as Windows programs on commit `3c9a853`, and the `regtest`
level both ways on commit `22f44f8`, which differs from it in one test of
`veil_wallet`, one test of `veil_core`, and the change described next; the
`offline` level was run on that last commit in a cloud session only. The
runs found three things. An older test still expected the library's bare
refusal where a lack of funds now has its own error. The session reported
the end of a synchronisation in one change of the state and what it said
about a lost send in the next, and a test, like an interface, could read
the state in between; both are one change now. And the test of the
synchronisation timer failed once as Windows programs while the machine
was busy with the daemons, having passed on the same code before; its
cause was not proven, it now waits as long as the other tests, and it
passed in the next run. Shown on a private chain, on both systems: a
transaction whose answer was lost is found among the node's waiting
transactions before any block and reported as sent; no other send is built
meanwhile; a send to a node that cannot be reached is reported as unknown,
settled by a synchronisation, and sent again, paid once. The owner
reported that sending works in the app. Not tried: a lost connection in
the app itself, which cannot be brought about by hand. Step 3.7,
settings, is done; what its acceptance took is told below. In the app: a Settings screen
with the user's own node, Tor bridges, the password, the auto-lock (one to
sixty minutes, never off, and never in the middle of a synchronisation),
the recovery phrase shown again for the password, language, a dark theme,
and the wallet removed from the device for the password; colours now come
from `VeilPalette.of(context)`. In the core: the password checked and
changed (`Wallet` keeps the one it was opened with in memory, because the
library's own copy is empty for a wallet made in the same session), the
user's own nodes and a send whose outcome is unknown kept in the wallet's
data file as encrypted notes, bridges through the C interface with their
programs looked for by `veil_core::transports` (moved from `veil_cli`),
and a check of bridge lines that starts nothing. The owner asked for the
removal of the wallet and for the unknown send to be remembered on disk
as part of this step. The offline tests pass in a cloud session. On the
owner's machine, on commit `9cbd182`, everything passed as Linux and as
Windows programs, the changed password and the notes in the wallet
included, except one thing in two tests: a session reported its wallet
open and, in a second change of the state, the send an earlier session
had left unknown, and the tests read the state in between. It is one
change now, said last. That failure stopped the `offline` level before
it built the app, so the app the owner then started was the one of the
step before, without Settings. On commit `8d27d89` the `offline` and
`regtest` levels passed as Linux and as Windows programs, a session
closed and opened again while a send was unknown included. In the app
with a real wallet the owner then confirmed the changed password, the
phrase shown for the password, theme and language kept over a restart,
and the removal of the wallet; the auto-lock was not checked; and with a
wrong password the app hung. Which screen was not said. Found for it,
with tests that fail without the fix: on the screen that opens the
wallet, the backend closed the refused session on another isolate with a
function made inside one of its methods, which took the backend and,
through its listeners, the screens along, so the call failed with an
error the screen did not catch and it said "opening" for ever (the tests
had nobody listening to the backend). Fixed with it: screens show any
failure and go on; a password field keeps the keyboard through a
refusal; and the C interface no longer locks the handle for the whole of
a call that waits, which froze the window while a password was checked
during a synchronisation. The owner confirmed that a wrong password no
longer hangs the app, and the run on that commit, `e7acc48`, passed at
the `offline` and `regtest` levels both ways. The owner then
asked for a review of the whole code for faults and for things that only
look as if they worked, and for the findings to be fixed. Fixed after
it, with offline tests unless said otherwise: a send whose outcome is
unknown was written into the wallet but saved only by the next
synchronisation or by closing, so a kill in between lost it; it is saved
at once now (shown on a private chain only, by a copy of the data file
taken while the session is open). A wallet that cannot be written to disk
says so in the state (`not_saved`), on Home, and on the screen that
closing leads to; before, nothing showed it. A cancel that arrives when
the transaction has just gone out no longer takes "sent" off the state:
`cancel_send` drops only what has gone nowhere, and `clear_send` is the
new command for an outcome the user has seen (private chain only). A
transaction is built only after a synchronisation of the same session
that finished; otherwise the session synchronises first, and a send fails
with the synchronisation it waited for. A new wallet's files are written
after Tor has started, not before. A payment that would need several
transactions is refused (`SeveralTransactions`): the library would send
them one by one and stop at the first failure. A wallet opened from its
keys alone waits to be told before it goes to any node, because the
user's own nodes were in the lost file, and says so. An amount with a
comma and exactly three digits after it (`1,250`) is refused as
ambiguous; with a point it is taken as it stands. Changes of the state
that belong together are one change in two more places. In the app: "not
now" is no longer shown as a wrong password, a phrase that was never
checked and cannot be read closes the wallet instead of opening Home,
and locking or closing the window survives a failure of the backend.
Left for tasks of their own, by the owner's decision: the Tor component
reported itself ready from the end of its bootstrap on and did not
notice a network that goes away afterwards, so the difference between a
failure of Tor and of the node existed in the tests only (taken up after
step 3.8, see the end of this section); and the concept says that no node can link
synchronisation and spending, while the node synchronised with is asked
for what a transaction is built from. Also open: a password change that
fails after the keys file was rewritten is reported as failed although
the new password is in force; and `VEIL_CORE_LIB` and `VEIL_WALLET_LIB`
still name the libraries in a release build. On commit `3208ff8` the
owner's run passed at the `offline` and `regtest` levels as Linux and as
Windows programs, the scenario on a private chain included with its two
new checks: the data file as a kill would leave it knows of the send
whose outcome is unknown, and a cancel sent right after a confirmation
leaves the send shown as sent. In the app the owner then got "outcome
unknown" for an ordinary send, where sends had gone through at step 3.6.
Nothing on the way of a transaction to its node had changed since then.
The owner asked each of the four public stagenet nodes with `veil sync
--node`: two did not answer. The node a transaction goes to was the
candidate after the one synchronised with, never asked anything before
the send, so with two of four down about every second session could not
send, and the library reports a node that is not there like a lost
answer. Now `PendingSend::commit_through` asks each node of
`Core::broadcast_route` whether it is there before handing it the
transaction, passes over those that do not answer, uses the node
synchronised with if no other answers, and fails as not sent if none
does. Offline tests show which node is asked and chosen; that a payment
goes out that way is in the scenario on a private chain, which also has
a new kind of cut in the stand-in for a transaction that never arrives
(`BeforeMoreRequestBytes`). On commit `2d187fc`, which adds only
`accept.sh` to that, the owner's run passed at the `offline` and
`regtest` levels as Linux and as Windows programs, the reworked scenario
included, and the owner reported that sending works in the app again.
As Windows programs the `offline` level failed the first time in one
test, `a_wallet_survives_being_killed_while_it_saves`, on its first
line: loading the wallet DLL ended with Windows error 995, an operation
aborted, after about half a minute. Nothing under test had run; the same
test passed minutes later in the same run at the `regtest` level, and
the level passed when run again. The cause was not found; it has
happened once. The auto-lock was never tried in the app with a real
wallet: the owner left it to be checked without one, and its tests on the
wallet that only pretends were widened for that (a key counts as use; it
locks from any screen and leaves no phrase on show; a wallet opened again
locks again; the default is five minutes). They pass, and they say
nothing about a real window's clicks and keys. A synchronisation started with the first candidate every
time, so a first node that was down cost every synchronisation the time
it takes to find that out; since step 3.8 was merged the node that
carried a synchronisation to its end is the one asked first the next
time (shown on a private chain only, near the end of the scenario; it
passed there as Linux programs on commit `23882af`). Bridges changed
in Settings take effect the next time the wallet is opened. Step 3.7 was
merged on the owner's word after hosted CI had passed on Linux and
Windows. Step 3.8, the app on Windows, is done; how it was accepted is
told below. Hosted CI, which already built the app for Windows, now makes that build
complete (`check.sh`: the core as a release build and the three libraries
of Microsoft's C++ runtime next to `veil.exe`) and keeps it with the run
as the artifact `veil-windows`; the owner allowed
`actions/upload-artifact` in the repository's settings for that.
`native/windows-app.sh` takes the downloaded file on the owner's machine,
adds the wallet library, and starts the app from a Windows disk. With it:
a wallet that is open holds a lock of Veil's own (`<wallet>.lock`), and a
second window, session or `veil` on the same wallet fails with
`WalletInUse` before anything is started. That was found while reading
for this step: the library locks the keys file of a wallet it has opened
but not of one it has just made, so a second window could open a new
wallet a second time. Tried in a cloud session: the offline level, and
`windows-app.sh` on made-up folders without starting anything. In hosted
CI the two new parts of the workflow worked the first time they ran: the
Windows job took twenty-four minutes, most of it the core's release
build from nothing, and kept an artifact of nineteen files and twenty
megabytes. On commit `52720ee` the owner's run passed at the `offline`
and `regtest` levels as Linux and as Windows programs; as Windows
programs the offline tests of `veil_wallet` once took two hundred seconds
where they take seventeen, and seventeen again minutes later, which
looks like the slow loading over the WSL share seen once before and was
not looked into. The owner then started the app as a Windows program:
it opened a wallet, coins sent to it from another wallet arrived, and
the owner reported that everything works. Which of the things listed
for that walk were tried was not said; a second window on an open wallet
and the auto-lock had never been seen with a real wallet before it. The
two public faucets named at step 1.3 no longer served: one did not work
and the other was blocked by the owner's antivirus as dangerous, so the
coins came from the owner's own stagenet wallet. Step 3.8 was merged on
the owner's word after that. After it, the state of Tor when the network
goes away, written, run on a private chain, and seen to work once with a
real network. Arti's own status
never goes back once it has connected, so `veil_tor` reads the loss from
the connections it is asked for: one that fails on the way to the first
relay makes a ready Tor `Lost`, one that gets through or is answered
from inside the Tor network makes it ready again, and a timeout changes
nothing, because it is also what a node that is not there looks like.
`veil_core` then reports a failure as Tor's (`TorLost`, `tor_lost`), not
as "no node answered", goes on asking every node through Tor, since a
connection that gets through is the only sign of the network being back,
and a session goes on synchronising on its timer meanwhile. A send that
no node took is reported as not sent for Tor's reason; one whose outcome
is unknown stays unknown. Home and Send say it in both languages. The
stand-in loses its network the same way (`set_network`,
`AfterCut::NetworkLost`), in place of the "not ready after a cut" it had,
which a real Tor never does. The offline tests pass in a cloud session;
the scenario on a private chain has a new end, a send confirmed with the
network gone. On commit `23882af` the owner's run as Linux programs
passed at the `offline` level and failed at `regtest` in that new end,
for a mistake of the test's own: it asked for a payment of one and a
half where the three payments before it had left the wallet under a
half. It asks for a tenth now. The part of the
scenario before it, the node that worked being asked first, passed there
for the first time. The owner then took the network away under the app
with a real wallet open and sent two pictures of Home: first "Tor has
lost its connection" with the synchronisation failed for Tor's reason,
then "Tor is connected" and "Synchronised". So Arti does give the kind
of failure that is read as a loss there, and the wallet found the
network again by itself. Which build it was and how the network was
taken away was not said; the only build with this code was the Linux one
of that run, started under WSL. Not seen: the same in the app for
Windows, which hosted CI builds only on a pull request, a send confirmed
with the network gone, and a loss that shows as nothing but timeouts,
which would still read as nodes that do not answer
(`veil_tor::service::network_after` is the place to change then).
That work was merged on the owner's word after hosted CI had passed on
both systems. Next is step 4, Android, in the six parts listed in the
build plan. Step 4.1, the core for Android, is written; nothing of it can
be run before there is an app on a phone, so no run of the owner's is
asked for until step 4.2. `./check.sh --android offline` lints the Rust
code for Android on 64-bit ARM, builds the core as a library, and
compiles the tests; it passed in a cloud session at the first try, Arti
and its cryptography included, with the NDK release the pinned Flutter
uses (`r28c`, pinned in `check.sh` by checksum). The wallet library for
Android is built by `wallet_lib/build.sh --android` in ten minutes with
Monero's own dependency build, which brings the NDK release of 2018 that
Monero pins. Three things make up for that compiler's age, each in
`build.sh`: the system's `libtinfo` offered under its old name to the
compiler, a patch of three lines to a library monero_c adds (`bc-ur`),
applied in a source tree only the Android build uses, and the name of
the C++ runtime told to the linker. The result needs only what every
Android has, exports the sixty-three functions Veil calls, and is laid
out for devices with 16-kilobyte pages, as the core is. None of it has
been loaded by anything. Hosted CI now also builds the core for Android
on its Linux runner, which downloads the pinned NDK; that part of the
workflow has not run yet. Step 4.2, the app on a phone, is written as
far as a cloud session can take it: `./check.sh --android app` builds an
APK of fifty-four megabytes with the app, the core and the wallet
library, for 64-bit ARM and Android 7 or later. Looked at from outside:
its one permission is the network, nothing of Google's services is in
it, Android copies its data to no backup, and its four libraries lie in
the package the way devices with 16-kilobyte pages want them. It is
signed with the build machine's debug key. It has never been installed
or started: whether the core finds the wallet library by name, whether
Tor starts in an app's storage, and how the screens sit on a phone are
all still to be seen, on the owner's phone. The owner installed the
APK, and both ways to a wallet failed: restoring with "this is not a
recovery phrase", making a new one with "the wallet could not be marked
as open ... try_lock() not supported". One cause: Rust's standard
library has no file locks for Android and says so only when asked, so
the mark that keeps a wallet from being opened twice could not be made.
The lock is now taken with the system's `flock` directly on every
system that has it, which is also what the tests on Linux run. The
restore had hidden the reason because the session called every refusal
of a restore a wrong phrase; a mark that cannot be made is an error of
its own now, and an invalid phrase is shown with what the library said.
What that first try did show: the app starts on a phone, the core finds
the wallet library inside the package, and Tor starts in the app's
storage, since the mark is made after all three. The words of a phrase
are now also taken without full stops, commas and numbers, which a
phone's keyboard or a note adds. The change of the lock made an
unrelated test fail every time, the fake node suddenly "incompatible":
the fake did not list the protocol upgrades with its version, the
library then takes a path that can read a variable nothing has set, and
what it decides there moved with the layout of the program. The fake
lists them now, like every current node; the library's side is noted in
`native/veil_wallet/README.md` and not patched. With the APK built from
commit `a5f5c00` the owner then reported from the phone that everything
works and that coins are sent. So on a phone the wallet opens, Tor
connects, the wallet synchronises, and a payment goes out, on stagenet.
What else was looked at was not said: whether the wallet was made new or
restored, how the screens sit, and the phone and its Android are not
known. That APK was built in a cloud session and signed with that
session's debug key; one built on the owner's machine has another key
and does not install over it. The owner said more afterwards: the phone
is a Samsung Galaxy A17; a wallet made new and a wallet restored from
its phrase both work there; Tor connects and the wallet synchronises;
and nothing on the screens is cut off. The version of Android was not
said. On commit `4dafd44` the owner's run passed at the `offline` and
`regtest` levels as Linux programs, with the lock taken the new way, and
at the `offline` level as Windows programs. The `regtest` level as
Windows programs did not finish: the test program of `veil_core` ended
with status 116 after its first seven tests, none of which touches a
wallet, where a failed test ends with 101. Nothing says which test it
ended in, because the tests run side by side and hold their output back.
The cause is not known. Two readings of the number, neither shown: the
last byte of Windows' code for a damaged heap (`0xC0000374`), or
Linux's "stale file handle", which would point at the file share
between WSL and Windows that has failed in other ways before. The same
tests without the `regtest` feature had passed as Windows programs
minutes earlier in the same run, and the level had passed on `39a19a1`.
`check.sh` now runs the tests again one at a time, with their output,
when a test program ends this way, so that the next run names the test;
that part has only been tried with a stand-in for cargo. Run again by
itself on commit `be063de`, which differs only in that script, the
`regtest` level passed as Windows programs, all fifty-three tests of
`veil_core`. So the ending with 116 has happened once and did not come
back; its cause was not found, and a run that passes afterwards does not
show that nothing is wrong. With that, step 4.2 has passed the owner's
run on both systems and on the phone. Hosted CI then passed on both
systems, its Linux job building the core for Android for the first time,
in twelve minutes where it took six, and steps 4.1 and 4.2 were merged on
the owner's word. Step 4.3, what a phone asks for, is written and has
not been on a phone. No picture can be taken of the app while a recovery
phrase is shown or typed (Android's `FLAG_SECURE`, put up by the four
screens that have a phrase on them and taken down when the last is
gone); every other screen can still be photographed, which is how the
owner reports problems. An address that was copied is taken out of the
clipboard a minute later if it is still there, on every system, and on a
phone when the app is next in front if the minute ran out in the
background. Receive has Share and Send reads a code with the camera,
both only where the device can; the code is read by ZXing through
`zxing-android-embedded`, the app's first dependency on Android besides
Flutter, and the camera is its second permission. The icon is the
profile cut out of the logo, until there is a mark made for it. Tests on
the wallet and device that only pretend cover the Dart side of all of
it; the Kotlin side was compiled into an APK and nothing more. With the
APK built from commit `c0c176c`, installed over the one before on the
same phone, the owner went through five things and reported that all
five work: the icon, no screenshot of the screen with the phrase while
Home can be photographed, a copied address gone from the clipboard after
a minute, Share on Receive, and a code read with the camera into the
form on Send. Not run with it: the owner's run on the desktop, which
waits for the change to the wallet library in pull request 16 so that
the libraries are rebuilt once. The wallet library's "shared
ring database", which the library makes outside Veil's own folder on a
desktop, was noticed here; the end of this section tells what was done
about it. On
commit `39a19a1`, which has the amount put right, the owner's run passed
at the `offline` and `regtest` levels as Windows programs, the scenario
with its new end included: a send confirmed with the network gone is
reported as not sent for Tor's reason, and the wallet holds what it held
once the network is back. The log of the Linux programs of that run was
not sent; `accept.sh` runs them first and stops when they fail, so they
passed, which is a conclusion and not something read in a log. After that, the wallet library's "shared
ring database", by the owner's decision switched off. The library made it
when a wallet was opened, before the password was checked, and never
when one was created or restored: `~/.shared-ringdb` on Linux, and on
Windows, by the library's source, in the folder all users share
(`C:\ProgramData\.shared-ringdb`). It stayed when the wallet was
removed, and a database cut short ended the process when any wallet was
opened; both were seen in a cloud session with a temporary home. The
wallet API at the pinned commit has no call to move it or leave it out,
so a patch does it (`monero/0005`), which the Android build applies as
well, and the owner has to rebuild the library for Linux, for Windows
and for Android (`./check.sh setup`, `--windows setup`, `--android
setup`) and allow the new DLL in the antivirus. What is given up: a transaction built twice for the same coin
gets new decoys the second time, which can show the node which coin is
spent (`docs/concept.md`, "Known limitations"). The offline test
`a_wallet_leaves_nothing_outside_its_folder` makes, restores, opens and
uses wallets in a process whose home, temporary folder and working
folder are empty folders and finds them empty afterwards; it fails on a
library without the patch. It passes in a cloud session on Linux. Not run
anywhere yet: the same test as a Windows program, where the shared
folder cannot be moved out of the way and is looked at before and after
instead, the same look around the scenario on a private chain, and
anything on Android, whose library has not been built with the patch.
Where
an older library has left the folder (`~/.shared-ringdb` on the owner's
machine, and likely the one on Windows), the tests can only see that it
was not written to; with the folder deleted first they see that it does
not come back. Seen on the way, and then removed at the owner's word: on
stagenet and testnet the library writes the wallet's primary address as
plain text next to a wallet it creates or restores
(`wallet.address.txt`). `Wallet` now takes that file away whenever a
wallet is made or opened; the library writes it again only where it
finds it, so that is for good, and a wallet made earlier loses it the
first time it is opened. An offline test shows it on all three networks,
a changed password included, and fails without the removal. This needs
no patch to the library.
That work and step 4.3 then met: `main` was merged into the branch of
step 4.3, with one conflict, in this file. In a cloud session the
libraries for Linux and for Android were rebuilt with the new patch, the
offline level passed on the joined code, the test that looks outside the
wallet's folder included, and an APK was built with the rebuilt library.
On that commit, `42132ee`, the owner rebuilt the library for Linux and
for Windows (`./check.sh setup`, `--windows setup`), and the run passed
at the `offline` and `regtest` levels as Linux and as Windows programs.
With it ran for the first time: the test that looks outside the wallet's
folder as a Windows program, the same look around the scenario on a
private chain on both systems, and the removal of the address file as a
Windows program. Whether the folders an older library had left
(`~/.shared-ringdb`, `C:\ProgramData\.shared-ringdb`) were deleted before
that run is not known; if they were still there, the run shows that they
were not written to and not that they would not come back. The owner
installed the APK with the rebuilt library over the one before and
reported that it works; what was asked for was that the wallet opens and
synchronises. With that, step 4.3 has passed the owner's run on both
systems and on the phone. `./check.sh --android setup`, run on the
owner's machine for the first time, did not get past its first download:
Google's server answered 404 for the pinned NDK, where the same address
answers a cloud session with the file. Why is not known; the script now
names the address it could not download. Nothing depends on it while the
APKs are built in a cloud session, and the owner's machine has no
library for Android. Hosted CI passed on both systems, and step 4.3 was
merged on the owner's word.
Step 4.4, background synchronisation, is written and has not been on a
phone. While a wallet is open the app asks Android to keep it running
outside the screen: `WalletService`, a service "in the foreground" of
the kind for synchronising data, with a notice that says a wallet is
open and whether it is synchronising, no amount, no address, and
nothing on a locked screen. It holds the phone awake only while the
wallet connects, synchronises or sends, ten minutes for each time the
Dart side says so, which it does again every minute. It ends when the
wallet is locked. The Dart side (`Device.walletOpen`, called from
`app.dart` on every change of the state) decides all of it. Four
permissions came with it: two for the service, one to show the notice
(asked for once, the first time a wallet is open), one to keep the
processor running. Back from the first screen now puts the app behind
the others instead of ending its screen. When the screen does end with
a wallet open, by the app being taken off the list of recent apps or by
Android destroying it, the process is ended at once: the wallet lives
in the core, only the Dart side can talk to it, and that side goes with
the screen, so the wallet could neither be kept nor closed in order,
and left alone it would go on running unseen and be "open elsewhere"
at the next start. That last was true before this step, wherever
Android left the process alive, and nobody had met it. The auto-lock
now also goes by the clock when the app comes to the front: a phone's
timers stand still while it sleeps. Home says when Android is set to
stop the app outside the screen. Tests on the wallet and device that
only pretend cover the Dart side; the Kotlin side was compiled into an
APK and nothing more. Not known until a phone shows it: whether a
synchronisation goes on for long with the screen off (Android's
documents promise the network in the phone's deep sleep only to apps
exempted from battery saving, which Veil does not ask for; a service in
the foreground with the processor held awake is said to keep it, and
that is from memory of Android's code, not from its documents), what
Samsung adds to Android's restrictions, and how the six hours a day
that Android 15 gives such a service show. With the APK built from
commit `c652005` the owner went through seven things on the phone and
reported that all but one work. Asked for were: the question about
notices, the notice and what it says, a long synchronisation that has
gone on after ten to fifteen minutes with the screen off, the notice
gone after locking, the auto-lock after more than five minutes behind
other apps, and the app taken off the list of recent apps starting
again with the question for the password and no "open elsewhere". The
one: Back on the first screen sometimes closed the app, took the notice
away and started the synchronisation over, and sometimes did not. That
is the screen being ended, and the process with it as written above.
Why only sometimes was not seen on the phone; the reading that fits is
that Android dealt with Back by itself. Flutter tells Android that it
has nothing to go back to on its first screen, and Android then puts an
app behind the others only if it was started from the home screen, and
ends the screen of one started any other way, such as from the screen
that installed it. Whether that is what happened, and which Android the
phone has, is not known. Two things are done about it, each enough
alone: the app now always tells Android that it deals with Back itself
(`onNavigationNotification` in `app.dart`; a test shows it and fails
without it), so Back reaches `MainActivity.popSystemNavigator`, and
`MainActivity.finish` puts the app behind the others instead of ending
its screen, whoever asks. The second is Kotlin and has only been
compiled. With the APK built from commit `1fd8479` the owner tried Back
on Home with a wallet open both ways, the app opened from the screen
that installed it and from the home screen, and reported that both
work: the app goes behind the others and the wallet stays open. Which
of the two changes did it cannot be told from that, and whether the
reading above was the cause was not shown either. No run on the
owner's desktop was made for this step: neither the core nor the wallet
library changed in it, and the offline level passed in a cloud session.
Hosted CI passed on both systems, and step 4.4 was merged on the owner's
word.
Step 4.5, the wallet opened for a fingerprint or a face, is written and
has not been on a phone. The wallet knows only its password, so the
phone keeps the password: encrypted with a key Android makes and holds
in the phone's secure hardware, which works for one use right after a
fingerprint or a face that Android counts as strong was recognised, and
which Android destroys when the fingerprints or faces change
(`Biometrics.kt`: `AndroidKeyStore`, AES in Galois/Counter mode,
`BiometricPrompt` of the system, no library added). The app writes no
cryptography for it; it calls Android's. It is offered on Android 11 or
later and only if the key really is in secure hardware. It is off until
switched on in Settings, for the password and after a screen that says
what it costs: whoever the phone recognises can open the wallet and
send. It only opens the wallet; the phrase, a new password and removing
the wallet ask for the password. A new password switches it off,
removing the wallet removes what was kept, and a kept password the
wallet refuses is forgotten at once. One more permission came with it,
for Android's question. Fifteen tests on the wallet and device that only
pretend cover the Dart side; the Kotlin side was compiled into an APK
and nothing more, so everything about a real reader is still to be
seen. With the APK built from commit `6dee925` the owner reported from
the phone that everything works. Asked for were: the line in Settings,
a wrong password refused and the right one followed by Android's
question for a fingerprint, the wallet opened with the fingerprint and
still with the password, Cancel leaving it closed, a new password
switching it off, switching it off in Settings, and, if wanted, a new
fingerprint in Android ending it. Which of these were tried one by one
was not said; the owner did say that the fingerprint opens the wallet
after a wrong password was typed, which is as meant on the screen that
opens the wallet. So Android's key store and its question work on that
phone as called. No run on the owner's desktop was made for this step:
neither the core nor the wallet library changed in it. Hosted CI passed
on both systems, and step 4.5 was merged on the owner's word.
Step 4.6, bridges on Android, is written and has not been on a phone.
A phone has no transport programs installed and lets an app start only
what came in its package, so the package now carries one: lyrebird, the
Tor Project's program for obfs4, Snowflake and WebTunnel, built for
Android on 64-bit ARM from source by `native/transports/build.sh` in
half a minute. Its source is pinned by commit, its Go packages by its
own `go.sum`, Go itself by checksum, and the C compiler is the pinned
NDK's; built twice in a cloud session in different folders it came out
the same byte for byte. It is built with the NDK's C compiler because a
Go program on Android finds names on the network only through the
system's C library, and with a linker flag one of Snowflake's packages
needs; the README there says why. It goes into the package as
`liblyrebird.so`, untouched, and the package's native parts are now
unpacked when the app is installed, which is the one place Android lets
an app start a program from; the APK is twenty-nine megabytes where it
was fifty-four, because they are compressed in it now. The core looks
for a transport program first in a folder the app names
(`transport_dir` in a start request and in `veil_check_bridges`,
`veil_core::transports::programs_with`), under its own name or as
`lib<name>.so`; the app for Android names the folder Android unpacked
the package into. Offline tests cover the search and the way through
the C interface and the Dart side. Not known until a phone shows it:
whether Arti can start the program there, which is what the concept's
fallback to the reference Tor implementation was about, and whether a
bridge then carries a connection. lyrebird is under the GNU General
Public License, version 3 or later, and Veil under MIT; they are
separate programs, but a package with lyrebird inside has to be handed
on under lyrebird's terms for that part. That is an open decision in
the concept, for the owner, before a release; it does not stand in the
way of trying the app on the owner's own phone. With the APK built from
commit `ecc8c94` the owner put bridges into Settings on the phone, the
obfs4 bridges built into Tor Browser and then one of its Snowflake
bridges, the lines the `bridges` level uses, and reported: with obfs4
Home first said that the wallet could not synchronise because no node
answered and that it tries again by itself, and then that went away;
Snowflake works. So Arti starts the program from the app's package on a
phone and both transports carry Tor there, which answers the question
the concept's fallback was kept for. What Home said after the message
went away with obfs4 was not said; a message about nodes, and none
about Tor, means Tor had connected through the bridge, and nodes that
do not answer at the first try are what the public stagenet nodes have
done before, here through a connection several times slower. Whether
the bridge had a part in it was not looked into. Not tried: WebTunnel,
and a network that blocks Tor, since the owner's does not. On commit
`241dd5f` the owner's run passed at the `offline` and `regtest` levels
as Linux programs and at the `offline` level as Windows programs. The
`regtest` level as Windows programs failed in one test of `veil_core`,
`a_session_is_driven_through_the_interface_from_start_to_close`: after
its sixty seconds the session was still opening, at its very first
state. The seven other tests that open a wallet and started with it
were reported as running for over a minute too, tests that take
seconds, and then passed; they have no limit of their own. The same
test program had passed minutes earlier in the same run without the
`regtest` feature. That is the picture of the wallet DLL taking over a
minute to load the first time in a freshly built program, which this
machine has shown before in other forms; that it was the loading and
not the first call after it was not seen. Nothing of this step is in
that test's way. The tests that wait with a limit now load the library
first, outside what they time, and a loading that takes five seconds or
more says so in the test's output. On commit `d28e3a5`, which has that
change, the `regtest` level passed as Windows programs, all fifty-six
tests of `veil_core`. No test stood at the start of the program this
time, so the slow loading did not come back, and the run does not show
whether the change would have carried the tests through it: a test that
passes shows none of its output, so how long the loading took is not in
the log either. With that, step 4.6 has passed the owner's run on both
systems and on the phone. The owner said afterwards that obfs4 works on
the phone, and about lyrebird's licence: it will be decided before a
release, and the owner is to be reminded of it then. Whoever takes up
step 6, the release, raises it first: `docs/concept.md`, "Open
decisions", "The licence of the transport program". Hosted CI passed on
both systems, and step 4.6 was merged on the owner's word. That
completes step 4 of the build plan, Android.
Next is step 5, hardening, in three parts: a leak check, fuzzing, and
what an external audit needs. Step 5.1, the leak check, is written.
`./check.sh leaks` runs the Rust tests of the `offline` level once more
under `strace`, which records every network call they and the programs
they start make, and `veil-trace-check` (`veil_test_guard/src/trace.rs`)
reads the record: an address that is not loopback, a name looked up, a
socket bound to every interface, or anything it cannot read fails the
level. `leaks-chain` does the same for the tests on a private chain and
leaves the daemon's own calls out. In a cloud session `leaks` passes:
352 sockets and 665 addresses, the core's tests, the wallet's, and
forty-five runs of the command-line tool among them, and none leaves
the machine. Its first run did report something: four connections to
the system's cache of names, from Tor's code. They were its questions
about who owns its files, a user and a group looked up in the machine's
own lists (seen in the call stacks: `getpwuid_r` and `getgrnam_r`), and
the cache was not running, so nothing was even asked. The reader now
judges that cache by what it is asked: about a host is a finding, about
a user or a group is not. That it does fail was tried on a small
program that connects outside, looks a name up and binds to every
interface: seven findings. What these levels cannot see: the real Tor
client, whose connections leave the machine and are meant to, the app's
window and Flutter's own code, and anything on Windows or Android. On
commit `d4e3d2d` the owner ran both levels and sent the log and the two
records. `leaks` passed there with the same numbers as in a cloud
session, 352 sockets and 665 addresses, none leaving the machine.
`leaks-chain` failed, and not for a leak: its record, 360 sockets and
607 addresses, holds nothing that leaves the machine, and read again
without the daemon left out it still holds nothing, so the test daemon
keeps to loopback too. What failed was one test while it was recorded,
`a_long_scan_is_saved_on_the_way_gives_way_and_misses_nothing`: the
daemon did not answer a request for blocks within the five minutes the
helper gives it. The recording stopped every program at every call it
made, the daemon included, and the test program took eleven minutes
where it takes six. Because cargo stops at the first test program that
fails, the wallet's own tests on a private chain were not recorded at
all. The recording now uses the kernel's own filter (`strace
--seccomp-bpf`), which stops a program only at the calls that are
recorded: the wallet's offline tests take 19 seconds that way where
they took 107, with the same record, 26 unrecorded. And a test that
fails no longer keeps the rest from being recorded. The owner ran
`leaks-chain` again on commit `7e79893` and sent the record, not the
log. Read in a cloud session, it holds 436 sockets and 692 addresses
and nothing that leaves the machine, and nothing either with the six
daemons of that run not left out (442 sockets, 866 addresses). The
wallet's own tests on a private chain are in it this time. Whether the
tests passed while recorded the record does not say: it was made
without the lines for how a program ended, which it has from now on
(`strace -q` in place of `-qq`). The log of that run came afterwards
and says it: the level passed. All fifty-six tests of `veil_core`
passed while recorded, the long scan among them, in 311 seconds where
the first recording took 665; so did the wallet's tests, the two on a
private chain included. With that, step 5.1 has passed the owner's run:
`leaks` on commit `d4e3d2d` and `leaks-chain` on `7e79893`. The last
change, keeping how each program ended in the record, has run in a
cloud session only. Hosted CI passed on both systems, and step 5.1 was
merged on the owner's word.
Step 5.2, fuzzing, is written. Whatever reads text or bytes from outside
has a test that hands it made-up input by the thousand and says what has
to hold for every one: thirteen in Rust (`made_up` in their names, the
input from `veil_test_guard::fuzz`) and four in the app
(`app/test/made_up_test.dart`). The table of what is read and what has
to hold is in `docs/development.md`, "Made-up input". In the `offline`
level they run from a seed that never changes, three thousand inputs
each, fewer where one costs a wallet or a synchronisation;
`./check.sh fuzz` runs them for a minute each from a seed of the run's
own, and a failure names the seed and the input. It is not fuzzing
guided by coverage. What it found, each fixed with the test that shows
it: an amount with a comma, three digits after it and something that is
no digit before it was refused as ambiguous, with the advice to write a
point, where it is simply no amount; a node's port was taken with a plus
sign in front, which the standard library reads as a number; and Tor's
listener let a name with a last dot through (`x.onion.`) and handed it to
Arti as it stood. Arti goes by the ending `.onion` when it decides
between an onion service and a host to reach through an exit relay; the
Arti in use refuses a name with a last dot outright, so nothing went
anywhere, but that was Arti's rule and not Veil's. The name is handed on
without the dot now, and a test asks Arti itself what it takes each name
that is let through for (by its `Debug` output, which is no promise of
Arti's). Two more were seen while reading for these tests and are fixed
with them. A settings file with `5.0` for the auto-lock made the app
fail while it read its settings, before any screen: the reader now takes
any JSON. And the settings file was written in place, so the app ended
in the middle left half a file, which reads as no settings and so as no
bridges; it is written next to itself and then put in place. What that
leaves is in the concept under "Known limitations": a settings file
that cannot be read still takes the bridges with it without a word, and
bridges can be set only behind an open wallet, after Tor has started.
That is for the owner to decide about. What found nothing: the wallet
library given six hundred phrases of the right length with wrong words
and three thousand damaged addresses, and a node that answers the
wallet's questions with made-up bytes, which is the one of these tests
that reaches the library's reading of what comes from the network: over
some hundreds of answers, the synchronisation ended, no block counted
as scanned, and the program was still there. That is little against a
library of that size, and it says nothing about blocks and transactions,
which no test here makes up. Tried in a cloud session: the `offline`
level, which passes with these tests in it, `./check.sh --android
offline`, and the `fuzz` level at thirty seconds a test. There the Rust
tests passed, and one of the app's failed on its ninety-five-thousandth
input for a mistake of the test's own: its second reading of an address
took the spaces off differently from the reader, which takes one
character more off the ends than out of the middle (U+0085). With the
test corrected the app's tests passed from the same seed. On commit
`4a9ce46` the owner's run passed at the `offline`, `regtest` and `fuzz`
levels as Linux programs, the `fuzz` level at its full minute a test,
the app's four included, and at the `offline` and `regtest` levels as
Windows programs. The `fuzz` level as Windows programs failed in one
test, the one of Tor's listener, at its 13,628th input, and not for the
input: the test's own connection to the listener was refused by Windows
with "only one usage of each socket address" (error 10048). The reading
that fits the number: every case is a connection of the test's own and
often a second where Tor is asked, Windows keeps the port of a closed
connection aside for two minutes and has about sixteen thousand to
give, and the test had used them up; it was not seen on the machine.
The listener had answered every input before that. The test now makes
no more than forty connections a second after its first four hundred
cases; with that it passes in a cloud session, the input it had stopped
on included, which shows nothing about Windows. Bringing that input back
showed a fault of the tool's own: the failure's message names a seed and
a number of cases to run again with, and a test with a limit of its own
ran only up to its limit, so the run "passed" without reaching the
input. A number of cases that is asked for is run in full now. The other twelve Rust
tests passed there as Windows programs for their minute, the node that
answers with made-up bytes and the wallet library's two among them.
On commit `6316712`, which has those two changes, the owner ran the
`fuzz` level alone, and it passed as Linux programs, the app's four
tests included, and as Windows programs, all thirteen Rust tests for
their minute, the one of Tor's listener among them. So the listener's
test no longer runs out of ports there; that the ports were what it ran
out of is still a reading and was not looked at on the machine. The
app's tests on made-up input have not run as Windows programs: there is
no Flutter for Windows in that setup, and hosted CI runs them there at
their fixed seed. The `offline` and `regtest` levels were not run again
on that commit on the owner's machine; it differs from `4a9ce46` in that
test, in the tool that makes the input, and in documents, and the
`offline` level passed on it in a cloud session. With that, step 5.2
has passed the owner's run on both systems. Hosted CI passed on both
systems, the app's tests on made-up input running as Windows programs
there for the first time, and step 5.2 was merged on the owner's word.
Step 5.3, what an external audit needs, is written: `docs/audit.md`. It
says what Veil is made of and what of it is Veil's own, with the pinned
versions of what is not; for each promise of the concept the code that
keeps it and the test that would fail if it did not; whom the wallet
defends against and whom it does not; where input from outside comes in;
the `unsafe` code by file; where secrets live and for how long; what
lies on the disk and who can read it; how to build it and run its checks;
what is known to be weak; what was never tried; and where we would look
first. Every statement in it was held against the code, and several did
not hold as first written and were put right. Two rules it names were
only comments before and are checked now. The two features that are ways
around Tor for tests, `regtest` in `veil_wallet` and `stand-in` in
`veil_tor`, do not compile in an optimised build (`compile_error!`), and
`check.sh` shows that after each build of the core for an app
(`no_test_features_in_a_release`). And every `unsafe` block has a comment
that says why it is sound: clippy's `undocumented_unsafe_blocks` is on
for the workspace; three blocks in tests of `veil_tor` had none. Written
down for the first time while doing this: the seven functions of
`veil_tor`'s own C interface are exported from the app's core library,
which does not call them; any program on the machine can use the two
SOCKS listeners of an open wallet to reach onion services; and nothing
checks the pinned packages against a list of known vulnerabilities. None
of the three is changed. The audit itself is the owner's to commission,
and the document has no address to send findings to. Tried in a cloud
session: the `offline` level, with the new step in it, and `./check.sh
--android offline`. The new step runs where a core for an app is built:
on the owner's machine and in hosted CI's Windows job, where it has not
run yet. On commit `c54d0de` the owner's run passed at the `offline`
level as Linux and as Windows programs. As Linux programs the new step
ran after the core for the app was built, and all three builds with a
test-only feature were refused. As Windows programs that step does not
run: no app is built there. With that, step 5.3 has passed the owner's
run; `docs/audit.md` itself is for the owner to read, which no run does.
After that, by the owner's word, bridges before a wallet is open. Two
things were wrong, and the second was found while looking at the first.
A settings file that could not be read gave the app its defaults, which
is no bridges, and Tor was then connected to directly without a word.
And bridges could be set only in Settings, behind an open wallet, while
making, restoring and opening a wallet all start Tor: on a network that
blocks Tor there was no way to a wallet at all. Now the first screen and
the screen that asks for a wallet's password each have a line of words
that says how the bridges stand and leads to the screen where they are
set, the same one Settings leads to; it needs no open wallet. And
settings read from a file that is there and cannot be read, or whose
bridges cannot, are marked as lost (`AppSettings.lost`): the app then
shows a screen of its own in place of every screen that starts Tor
(`SettingsLostScreen`, chosen in `app.dart`), with a warning and two
buttons, set bridges or go on without. Either writes the settings again
and the app goes on to its first screen. No file at all, which is how
every installation begins, is no loss. Only the app changed: neither the
core nor the wallet library. Six tests on the wallet that only pretends
walk both ways, and the test of the settings file on made-up input now
also holds that bridges are the ones written or are said to be lost,
never quietly none. The `offline` level passes in a cloud session with
them. With the APK built from commit `d7d74f2` the owner sent one
picture from the phone and three answers: Home said "Tor has lost its
connection" and that the device's internet connection should be checked,
with Wi-Fi shown as connected; bridges were set; it was right after the
wallet was opened; whether it went away was not looked at. Which bridges
and where they were set was not said, so the picture does not show that
the new line on the first screens was used. The reading: Tor reads the
loss of its network from a connection that fails on the way to the first
relay, with bridges the first relay is a bridge, and some of the obfs4
bridges built into Tor Browser do not answer. That a bridge was the
cause was not seen. The state is left as it is, since the two cannot be
told apart from where it is read; the words are changed. A wallet opened
through bridges (`WalletBackend.viaBridges`) now says that Tor cannot
reach its network through the bridges that are set and that one may not
be answering, on Home and on Send; without bridges the words are as
before. Not known: whether the message goes away by itself there within
a minute or two, as it should when the next connection gets through.
The owner then said that bridges had worked before the last change and
do not now, and, given an APK with the app as it was before the bridges
change (the app of commit `2e11c9d` on the same native parts), that it
does not work with that one either. So the change of the app is not the
cause. What "does not work" looked like, how long it was waited, and
which bridges were set was not said. Looked at in a cloud session: no
code on the way of Tor or of bridges has changed since the APK on which
bridges worked at step 4.6 (`ecc8c94`) but for the last dot of an onion
name being taken off, and the transport program in the new APKs is the
same file byte for byte. And one of the four obfs4 bridges the tests and
the owner use, `212.83.43.95:443`, is no longer among those the Tor
Project serves (`bridges.torproject.org/moat/circumvention/builtin`,
read on 26 November 2026); the other three and both Snowflake bridges
still are. Whether the three answer from the owner's network is not
known, and nothing in a cloud session can tell: the `bridges` level on
the owner's machine can, with the same lines and the same code. The
list in `native/veil_tor/tests/bridges.rs` is left as it is until that
run has been seen, so that it tries what the phone tried.
The owner ran the `bridges` level on commit `c54d0de` and it passed:
Tor was ready through the four obfs4 lines in 22 seconds and through
Snowflake in 28, an onion service answered through each, and the tool
synchronised a new wallet through the obfs4 bridges in 951 seconds. So
the bridges answer from the owner's network, the one that is no longer
served among them or not, and the code carries Tor through them on
Linux. On the phone, by the owner's word and a second picture: without
bridges everything works, and with obfs4 and with Snowflake alike Home
says that Tor has lost its connection, with the older APK too. Both
transports failing on the phone and neither on the desktop points at
what only the phone has: the transport program the package brings,
which the desktop does not use (it has the system's `obfs4proxy` and
`snowflake-client`), and how Android lets it be started. Why it worked
there at step 4.6 and does not now is not known. The app shows only
that the way to the first relay failed, not why; the owner was given an
APK built from a change that is not in the repository, which puts the
whole of Arti's error on Home, to read the reason from.
What it showed on the phone, with bridges set: Arti could not download
the onion service's descriptor because it could build no circuit, and
the reason for every attempt was the same, within three thousandths of
a second: `AllGuardsDown`. Of five bridges four were counted as not
running, which is what Arti calls a guard that failed a moment ago, and
the fifth was running but "not suitable", which for a circuit of
several hops means that Arti has no descriptor of that bridge. Arti
gives that the kind that `veil_tor` reads as a lost network, so the
state on Home is as written; the words about the device's connection
were what was wrong. What it did not show is why the four failed: Arti
keeps that for its log, and `veil_tor` sets up no logger. So a transport
program that does not start is neither shown nor ruled out. The phone's
clock was right to within seconds, by the time in the error and the
time of the owner's message. A second such APK, again from a change
that is not in the repository, keeps what Arti logs about transports,
channels, guards and circuits and shows it with the error.
The owner sent thirty-four lines of that log, from the thirtieth second
of a session to the fifty-first; what came before them did not arrive.
They show Arti opening a channel "via obfs4" to one bridge
(`146.57.248.225:22`) three times, ten seconds apart, the first ending
in `ChanTimeout`, and every other circuit refused for want of a guard.
So Arti does go to the transport program, and a channel through it gets
no answer within Arti's ten seconds. Whether the program was started,
is running, or is itself waiting for the bridge, the lines do not say:
its start is in the part that did not arrive, and it keeps a log of its
own only when told to. A third such APK tells it to (`-enableLogging
-logLevel DEBUG`), shows the end of that log first, and leaves out the
line Arti repeats for every circuit it cannot build.
What that one showed, in a text that arrived whole: the transport
program is started from the package, says "accepting connections" within
a fifth of a second, and Arti takes it up; so starting it on the phone
was never the fault. And Arti loads five bridges from its state, two of
them ones that had worked, and takes one as usable: the one bridge it had
never learned more about than its line, `146.57.248.225:22`, which does
not answer from the phone. The other four it does not try at all. Read
in Arti's code, and then shown on it: a bridge that is taken out of the
configuration is marked as no longer listed; put back, it loses the mark
only if Arti had never read its description, because with the
description once read Arti wants it again first, and asks only for the
descriptions of bridges without the mark (`tor-guardmgr` 0.48,
`Guard::update_from_universe`). A test added to a copy of that crate
(`native/veil_tor/upstream/tor-guardmgr-bridge-put-back.patch`) says it
in two lines: never described, put back, usable; described, put back,
not usable. That fits everything the phone did: obfs4 and then Snowflake
worked at step 4.6; obfs4 set again left only the bridge that had never
worked; Snowflake set again left none; the older APK failed on the same
files; without bridges everything worked; and the desktop's tests, which
start from empty folders, passed. Whether it is all of the cause the
phone has still to show. No report of it was found, and none was made to
the Tor Project. Veil goes around it: with bridges Tor keeps its state in
a folder of its own (`<state_dir>/bridged`, `veil_tor/src/state.rs`),
beside a note of the bridge lines it belongs to, and the folder starts
empty when the lines are others, in whatever order. Three offline tests
hold that; `bridges_that_are_set_again_carry_tor_again` walks the phone's
way, obfs4, Snowflake, obfs4 on the same files, at the `bridges` level
and has not run.
The offline level passed with it in a cloud session, and the code was
linted for Windows and for Android. With the APK built from commit
`05b8dd9`, installed over the one before, the owner reported that bridges
work again on the phone, obfs4 and Snowflake. Asked for were obfs4, then
Snowflake, then obfs4 again, each after the wallet was closed and opened;
whether obfs4 was set a second time was not said. Not run on the owner's
machine with it: anything. The three tests of the folder have not run as
Windows programs, and the test on the real network has not run at all.
The owner then ran `./accept.sh --no-app offline bridges` on commit
`fecd2a3`. The `offline` level passed as Linux programs, the three tests
of the folder among them. The `bridges` level failed in the new test and
nowhere else, and for the test's own haste: its first Tor, through
obfs4, was ready in 14 seconds and an onion service answered; its
second, through Snowflake on the same files, called itself ready after 4
seconds, having found the directory the first had kept, and refused both
connections asked of it at once with the answer that reads as a lost
network. The test asked once and gave up; it never came to obfs4 again.
The two older tests, each from empty folders, passed in the same run,
Snowflake ready in 11 seconds. The reading, not seen in a log: with a
directory already there Arti is ready before it has the description of a
bridge it has not used before, and until then it has no bridge to build
a circuit with. A wallet asks again every minute, so in the app this is
"Tor cannot reach its network through the bridges" for up to a minute
after bridges were changed; whether it also shows at every start with
the same bridges is not known. The test now asks again every five
seconds for three minutes, as a wallet would, and prints at which try it
was answered, which will say how long that state lasts. Because the run
stopped there, nothing ran as Windows programs.
On commit `215f5cb` the same run passed: the `offline` level as Linux
and as Windows programs, the three tests of the folder both ways, and
the `bridges` level. In the new test Tor was ready through obfs4 in 15
seconds and answered at the first try; through Snowflake on the same
files it was ready in 4 seconds, refused the first try's two connections
and carried the second try's, 18 seconds after it was ready; through
obfs4 again it was ready in 3 seconds and answered at the first try. So
the way that failed on the phone works on Linux with the folder of its
own, and the state in which Tor is ready and of no use lasted under 18
seconds there, once, with bridges it had not used. In the app that
would be the words about bridges on Home until the next
synchronisation, a minute later; the owner has not seen them on the
phone, where everything works by the owner's word. Nothing is changed
for it; `native/veil_tor/README.md` has it. In the same run the older
test through obfs4 took 64 seconds to be ready, with Arti saying once
that it could not make progress, where it took 19 an hour before; the
tool synchronised a new wallet through obfs4 in 543 seconds. The list
of obfs4 bridges in the two tests still has `212.83.43.95:443`, which
the Tor Project no longer serves: the level passes with it, and it is
to be replaced when the level fails for it (`VEIL_TEST_OBFS4_BRIDGES`
names other lines meanwhile). With that, the bridges before a wallet,
the words for a lost Tor with bridges, and the folder of its own have
passed the owner's run on both systems and work on the phone. Which of
the new screens the owner used there was not said. Hosted CI passed on
both systems, the three tests of the folder as Windows programs there
too, and its Windows job ran the step that has a build with a test-only
feature refused for the first time: all three were. Step 5.3 and the
three changes about bridges were merged on the owner's word. That
completes step 5 of the build plan, hardening.
Next is step 6, the release, in the six parts now listed in the build
plan: what a package owes others, the same build twice, signed
packages, the main network, F-Droid, and the release itself. The owner
was reminded of the licence of the transport program first, as asked,
and of the other things that are the owner's to decide before anything
is published: when Veil may hold real money, the name and the
identifier Android knows the app by, and who holds the key for
releases. The README's status, which still said that there was no app,
says what there is. The owner answered all four at once: the transport
program stays in the package for Android and is handed on under its own
terms; the first release is for the main network, without an audit
before it; the name Veil and the identifier `io.github.xmstarter.veil`
stay; and the key for releases never leaves the owner's machine, where
a script signs with it. They are in `docs/concept.md` under "Decided
for the release", and `docs/development.md` no longer says that hosted
CI is given signing keys. Until the part of this step that brings the
main network, the app and the tool know stagenet only, and the rules
above about real funds in tests and about `veil_cli` stand.
Step 6.1, what a package owes others, is written. Every package of the
app carries the licence texts of what it is built from, in
`app/assets/licences`, and the app shows them under Settings, About
Veil, with its version and the commit it was built from, which
`check.sh` now tells the build. Six files: Veil's own licence; the
texts of the 382 Rust packages in the core, read from each package by a
new crate, `veil_notices`, which asks `cargo metadata` what is in the
core on each of the four systems an app is built for and leaves out
what only builds or tests; those of the wallet library and its sixteen
parts, with the GNU General Public License that the Lesser one of
monero_c and polyseed builds on; those of lyrebird, Go and the sixty Go
packages the program says are in it; the three typefaces; and the
Android libraries, a list by hand. `native/notices.sh` writes them and
with `--check` says whether they are still right, which the `offline`
level asks as Linux programs. The wallet library's and the transport
program's can be checked only where their source is, which in hosted CI
is nowhere. Found while reading for it: the library for Linux is linked
with whatever the machine has of Boost, OpenSSL and the like, as shared
libraries that are in no package, so a package for Linux is not
complete in itself yet; that is for the part about the same build
twice. And on Android the library is not linked with readline, which is
under the GPL and which Monero's dependency build makes: the script
that collects the texts reads the line the library is linked with and
stops at a part it does not know. Nobody who knows licences has looked
at any of this. Tests: eight of the crate, twelve of the app, on a
package that only pretends and on the files as they are.
On commit `fd944d6` the owner's run passed at the `offline` level as
Linux and as Windows programs, the app built there with its commit told
to it. The check of the licence texts ran and held the four files it
could; the wallet library's it left alone, wrongly: it said that the
source archives were not on the machine, where the library had been
built for Windows. The script looked for them with one `ls` over both
source trees, the one for Linux and Windows and the one for Android,
and `ls` fails when one of the two is not there, which on that machine
the Android one is not. In a cloud session both are, so it had passed.
Each tree is looked at by itself now, in the script that asks and in
the one that reads the archives, where the same mistake would have
ended the script without a word. Tried in a cloud session on folders
with one tree only: with the tree for Linux and Windows the file is
checked, with the one for Android alone it is left and said to be,
since only the build for Windows downloads every archive the file is
written from. The owner then ran the check alone on commit `1c9c092`:
the wallet library's file was held against the source there and its
sixteen parts are as kept; the transport program's is left alone on
that machine, where the program was never built. And the owner reported
that on the phone everything works, the APK being the one built from
`fd944d6`; which lines of About were opened was not said. With that,
step 6.1 has passed the owner's run on both systems and on the phone.
Step 6.4, the main network, was taken up next by the owner's word,
before the two parts between. The wallet code never knew one network
from another, and the tests on a private chain have always made wallets
of the main network's kind; what knew stagenet only was the request a
session is started with, the list of public nodes, the app's folder for
its wallet, and its words. Now a start request names the network,
`mainnet` or `stagenet`, with no default, and the core has a list of
public nodes for each (`veil_core::public_nodes`, `candidates`). The app
is built for one network: `--dart-define=VEIL_NETWORK=mainnet`, which
`check.sh` passes when `VEIL_NETWORK=mainnet` stands in front of it, and
stagenet where nothing is said, tests included
(`app/lib/core/network.dart`). Each network's wallet has a folder of its
own (`<data>/mainnet`, `<data>/stagenet`). An app for the main network
says on its first screen and under About that the money is real and
that nobody outside has audited Veil; one for stagenet says what it
always said. On Android a build for trying is an app of its own,
`io.github.xmstarter.veil.stagenet` or `.preview` with a name to match,
so that it stands beside the released app and can never be installed
over it; the plain identifier is for the build the release step signs.
Both were built in a cloud session and their identifiers read back. The
app the owner has on the phone from earlier steps carries the plain
identifier and stagenet coins; it stays as it is until it is removed.
`veil_cli` stays on stagenet. The public nodes for the main network are
the six the `tor` level has used since step 1.2, from monero.fail, each
of another operator; nobody vetted them, monero.fail could not be read
from a cloud session to see whether they are still listed, and which of
them a release carries is the owner's to say (`docs/concept.md`, "Open
decisions"). The `tor` level now goes by the app's own list and has two
new things, both with an empty wallet and no funds: where a wallet made
today begins to scan, which has to lie behind the end of the chain and
not more than a hundred days behind, and every node of the list asked
where it is, failing when fewer than two answer or one is more than ten
blocks behind the furthest. Neither has run. Nothing here has ever
received or sent a payment on the main network.
On commit `312903b` the owner's run passed at the `offline`, `regtest`
and `tor` levels as Linux and as Windows programs. What the `tor` level
showed on the main network, with an empty wallet: a wallet made that day
begins to scan 21,481 blocks before the end of the chain, a little under
thirty days, and the same to within three blocks half an hour later as
a Windows program; it synchronised its last ten blocks in 14 seconds as
a Linux program and in 243 as a Windows one. Of the six public nodes,
three answered in the first run and five in the second, all at the same
height each time. plowsof, HashVault and SupportXMR answered both
times. Rucknium and the node beginning `xmrnodei` did not answer within
twenty seconds the first time and answered within six the second. The
node beginning `moneronk` was said to be unreachable both times, within
four seconds, which is how a service that is not there shows. So a
wallet can count on three of the six and often on five, and one of the
six may be gone; the list is left as it is until the owner says which
nodes a release carries. The owner said it: the node beginning
`moneronk` is taken off and the other five stay. That is in
`docs/concept.md` under "Decided for the release" now.
With the app for the main network built from that commit, `1a3b22a`,
the owner reported from the phone that everything works with real
money, and one fault, with a picture: after Send payment was pressed,
the screen that confirms a payment stood there with "Sending…" and
nothing in its four lines, no address, no amount, no fee, no total.
So a payment has been made through Veil on the main network; how much,
whether one was received as well, and how long the first
synchronisation took was not said. The fault was the core's: its state
named a payment only while it was under review, and from the
confirmation until a node had answered it said "sending" and nothing
else, while the screen draws what the state says. On stagenet and on a
private chain that moment is too short to see; the app's own test of it
handed the screen a state with everything in it, which the core never
gave. Now `SendState::Sending` carries what `Review` carried
(`SendState::confirmed`), through the C interface too
(`tests/fixtures/state-sending.json`). Offline tests hold the change of
the state, the fixture on both sides, and the screen on a wallet that
pretends the way the core now behaves; the scenario on a private chain
checks the state whenever it catches a payment on its way and fails if
it never does in four payments.
On that commit, `fb781ac`, the owner's run passed at the `offline` and
`regtest` levels as Linux and as Windows programs, the scenario with
its new check both ways; so a payment was caught on its way there at
least once on each system and named what was being paid. How often of
the four the log does not say. The owner installed the two apps built
from that commit, the one for the main network and the one for
stagenet, and reported that on the phone everything works; whether the
lines were looked at while a payment was on its way was not said in so
many words. With that, step 6.4 has passed the owner's run on both
systems and on the phone, with real money. Hosted CI passed on both
systems, and steps 6.1 and 6.4 with the fix were merged on the owner's
word. Left of step 6: the same build twice (6.2), signed packages
(6.3), F-Droid (6.5) and the release itself (6.6). The `tor` level has
not run with the list of five nodes.
Step 6.2, the same build twice, is written. `native/twice.sh` builds a
part of a package twice, in two folders that have nothing in common and
with a home for cargo each, and compares the two byte for byte. In a
cloud session, each pair built one after the other on one machine:
the core differed in 19.6 of 24.3 million bytes, for the paths of the
528 source files that can report an error, most of them under cargo's
folder in the home of whoever builds; every optimised build of the core
now names those folders the same to the compiler (`release_build` in
`check.sh`), and the core for Linux and for Android comes out the same.
The transport program came out the same, as before, and the same as the
file in the packages the owner has. The wallet library had the folder
it was built in written into it some 770 times, and the compiler Monero
brings for Android has no option against that, so its script now builds
at one path on every machine, `/srv`, in a view of the folders it makes
for itself (a mount namespace, for an ordinary user through a user
namespace; tried as an ordinary user only as far as the first
download). With the paths the same, the library for Android still
differed in eight bytes, the name of the commit monero_c makes of its
patches, which had the time of the build in it; those commits have one
date now. The library for Windows then differed in six, the time of
linking in its first bytes and a sum that follows from it; the linker is
told to leave the time out, after which the two were linked again and
were the same; they were not built from nothing a second time. For
Linux, Monero's dependency build was tried, so that the library would
need nothing of the system's: its Boost of 2018 does not compile on
Ubuntu 24.04. So the library for Linux stays linked against the
system's libraries, is the same only where those are, and a package for
Linux is still not complete in itself. It was the same in two builds on
the one machine. Found on the way: it looked for Polyseed in the folder
it had been built in, which worked only because that folder was still
there; Polyseed now lies beside it (`libpolyseed.so.2`), is looked for
there, and goes into the app's bundle with it. The app for Android: all
80 files of the package the same, given the same native parts; the
packages differ in their signatures and nowhere else, which the script
checks by where the signature lies. The app for Linux differed in its
own program, 483 bytes that came from the length of the folder's name
(the place to look for libraries was written over the build folder's
afterwards); it is linked with the final place from the start, and all
25 files are the same. Not compared: the app for Windows, which hosted
CI builds, and anything on a second machine. A work folder of the
wallet library built the old way is emptied and built again from
nothing, which `check.sh` notices by `wallet_lib/built-how.txt`: the
owner has to run `./check.sh setup` and `--windows setup` again and
allow the new DLL and `monerod.exe` in the antivirus. The first offline
run of this step failed in one test of `veil_wallet`, not for anything
of the step: a wallet closed and opened again at once was "in use". The
mark that keeps a wallet from being opened twice is a lock that goes
with the open file, and a program that is being started holds the open
files of the one that starts it for a moment; another test was starting
a program just then. A new test starts programs while a mark is taken
and let go two thousand times and failed within the first hundred
every time; the mark is now asked for again for half a second before
a wallet is refused. It had not been seen before in any run.
Tried in a cloud session with all of that: the `offline` level, the
lint for Windows, and `./check.sh --android offline`; they pass. What
that session built, on Ubuntu 24.04 with its gcc 13.3.0 and its
cross compiler for Windows of the same release, for another machine's
builds of the same commit to be held against (the first sixteen
characters of each SHA-256, which `build.sh` now prints): the wallet
library for Android `e41051ed8baf5e38`, for Windows `c066ba144ef8129b`,
for Linux `f5bbe2c8fc8bb5a3` with Polyseed `93c8dd53fb487964`, and the
transport program `ba57d65aba48a697`.
On that commit, `d39e09f`, the owner built the library again for Linux
and for Windows, which was the first time anything of Veil was built on
two machines and compared. The build's view of the folders worked there
as an ordinary user, at the first try. The run passed at the `offline`
and `regtest` levels as Linux programs and at the `regtest` level as
Windows programs, and the owner reported that on the phone everything
works with the two apps built from that commit. Two things did not come
out as hoped. The sums: Polyseed for Linux was the same file on both
machines; the library for Linux was not (`2864b08445cd6b99` there), and
of another size; the library for Windows was of the same size, 32,185,791
bytes, with another sum (`c8e9b13b1fdd76e2` there). For Linux the cause
was in the owner's log: Monero's build compiles for the processor it
runs on unless told otherwise (`-march=native`), and RandomX inside it
does too. The cloud session's library had some 3,400 instructions of
AVX-512 in it. That was a fault of a package for Linux and not only a
difference: such a library can end with an illegal instruction on a
processor older than its builder's. `build.sh` now says `ARCH=default`,
as Monero's dependency build always did for Windows and Android; built
that way in a cloud session the library has none of those instructions,
the offline level passes with it, and two builds in two folders are the
same (`db59d3709be0fed0`). The script also fixes the language and the
time zone of a build and switches off the compiler cache Monero's
dependency build brings, none of which was seen to matter: of 1,678
compilations of the two Android builds two had come from that cache.
For Windows the owner sent the file, and the two differed in sixteen
bytes of thirty-two million: the nine characters of the name of
Monero's commit, in two places, and the sum over the file. The cloud
session's machine is set to sign its commits, so the commits the build
makes of monero_c's patches were signed there and had another name. The
build now reads none of the machine's settings for git but those for
the network (`GIT_CONFIG_GLOBAL`), makes the owner's commit
(`1429b158a`), and the library for Windows built from nothing in a cloud
session is the owner's file byte for byte, `c8e9b13b1fdd76e2`. That is
the first part of a package seen to be the same on two machines, with
Polyseed.
The other thing: the `offline` level as Windows programs failed, the
test program of `veil_core` ended with status 116 after its first
eleven tests, for the second time (the first was at step 4.2, at the
`regtest` level). The same tests with the `regtest` feature passed
minutes later in the same run, all sixty-four. The diagnosis written
after the first time did not run: it was called at the `regtest` level
only, and at the `offline` level cargo goes on after a program that
fails and ends with 101 whatever happened. Now that level reads what
cargo prints for such an ending (`tests_of_the_workspace`,
`odd_endings` in `check.sh`), and the diagnosis first asks Windows what
it recorded about programs that ended badly (`what_windows_recorded`),
then runs the tests again one at a time; tried on the owner's own log
with stand-ins for cargo and for PowerShell only. The owner put that
question to Windows by hand, and its record names both endings: a
damaged heap (`0xC0000374`, whose last byte is 116), reported from
`ntdll.dll`, in the test program's first second, on 25 and on 26
November. So it was never the file share. The cause was then read in
the code and has not been seen at work: Veil's own patch for paths on
Windows (`monero/0004`) put its setting, a new locale for the program
and for Boost's reading of paths, where the library sets itself up on
its first call, behind the library's check for "already done", which
has no lock. Threads that make their first call together each pass the
check, each puts a new locale in place of the one before, and Boost
1.69 keeps that locale without a lock, so one is destroyed while other
threads read paths with it. A test program makes its first calls from
as many threads as the machine has processors; the app has one thread
that calls the library, so this is not a fault the app can meet as it
is built. The patch now makes the setting up happen once, whoever asks
(a local static, which the compiler guards), and
`the_first_calls_into_the_library_may_come_at_once` in `veil_wallet`
makes the first call from sixteen threads at once in eight processes.
On Linux it passes with either library, since nothing of this is
compiled there. What would have shown the cause: that test as a Windows
program failing on the library built before the change. The owner ran
it there, thirty processes of sixteen threads, and it passed. So the
fault in the patch is real as read and is put right, and that it was
what damaged the heap is not shown; the cause of the two endings is
open. The six endings of `veil.exe` in the same
record, "in a library that was unloaded", are of programs built on 23
November, before the library was kept loaded for good, and Windows
writes such a record again whenever it looks at it again. With all of
that the libraries built in a cloud session, for the owner's to be held
against, are: for Linux `2815398e3e401946` with Polyseed
`93c8dd53fb487964`, for Windows `3a33f0d609df79d4`, for Android
`ffdf870ca1abd41e`. The one for Android was built from nothing; the
other two were built from nothing with the patch as it was and then had
the changed patch applied and what it touches compiled again. The
`offline` level, the lint for Windows and `./check.sh --android offline`
pass with them. `built-how.txt` names the new way, so the owner has to
run both setups once more and allow the new DLL and `monerod.exe` in
the antivirus. The library for Linux is expected to differ there as
long as the distribution's packages do; nobody has compared those.
On that commit, `bc6c898`, the owner built both libraries from nothing
and ran `accept.sh`. All three files are the cloud session's, byte for
byte: the library for Windows, the library for Linux, which says that
the two machines had the same packages of Ubuntu 24.04 that day, and
Polyseed. The run passed at the `offline` and `regtest` levels as Linux
and as Windows programs, the new test of the first calls among them
both ways, and the owner reported that on the phone everything works
with the two apps built from that commit, whose library for Android is
the rebuilt one. With that, step 6.2 has passed the owner's run on both
systems and on the phone. What stays open from it: the damaged heap,
which has happened twice in some fifty runs as Windows programs and
never in the app; if it comes back, `check.sh` prints what Windows
recorded, and the next thing to ask of the owner's machine is a dump of
the program (Windows Error Reporting's `LocalDumps`), which would name
the place. The test can be run for longer and with more threads
(`VEIL_FIRST_CALL_ROUNDS`, `VEIL_FIRST_CALL_THREADS`) against a library
built before the change, which a cloud session can build or has from
the owner (`c8e9b13b1fdd76e2`); that has not been done. Not compared on
two machines: the core, the transport program, the apps. Hosted CI
passed on both systems, and step 6.2 was merged on the owner's word.
Left of step 6: signed packages (6.3), F-Droid (6.5) and the release
itself (6.6).
Step 6.3, signed packages, is written. A release for Android is built
in one place and signed in another. `VEIL_NETWORK=mainnet ./check.sh
--android release` builds the app under its plain identifier,
`io.github.xmstarter.veil`, and its plain name, from a checkout without
changes, and leaves `target/packages/veil-<version>-<commit>-unsigned.apk`:
the package with the signature of the building machine's own key taken
off again, which is the same file for everybody who builds the commit
and which no phone installs. `native/release.sh` is for the owner's
machine: `key` makes the key for releases once, a file outside the
checkout (4096-bit RSA in a PKCS12 file, `~/veil-release-key`) with a
password that is typed and kept nowhere, and prints the checksum of its
certificate; `sign` refuses anything that is not a release, has
Android's `apksigner` sign (schemes 2 and 3, as Android verifies since
version 7), and keeps the result only if the signature holds, the key
is the one `native/release-key.sha256` names, and the package is the
one it was given but for the signature. `compare` and `who` are for
anybody: whether two packages are the same but for their signatures,
and who signed one. The tools are Ubuntu's own packages (`apksigner`,
`aapt`), so nothing has to come from Google's servers, which did not
serve the owner's machine once. Seen on the way: the signing tool puts
up to 4095 empty bytes after a package's last file, which the
comparison allows for and nothing else; and the tool of 2025 in
Android's kit moves the files of a package about unless told not to
(`--alignment-preserved`), which the one of 2021 that Ubuntu packages
neither does nor knows, so the script says it where the tool knows it
and looks at the result either way. Tried in a cloud session with a
throwaway key in a scratch folder: making a key, its refusals (a short
password, two that differ, a key that is there already), signing, a
wrong password, a package that is a build for trying, a key that is not
the recorded one, and the comparisons; the release package built twice
from one commit was the same file; signed with either tool the result
verifies and is the same but for the signature. Not done, and not to be
done in a cloud session: the real key. Not seen: a signed package on a
phone. The app the owner has on the phone from the earliest steps
carries the plain identifier with a cloud session's key and stagenet
coins; Android will not take the release over it, so it has to be
removed first. Nothing signs a package for the desktop, and what a
first release has for the desktop is an open decision in the concept.
On that commit, `10eaa7a`, the owner made the key for releases on the
owner's own machine, signed the release package a cloud session had
built from the commit, and sent what `release.sh compare` said of the
two: the signed package is the unsigned one but for the signature, and
the unsigned one has the checksum the cloud session's had
(`4222057fafe56fca`). The certificate of the key is in
`native/release-key.sha256` (`82ab766ac1dfaf80`), so `sign` now refuses
any other key. The owner's run passed at the `offline` level as Linux
and as Windows programs, the test of the comparison among them both
ways, and the owner reported that on the phone everything works with
the signed package; what was asked for was that it installs, after the
earliest app under the same identifier was removed, and opens under the
name Veil. With that, step 6.3 has passed the owner's run on both
systems and on the phone. That signed package is of a commit that is no
release; it is on the owner's machine and phone and was sent to nobody.
The owner was told to copy the key to something that is not connected
to anything; whether that was done is the owner's to know. Still as it
was: the package was built by a cloud session and by nobody else, since
the owner's machine has not built for Android; before a release is
published, somebody other than that session builds the commit and the
checksums of the unsigned packages are held against each other.
Hosted CI passed on both systems, the test of the comparison running on
Windows itself there for the first time, and step 6.3 was merged on the
owner's word. Left of step 6: F-Droid (6.5) and the release itself
(6.6). The owner was told what each takes: the submission to F-Droid is
a merge request on GitLab that a cloud session cannot make, so it is
prepared here and made by the owner; and a release for Windows could be
built by hosted CI for the main network, started by hand, if the owner
takes that nobody can build it a second time to compare. Which systems
the first release is for is the owner's to say and has not been said.
The owner said it: F-Droid is left out, and the release is made for
Android first and for Windows after it. Step 6.6, the release, is begun
for Android. Written so far, and nothing published: `SECURITY.md`, which
sends a finding through GitHub's private report for the repository, a
setting the owner has to switch on; the README's status and a "Download"
section with the certificate of the key for releases; `docs/releases.md`
with an entry for 0.1.0 that says it is not released yet; and the order
a release is made in (`docs/development.md`, "Making a release"): the
commit on `main`, the owner's run on it with the `tor` level, the
package built on two machines, the signature, the comparison, a phone,
the releases page, the record. The version stays 0.1.0, number 1, which
is also what the signed package of step 6.3 on the owner's phone calls
itself, so the release installs over it. Not done: every step of that
order. Open before publishing, each the owner's: who builds the package
besides a cloud session, and whether somebody who knows licences is
asked first, which the owner had decided to do before a release.
Those documents were merged on the owner's word, and the owner switched
the private report on. No CI ran for that pull request, as none does for
a change of documents only; its code is the code of step 6.3, which CI
had passed. On the commit that made, `11e4a6d`, the owner's run passed
at the `offline`, `regtest` and `tor` levels as Linux and as Windows
programs. The `tor` level ran with the list of five nodes for the first
time: as Linux programs four of the five answered, all at one height,
and the node of plowsof did not within twenty seconds, twice; as Windows
programs all five did. A wallet made that day begins to scan 21,530
blocks before the end of the chain. The owner then decided three things.
The release goes out as it is, without anybody who knows licences having
looked, and says so. It is for Android only; Windows is put off, with no
date, and the documents say that. And the release is built a second time
on GitHub: the owner asked whether that gets the account banned, was
told that nobody can promise it does not, that the terms allow
compiling, that the Monero project builds there, and that the earlier
suspension followed a connection to Tor, and chose it. So
`.github/workflows/release.yml` is new: started by hand, or by a pull
request that changes that file, it runs `./check.sh --android setup` and
`VEIL_NETWORK=mainnet ./check.sh --android release` on GitHub's Ubuntu
and prints the checksum of the unsigned package; `check.sh` lets those
two levels run on a CI runner with `--android` and refuses them without.
Nothing of it has run yet. Because `check.sh` changes, the release is
built from the commit this makes on `main`, not from `11e4a6d`, and the
owner's run is made again on that commit. The unsigned package of
`11e4a6d` that a cloud session built and sent (`5003838b2e6f7f8f`) is
therefore not the release and is not to be signed.
The workflow ran for the first time on that pull request and passed, in
sixteen minutes, with the build's view of the folders working on
GitHub's machine. Its package was not the cloud session's. A cloud
session then built the commit that run had built, the pull request's
merge with `main`, and of the 80 files of the two packages 78 were the
same byte for byte, the core and the app's own code among them; no
machine but a cloud session had built either before. The two that
differed, and why, each read from the two files and from the run's log:
the wallet library, in fifteen thousand bytes of tables and in where
everything after them lies, because GitHub's machines have `flex` and
unbound's build, inside Monero's dependency build, makes the reader of
its settings file anew where it finds flex; and the transport program,
in sixty bytes, two marks Go makes from the command the C compiler is
called by, which is a path under the builder's home. `wallet_lib/build.sh`
now says that there is no flex (`LEX=:`), and with flex installed in a
cloud session the library built from nothing that way is the one built
without it (`ffdf870ca1abd41e`); `built-how.txt` is not changed for it,
since a library built on a machine without flex is the same library,
and the owner's has none, or the library for Windows would not have
been the same file there. `transports/build.sh` builds without the
marks (`-buildid=`), which makes the transport program another file
than the one in every package so far (`d86de391d03a7d16` where it was
`ba57d65aba48a697`), the same code without two notes; it came out the
same with the compiler under two different paths. That file has not
been on a phone. The workflow now prints the checksum of each file in
the package. With the two changes the workflow ran again on the pull
request, and its package is the cloud session's of the same commit,
byte for byte (`76b606f689196bc4`), all 80 files, the wallet library
`ffdf870ca1abd41e` on GitHub's machine too. That is the first package
of Veil built by two machines that have nothing to do with each other
and found the same. Hosted CI passed on both systems, and the pull
request was merged on the owner's word, given beforehand for the case
that every check passed. The commit that made on `main`, `981c6a6`, is
the one the release is to be built from; a cloud session's unsigned
package of it has the checksum
`623396eccc7056ffe01498a0cca968b6775c0796b7965b849f3e7059e3f4b47b`.
The owner started the workflow `release` on `main`, and hosted CI's
package of `981c6a6` has that checksum, all 80 files the same. The
owner's run passed at the `offline`, `regtest` and `tor` levels as Linux
and as Windows programs, on commit `4da75aa`, which is `981c6a6` with
these notes changed and nothing else. At the `tor` level all five public
nodes answered both times, at one height each time, and the node of
plowsof once did not within twenty seconds as the first node a new
wallet asked, as Windows programs, and the wallet went on to the next.
The owner signed that package with the key for releases and sent the
signed file. Held against the unsigned package in a cloud session, it
is the same but for the signature, and it is signed by the recorded
key; its checksum is
`3a03aee92cc67644a430ad748fa7da4cb3bb9ac46e65d55593fe6363c0e2ab55`. The
owner reported that on the phone everything works with it and that
bridges are fine; which bridges, and what else was tried, was not said.
A cloud session may not create a release on GitHub (the request is
refused for this kind of session), so the owner makes the release page
by hand from a text prepared here: tag `v0.1.0` on `981c6a6`, the signed
file attached. Still to do: that publication, and the entry in
`docs/releases.md` after it.
See the build plan in `docs/concept.md`.

## Environment notes

- Tests on a private chain, over Tor, and on stagenet run on the owner's
  machine: Windows, inside WSL2 (Ubuntu 24.04), as Linux programs and with
  `--windows` as Windows programs. The owner runs them and sends the log.
  A cloud session can build for Windows but cannot run the result: Wine
  does not start there. The machine's antivirus (ESET) flags the wallet DLL
  and `monerod.exe` as a coin miner; the owner has to allow each after it
  is rebuilt, and a run made while its question is open fails on loading.
  Once, loading the DLL failed there with Windows error 995 (operation
  aborted) in one test and worked in every other; if it comes back, the
  first thing to try is loading it from a Windows disk, not over the WSL
  file share. Twice, a test program of `veil_core` run as a Windows
  program ended with status 116 within its first second and passed when
  run again. Windows recorded a damaged heap both times (`0xC0000374`);
  the end of "Status" says what was tried for it. Its cause is open. `check.sh` asks Windows what it recorded about such an ending and
  runs the program's tests again one at a time, at the `offline` level
  too, and what that prints is the first thing to read if it comes
  back. Once, the tests of `veil_core` that open a wallet all
  stood for over a minute at the start of a freshly built test program
  and then went on; the one with a limit failed. The tests now load the
  wallet library before they time anything and say when that took long.
  Each run of `veil.exe` takes 15 to 20 seconds there before it does
  anything, from loading over the WSL file share and being scanned. Tor connects
  from that machine without bridges, so the `bridges` level shows there
  that a bridged connection works, not that it gets through a block.
- What a cloud session can reach depends on its environment's network
  settings, which the owner can change. The rule above about Tor and the
  daemon holds either way.
- The wallet library encrypts its connection to a node (TLS) when the node
  offers it, and `monerod` does. A test cannot recognise a request to a
  real node by its text; the stand-in's cuts count bytes for that reason.
- Flutter in a cloud session: the version pinned in `native/check.sh`,
  unpacked where `check.sh` looks for it (`~/veil-flutter`), with usage
  reporting switched off (`flutter --disable-analytics`,
  `dart --disable-analytics`) before anything else is run. The tools
  complain about running as root; that is harmless there.
- Android in a cloud session: `./check.sh --android ndk` and
  `--android offline` work there, and so does `wallet_lib/build.sh
  --android`. Nothing built for Android can be started there. The hosts
  an app's build needs (Google's and Maven's package servers, Gradle's)
  answered when asked. On the owner's machine the first of those
  downloads, the pinned NDK from `dl.google.com`, was answered with 404,
  once, cause unknown; the other hosts were not reached because the
  script stopped there.
- The wallet library can be built in a cloud session from source with
  `native/wallet_lib/build.sh` (about half an hour; install the packages
  listed at the top of the script first). Building it is fine; `monerod`
  from that build is not started there.
- CI results are readable from cloud sessions with `gh api`:
  `repos/XMStarter/veil-wallet/commits/<ref>/check-runs` for the outcome,
  `repos/XMStarter/veil-wallet/actions/jobs/<id>/logs` for a job's log.
- A GitHub account was suspended shortly after a CI job of this project
  connected to the Tor network. Treat the hosted-CI rule above as firm.
