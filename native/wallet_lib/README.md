# The Monero wallet library

Veil does not implement Monero's cryptography. It loads the official wallet
code (`wallet2`) as a shared library, `libmonero_wallet2_api_c`, and calls it
through a C interface.

## Where it comes from

| Layer | Source | Pinned at | Licence |
|---|---|---|---|
| C interface and Polyseed support | [monero_c](https://github.com/MrCyjaneK/monero_c) | commit `a59deb8` (tag `v0.18.5.3-RC1`) | LGPL-3.0 |
| Wallet code | [Monero](https://github.com/monero-project/monero), `release-v0.18`, as pinned by monero_c | monero_c's `monero` submodule | BSD-3-Clause |
| Veil's patches | [`patches/`](patches) | this repository | same as the file each patch changes |

`build.sh` fetches exactly these revisions, applies monero_c's patches and
then Veil's, and builds the library. It takes about half an hour.

```bash
native/wallet_lib/build.sh ~/veil-wallet-lib --with-daemon
```

For Linux the library links against the system's own Boost, OpenSSL,
ZeroMQ, Unbound, libsodium and the rest, as shared libraries that are no
part of what is built. So it runs where those are installed, in the
versions of the machine that built it, and it is the same library only on
a machine with the same ones: a package for Linux is not complete in
itself. Monero's dependency build, which the builds for Windows and for
Android use, was tried for Linux as well, on Ubuntu 24.04: its Boost is
from 2018 and does not compile against a C library of today. One library
is built here and not the system's, Polyseed; it comes out as
`libpolyseed.so.2` beside the wallet library, which looks for it where it
itself is, and it goes wherever the wallet library goes.

### Where it is built

Whatever folder is named as the work folder, the build happens at one
path, the same on every machine: `/srv`. The compilers write the paths of
source files into what they build, some hundreds of them, and the
dependency build writes the folder it installs into; under the work
folder's own name the same sources gave another library in every folder.
The compiler Monero brings for Android, of 2018, has no option to write
another path in their place, so the path itself is made the same. The
script starts itself again in a view of the machine's folders that is its
own (a mount namespace), in which the work folder is what lies at `/srv`,
and builds there. Outside that view nothing changes: `/srv` stays what it
was for every other program, and the files are written into the work
folder by the user who runs the script. It takes a Linux that lets an
ordinary user have such a view (user namespaces, as Ubuntu under WSL2
does), or root.

Two more things are the same for every build. monero_c commits the
patches it applies to Monero, and Monero writes the first nine characters
of its commit's name into the library; a commit's name comes from its
date among other things, so the script gives those commits one fixed
date. And a work folder that was built in another way than the script
builds now is emptied and built again from nothing, for every system,
with only the dependency build's downloads kept: `built-how.txt` names
the way, and goes into the sum by which `check.sh` tells a library of
this checkout from any other.

`native/twice.sh` builds the library twice in two folders and compares
the two; `docs/development.md`, "The same build twice", says what came
out the same.

### For Windows

```bash
native/wallet_lib/build.sh ~/veil-wallet-lib --windows --with-daemon
```

builds `libmonero_wallet2_api_c.dll` and `monerod.exe` on Linux, into
`out/windows`. The cross compiler is the distribution's
(`g++-mingw-w64-x86-64`). There are no system libraries to link against for
Windows, so Monero's own dependency build compiles Boost, OpenSSL, ZeroMQ,
Unbound, libsodium and the others from source, about ten minutes, and they
are linked into the DLL. The result needs nothing but Windows itself: no
other DLL has to be shipped beside it.

monero_c's own Windows build was not used. It downloads a prebuilt compiler
from its author's file server and builds some seventy tools before it gets
to the library; the route above uses a compiler from the distribution and
sources fetched over HTTPS and checked by hash.

Because the Windows DLL contains those libraries, a build that ships it has
to carry their licence texts as well as monero_c's and Monero's.

### For Android

```bash
native/wallet_lib/build.sh ~/veil-wallet-lib --android
```

builds the library for Android on 64-bit ARM, on Linux, into
`out/android/arm64-v8a`. As for Windows, Monero's own dependency build
compiles Boost, OpenSSL, ZeroMQ, Unbound, libsodium and the others from
source, and they are linked into the one file, which then needs nothing
but what every Android has (`libc`, `libm`, `libdl`, `liblog`). The same
note about licence texts applies. There is no daemon for Android: tests
that need one run on a desktop. About ten minutes on top of the download.

The compiler is the one Monero's dependency build pins and downloads from
Google, checked by hash: the Android NDK, release 17b, of 2018. It is not
the NDK the core and the app are built with (`../check.sh` pins that one);
the library carries its own copy of the C++ runtime and talks to the rest
through a C interface, so the two do not have to match. Its age shows in
three places, each handled in `build.sh` and explained there:

- The compiler is itself a program of 2018 and asks for a system library
  under its old name, `libtinfo.so.5`. Where the system only has the
  current one, the build offers that under the old name, in a folder of
  its own and to its own programs only.
- One file monero_c adds to Monero uses a C++ feature the compiler does
  not have. A patch that only this build applies writes the same thing
  the long way (see the table below).
- monero_c's build files ask the linker for the C++ runtime by the name
  newer NDKs use; the build tells the linker which files that is.

The Android build has a source tree of its own under the work directory
(`android/`), so that its extra patch touches nothing the Linux and
Windows libraries are built from.

monero_c's own Android build was not used, for the reason given for
Windows: it is a build system of its author's that fetches and builds many
tools before it gets to the library. Moving Monero's dependency build to
the current NDK instead would mean maintaining that build; it has not been
tried.

The library as its authors build it keeps a small database of its own,
the "shared ring database", in the user's home folder
(`~/.shared-ringdb`), outside the wallet's files. No build of Veil's
makes it, on Android or anywhere else: see `monero/0005` in the table
below.

Only 64-bit ARM is built. That is what phones are; an emulator on a desktop
is a different processor and cannot load this library.

## Veil's patches

Each one is a few lines. None changes how keys, addresses, or transactions
are computed; `monero/0005` changes which decoys a transaction gets when it
is built a second time.

| Patch | What it does | Why |
|---|---|---|
| `monero/0001` and `monero_c/0001` | Expose the wallet's existing "allow mismatched daemon version" switch through the API | A private test chain activates every protocol version from its first block, which a wallet otherwise rejects. Tests need this to send and receive without real funds. The app never turns it on: the call exists only behind the `regtest` feature of `veil_wallet`. |
| `monero/0002` | A direct call to refresh downloads blocks even when the library's background refresh thread was never started | Without it a direct refresh reports success and downloads nothing. Veil drives the wallet from one thread and does not use the background thread. |
| `monero/0003` and `monero_c/0002` | A switch that keeps the library's background refresh thread idle | Building or sending a transaction switches that thread on, and it then synchronises on a timer through whichever node the wallet points at. While Veil sends through its broadcast node, that would be the broadcast node. `veil_wallet` turns the thread off for every wallet it opens. |
| `monero/0004` | On Windows, the library reads the paths it is given as UTF-8 | Without it, Boost.Filesystem reads them in the system's legacy code page. A wallet in a folder whose name is not ASCII was then not recognised as existing, so a second one could have been created over it. Programs that call the library's `Wallet::init` get the same setting from there, together with a log file, which Veil does not want. |
| `monero/0005` | Opening a wallet no longer sets up the library's "shared ring database" | The library made it outside the wallet's folder: under the user's home on Linux, and on Windows in a folder all users of the machine share. It stayed when the wallet was removed, and a damaged one ended the process when any wallet was opened. The wallet API has no call to put it elsewhere or to leave it out. [`veil_wallet`'s README](../veil_wallet/README.md) says what the database is for and what is given up without it. |
| `android/bc-ur/0001`, for the Android build only | In a library monero_c adds to Monero for payment codes shown as pictures, three places name the element type of a vector instead of leaving it to the compiler | The compiler Monero's dependency build uses for Android cannot work it out. The meaning is the same for every compiler. Veil does not call this code; the library does not build without it. |

`build.sh` notices when the patches here have changed since a work
directory was built: it puts the library's sources back, applies the current
set, and rebuilds only what changed. A library that lacks a function added
by these patches does not load, because the bindings require every function
Veil calls.

## Bindings

`generate-bindings.sh` produces `native/veil_wallet/src/sys/bindings.rs`
from the library's header, for the short list of functions Veil calls. Run
it again after changing the pinned version or that list.

## Licence obligations

Veil's own code is MIT. The wallet library is not: monero_c is LGPL-3.0.
Veil stays within the LGPL by using the library as a separate shared file
loaded at run time. Anyone distributing a build of Veil must:

- ship the library as a separate file, not linked statically into the app;
- include the LGPL-3.0 and Monero licence texts;
- make the library's source available, which is the pinned revisions above
  plus the patches in this directory.

The texts are in every package of the app, and the app shows them and
says which commit of this repository it was built from (Settings, About
Veil). `notices.sh` collects them into
`app/assets/licences/wallet-library.txt`: the licences of monero_c and
of Monero, of what Monero's source brings along (polyseed, RandomX,
LMDB, utf8proc, bc-ur, RapidJSON, Easylogging++), and of what the
dependency build makes for the libraries for Windows and for Android
(Boost, OpenSSL, libsodium, Unbound, ZeroMQ, and for Windows hidapi and
libiconv), with the text of the GNU General Public License that the
Lesser one builds on. Which parts there are is written into that script
by hand and held against the line the library is linked with, where a
build for Windows or for Android is at hand; a part it does not know
stops it. The library for Linux takes Boost, OpenSSL and the like from
the machine it is built on, as shared libraries that are in no package
of Veil's; what a package for Linux brings along is a matter of the
release step. `../notices.sh --check`, which the `offline` level runs,
says whether the file is still what the source says, on a machine that
has the source. Nobody who knows licences has looked at this; it is not
legal advice.
