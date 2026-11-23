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

This is the development and test build. It links against the system's
libraries. Release builds will use monero_c's reproducible toolchain.

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

## Veil's patches

Each one is a few lines. None changes how keys, addresses, or transactions
are computed.

| Patch | What it does | Why |
|---|---|---|
| `monero/0001` and `monero_c/0001` | Expose the wallet's existing "allow mismatched daemon version" switch through the API | A private test chain activates every protocol version from its first block, which a wallet otherwise rejects. Tests need this to send and receive without real funds. The app never turns it on: the call exists only behind the `regtest` feature of `veil_wallet`. |
| `monero/0002` | A direct call to refresh downloads blocks even when the library's background refresh thread was never started | Without it a direct refresh reports success and downloads nothing. Veil drives the wallet from one thread and does not use the background thread. |
| `monero/0003` and `monero_c/0002` | A switch that keeps the library's background refresh thread idle | Building or sending a transaction switches that thread on, and it then synchronises on a timer through whichever node the wallet points at. While Veil sends through its broadcast node, that would be the broadcast node. `veil_wallet` turns the thread off for every wallet it opens. |

| `monero/0004` | On Windows, the library reads the paths it is given as UTF-8 | Without it, Boost.Filesystem reads them in the system's legacy code page. A wallet in a folder whose name is not ASCII was then not recognised as existing, so a second one could have been created over it. Programs that call the library's `Wallet::init` get the same setting from there, together with a log file, which Veil does not want. |

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
