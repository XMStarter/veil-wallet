# The Veil app

The interface, written with Flutter, for Linux and Windows first. It follows
[docs/design.md](../docs/design.md): one column as wide as a phone, the
colours and typefaces named there, English and Russian.

So far it is a shell: the start screen in the app's visual language, with
its two buttons leading to screens that are not built yet. It does not open
a wallet and makes no connection of any kind. The screens come with build
steps 3.4 to 3.7 of [docs/concept.md](../docs/concept.md).

## Layout

| Path | What it holds |
|---|---|
| `lib/theme` | Colours, typefaces and sizes from the design document, and the theme built from them |
| `lib/widgets` | The button and the single column every screen uses |
| `lib/screens` | The screens |
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

## Checks

`native/check.sh offline` runs these when Flutter is installed, and builds
the app for the system it runs on:

```bash
flutter analyze
```

```bash
flutter test
```

The tests draw the screens without a display, with the app's own fonts.
They also write each screen as a picture to `build/screenshots/`, to look
at without running the app.

## Running it

After `native/check.sh offline`, on Linux or in WSL:

```bash
build/linux/x64/release/bundle/veil
```
