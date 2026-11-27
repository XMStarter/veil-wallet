<p align="center">
  <img src="app/assets/brand/veil-logo.jpg" width="440" alt="Veil — privacy crypto wallet">
</p>

# Veil

Veil is a hot Monero wallet for desktop (Windows, Linux) and Android. It does
four things: create a wallet, restore a wallet, receive, and send. All network
traffic goes through a built-in Tor client, and every incoming payment gets a
fresh address.

> **Status: first releases, not audited.** Veil for Android is released
> for the Monero main network, where coins are real money; see
> [Download](#download). The apps for Windows and Linux are built from
> source for now. Veil creates, restores, synchronises, receives and
> sends through Tor, with bridges where Tor is blocked. Tests cover a
> killed process, a damaged wallet file, and connections lost in the
> middle of a synchronisation or a send. Nobody outside the project has
> audited any of it. Do not keep more in it than you can afford to lose,
> and read the [disclaimer](DISCLAIMER.md).

## Download

Releases are on the
[releases page](https://github.com/XMStarter/veil-wallet/releases) of this
repository and nowhere else. A release for Android is one file,
`veil-<version>.apk`, for phones with a 64-bit ARM processor and Android 7
or later. It has no Google services in it and asks for none.

Before the first installation, see who signed the file. Every release is
signed with one key, whose certificate has this SHA-256 checksum:

```
82ab766ac1dfaf805df1aa5af37d4c869a64260a92474dee5076cc25e0b08fcd
```

Android's own tool prints it for a file, `apksigner verify --print-certs
veil-<version>.apk`, and so does `native/release.sh who` from this
repository. After the first installation the phone checks it by itself:
Android takes an update only from the key the installed app was signed
with.

Whoever would rather trust nobody builds the commit a release names and
compares; [docs/development.md](docs/development.md), "Signed packages",
says how, and a release's page gives the checksum that build has to
have.

Write the recovery phrase down before the first coin arrives. It is the
only way back to the money when the phone is lost, and nobody can
restore a wallet without it.

To report something that can cost a user money or privacy, see
[SECURITY.md](SECURITY.md).

## Priorities

1. **Reliability.** Transactions and cryptography come from the official
   Monero wallet library. Veil does not implement its own.
2. **Simplicity.** Few screens, few settings. Safe behaviour is the default
   and cannot be switched off.
3. **Anonymity.** Tor only, with no fallback to a direct connection. No
   analytics, no crash reports, no third-party services.

## What it does

| Operation | How |
|---|---|
| Create a wallet | A 16-word recovery phrase, shown once and then verified |
| Restore a wallet | From a 16-word or a classic 25-word Monero phrase |
| Receive | A new subaddress for every incoming payment, as text and QR |
| Send | Address, amount, a review screen, then broadcast over a separate Tor circuit |

## Stack

| Layer | Choice |
|---|---|
| Wallet logic and cryptography | The official Monero `wallet2` library, through the `monero_c` FFI wrapper |
| Tor | Arti, embedded in the app process, with bridge support |
| User interface | Flutter, one codebase for Android, Windows, and Linux |

## Documentation

- [Concept](docs/concept.md): what Veil is and how it protects the user
- [Design](docs/design.md): visual language and screens
- [Development](docs/development.md): how the code is written, tested, and released
- [For an audit](docs/audit.md): what is in scope, what Veil defends against and what it does not, and where each promise is kept
- [Releases](docs/releases.md): what has been released, from which commit, with which checksums
- [Security](SECURITY.md): where to send a finding
- [Disclaimer](DISCLAIMER.md)

## License

Veil's own code is under the [MIT License](LICENSE). A package of the
app also holds other people's work under their own licences: the Monero
wallet library it loads at run time, part of which is under the GNU
Lesser General Public License ([native/wallet_lib](native/wallet_lib/README.md)),
and in the app for Android the transport program for Tor bridges, under
the GNU General Public License ([native/transports](native/transports/README.md)).
Their texts, and those of everything else a package is built from, are
in [app/assets/licences](app/assets/licences) and are shown in the app.

## Notices

Veil is an independent project and is not affiliated with the Monero Project
or The Tor Project.

This product is produced independently from the Tor® anonymity software and
carries no guarantee from The Tor Project about quality, suitability or
anything else.
