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
    listeners and a C interface. It reads the loss of its network from
    the connections it is asked for (`State::Lost`); its README says how,
    and what that cannot tell.
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
    only; do not add another network to it.
  - `wallet_lib`: pinned build script for the wallet library, Veil's
    patches to it, and the bindings generator. After changing a patch, run
    `build.sh` again and regenerate the bindings. `--android` builds it
    for Android on 64-bit ARM from a source tree of its own, with a patch
    only that build has (`patches/android`); the README says why.
  - `veil_test_guard`: called first by every test that starts Tor or a
    daemon; fails the test on a CI runner. A new test of that kind calls it
    too. It also starts the daemon for tests, one at a time, and when the
    daemon does not come up it says how it ended and shows the end of its
    log.
  - `check.sh`: the checks by level (`offline`, `regtest`, `tor`,
    `stagenet`, `bridges`) with a log; `setup` prepares a machine;
    `daemons` is a diagnosis of the test daemon, not a check. With
    `--windows` in front, the levels are built for Windows on Linux and run
    as Windows programs through WSL. With `--android` in front there are
    four: `ndk` installs the pinned Android NDK, `setup` adds the Android
    SDK and the wallet library for Android, `offline` lints and builds
    for Android and runs nothing, and `app` builds the app as an APK with
    the core and the wallet library inside.
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
  and the app kept running while a wallet is open, which is
  `WalletService`).
  So
  far the way to an open wallet (create, recovery phrase, restore, unlock),
  Home, Receive, Send, and Settings.
  Screens talk to `WalletBackend` (`app/lib/core`), never to the core
  directly; tests use a backend that only pretends, and no test opens a
  real wallet, because that starts Tor. Read
  `app/README.md` and `docs/design.md` before changing it. Colours,
  typefaces and sizes come from `app/lib/theme`; texts from the `.arb`
  files, always in both languages.
- `docs/`: concept, design, development rules.
- `.github/workflows`: hosted CI, builds and offline tests only. Pin
  actions to commit hashes. It installs Flutter with `check.sh flutter`,
  not with an action. The Windows job keeps the app it built as the
  artifact `veil-windows`; the owner has to allow a new action in the
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
  file share. Once, a test program run as a Windows program ended with
  status 116 a second or so after starting and passed when run again;
  `check.sh` now runs such a program's tests again one at a time to name
  the test, and what that run prints is the first thing to read if it
  comes back.
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
