# The Veil app

The interface, written with Flutter, for Linux and Windows first. It follows
[docs/design.md](../docs/design.md): one column as wide as a phone, the
colours and typefaces named there, English and Russian.

So far it has the way to an open wallet: create one, write down and check
its recovery phrase, restore one from a phrase, and open the one on the
device with its password. An open wallet lands on a screen that says so
and shows what Tor is doing; the balance, receiving, sending and the
settings come with build steps 3.5 to 3.7 of
[docs/concept.md](../docs/concept.md). The network is stagenet.

The app reaches the wallet only through the core
([`native/veil_core`](../native/veil_core)), which it loads as a shared
library and talks to through [`dart/veil_bridge`](../dart/veil_bridge).
Opening a wallet starts the core's Tor client; the app itself makes no
connection of any kind.

## Layout

| Path | What it holds |
|---|---|
| `lib/theme` | Colours, typefaces and sizes from the design document, and the theme built from them |
| `lib/widgets` | The button and the single column every screen uses |
| `lib/screens` | The screens |
| `lib/core` | What the screens need from the wallet (`backend.dart`), the real thing over the core (`core_backend.dart`), where the app keeps its files (`paths.dart`), and the handling of a typed phrase |
| `lib/l10n` | The texts, in `app_en.arb` and `app_ru.arb`. `flutter pub get` writes the Dart files next to them |
| `fonts` | Cormorant Garamond, Jost and JetBrains Mono, each one variable font file with its licence (SIL Open Font License). Nothing is fetched at runtime |
| `assets/brand` | The logo |
| `linux`, `windows` | The window each system gives the app: 420 by 800, titled "Veil" |

## Rules

- A colour, a typeface or a size comes from `lib/theme`, never from a
  literal in a screen.
- A text the user reads comes from the `.arb` files, in both languages.
- A screen shows amounts and addresses as the core hands them over. It
  does not calculate with amounts.
- The app adds no package that connects to anything.
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
and walk the whole way to an open wallet on a wallet that only pretends:
no core, no files, no Tor. They also write each screen as a picture to
`build/screenshots/`, to look at without running the app.

`test/core_backend_test.dart` uses the real core, as far as it can go
without opening a wallet. It needs `cargo build -p veil_core` in `native/`
and is skipped without it.

## Where the app keeps things

| What | Linux | Windows |
|---|---|---|
| The wallet | `~/.local/share/veil/stagenet/` | `%APPDATA%\Veil\stagenet\` |
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
- Closing the window waits for a synchronisation that is under way.
