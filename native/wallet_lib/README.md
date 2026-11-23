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

## Veil's patches

Each one is a few lines. None changes how keys, addresses, or transactions
are computed.

| Patch | What it does | Why |
|---|---|---|
| `monero/0001` and `monero_c/0001` | Expose the wallet's existing "allow mismatched daemon version" switch through the API | A private test chain activates every protocol version from its first block, which a wallet otherwise rejects. Tests need this to send and receive without real funds. The app never turns it on: the call exists only behind the `regtest` feature of `veil_wallet`. |
| `monero/0002` | A direct call to refresh downloads blocks even when the library's background refresh thread was never started | Without it a direct refresh reports success and downloads nothing. Veil drives the wallet from one thread and does not use the background thread. |

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
