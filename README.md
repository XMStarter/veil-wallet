<p align="center">
  <img src="app/assets/brand/veil-logo.jpg" width="440" alt="Veil — privacy crypto wallet">
</p>

# Veil

Veil is a hot Monero wallet for desktop (Windows, Linux) and Android. It does
four things: create a wallet, restore a wallet, receive, and send. All network
traffic goes through a built-in Tor client, and every incoming payment gets a
fresh address.

> **Status: pre-release, test network only.** There is an app for Linux,
> Windows and Android and a command-line tool. They create, restore,
> synchronise, receive and send through Tor, with bridges where Tor is
> blocked, on the Monero stagenet, whose coins are worth nothing. They do
> not know the main network. Tests cover a killed process, a damaged
> wallet file, and connections lost in the middle of a synchronisation or
> a send. Nothing is released, nothing here has been audited, and nothing
> here is for real funds. Read the [disclaimer](DISCLAIMER.md).

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
