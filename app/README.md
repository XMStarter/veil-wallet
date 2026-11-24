# The Veil app

The interface, written with Flutter, for Linux and Windows first. It follows
[docs/design.md](../docs/design.md): one column as wide as a phone, the
colours and typefaces named there, English and Russian.

So far it has the way to an open wallet: create one, write down and check
its recovery phrase, restore one from a phrase, and open the one on the
device with its password. An open wallet lands on Home: the balance, what
Tor and the synchronisation are doing, in words, and the latest payments.
From there Receive shows the address to be paid at, as a QR code and as
text, and the wallet can be locked: it is saved and closed, Tor is
stopped, and the app asks for the password. While the wallet is open it
synchronises by itself, a minute after the last time. Send takes an
address and an amount, shows the transaction with its fee and total for a
last look, and then how it went: sent, or, if the connection was lost,
not known, which the wallet then clears up by itself. Settings has a line
for each thing that can be set: the user's own node, Tor bridges, the
password, the auto-lock, the recovery phrase shown again, language, the
light or dark theme, and the wallet taken off the device. The network is
stagenet.

The app reaches the wallet only through the core
([`native/veil_core`](../native/veil_core)), which it loads as a shared
library and talks to through [`dart/veil_bridge`](../dart/veil_bridge).
Opening a wallet starts the core's Tor client; the app itself makes no
connection of any kind.

## Layout

| Path | What it holds |
|---|---|
| `lib/theme` | Colours, typefaces and sizes from the design document, and the theme built from them |
| `lib/widgets` | What screens are made of: the buttons, the single column, a notice, a field, an address in groups of four, a QR code |
| `lib/screens` | The screens |
| `lib/core` | What the screens need from the wallet (`backend.dart`), the real thing over the core (`core_backend.dart`), where the app keeps its files (`paths.dart`), how it is set to behave (`settings.dart`), and the handling of a typed phrase and of pasted text |
| `lib/l10n` | The texts, in `app_en.arb` and `app_ru.arb`. `flutter pub get` writes the Dart files next to them |
| `fonts` | Cormorant Garamond, Jost and JetBrains Mono, each one variable font file with its licence (SIL Open Font License). Nothing is fetched at runtime |
| `assets/brand` | The logo |
| `linux`, `windows` | The window each system gives the app: 420 by 800, titled "Veil" |

## Rules

- A colour, a typeface or a size comes from `lib/theme`, never from a
  literal in a screen. A screen asks for colours with
  `VeilPalette.of(context)`, which is the light or the dark set; text
  styles carry no colour of their own. Only the QR code names its two
  colours, because it is dark on white whatever the theme.
- A text the user reads comes from the `.arb` files, in both languages.
- A screen shows amounts and addresses as the core hands them over. It
  does not calculate with amounts.
- The app adds no package that connects to anything. The one package
  beyond Flutter's own, `qr`, turns text into the squares of a QR code and
  does nothing else.
- A screen talks to `WalletBackend`, never to the core directly, so that
  it can be tested on a wallet that only pretends.
- A password or a phrase is typed into a field that suggests, corrects
  and remembers nothing.

## Checks

`native/check.sh offline` runs these when Flutter is installed, and builds
the app for the system it runs on:

```bash
flutter analyze
```

```bash
flutter test
```

The tests draw the screens without a display, with the app's own fonts,
and walk the whole way to an open wallet, and through Home, Receive, Send
and Settings, on a wallet that only pretends: no core, no files, no Tor. They also write
each screen as a picture to `build/screenshots/`, to look at without
running the app.

`test/core_backend_test.dart` uses the real core, as far as it can go
without opening a wallet. It needs `cargo build -p veil_core` in `native/`
and is skipped without it.

## Where the app keeps things

| What | Linux | Windows |
|---|---|---|
| The wallet | `~/.local/share/veil/stagenet/` | `%APPDATA%\Veil\stagenet\` |
| Language, theme, auto-lock, bridges | `~/.local/share/veil/settings.json` | `%APPDATA%\Veil\settings.json` |
| Tor's files | `~/.local/share/veil/tor/` | `%APPDATA%\Veil\tor\` |

The core and the Monero wallet library are looked for next to the program
(`lib/` on Linux), or where `VEIL_CORE_LIB` and `VEIL_WALLET_LIB` say.

## Running it

After `native/check.sh offline`, on Linux or in WSL, on a machine where the
wallet library has been built (the script then puts both libraries next to
the app):

```bash
build/linux/x64/release/bundle/veil
```

This is a real wallet on stagenet: it connects to Tor and to public
stagenet nodes.

## Not done yet

- Restoring from a 25-word phrase asks for a block number to scan from,
  not for a date as the concept says. The core cannot turn a date into a
  block yet.
- Screenshots are not blocked on the phrase screen. That is for Android.
- Receive has no Share. That is for Android too: a desktop has nothing to
  share into.
- Home lists the latest twenty payments and has no way to the rest.
- Send has no way to scan a code. That is for Android.
- New bridges take effect the next time the wallet is opened, and the
  programs they need have to be installed on the machine; none comes with
  the app yet. If bridges in the settings stop working, the screen that
  asks for the password offers to open the wallet without them.
- The auto-lock counts touches, clicks and keys. It does not lock a wallet
  in the middle of a synchronisation.
- Removing the wallet leaves Tor's own files and the app's settings.
- Closing the window or locking the wallet during a synchronisation waits
  for the blocks that are on their way, normally seconds. If the node has
  gone silent it waits until the wallet library gives up on it, which
  takes minutes.
