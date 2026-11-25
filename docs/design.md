# Veil design

The visual language comes from the logo: a profile under a veil, drawn in
charcoal on a milk-white ground, with two thin threads (navy and crimson) and
a small gold crown.

![Veil logo](../app/assets/brand/veil-logo.jpg)

## Principles

- **One main action per screen.** Home has the balance and two buttons.
  Everything else lives in the activity list and in Settings.
- **Status in words.** Tor and sync state are always visible and always
  written out ("Tor connected", "Syncing, N blocks left", "Ready"). Colour
  alone never carries meaning.
- **Addresses are made to be checked.** Monospaced, in groups of four
  characters, with the beginning and the end emphasised.
- **No decoration.** No illustrations inside the app apart from the logo, no
  gradients, no animation except progress.
- **One layout everywhere.** Desktop uses the same single column as the
  phone, in a narrow window.

## Colour

| Token | Value | Use |
|---|---|---|
| Ground | `#FDFDFB` | Screen background |
| Surface | `#F4F3EF` | Address blocks, notices |
| Field | `#FFFFFF` | Input fields, QR code background |
| Ink | `#2F3033` | Text, primary button |
| Muted | `#66676B` | Secondary text |
| Hairline | `#E4E2DD` | Dividers |
| Field border | `#8A8B8F` | Input outlines |
| Thread navy | `#2C3E6B` | Incoming amounts |
| Thread crimson | `#A3324A` | Outgoing amounts, warnings |
| Gold | `#B08D4C` | Small decorative marks only, never text |

Navy and crimson are the two threads of the veil in the logo. Incoming and
outgoing amounts also carry a plus or minus sign and a label, so they can be
told apart without colour.

The light theme is the default. A dark theme is available in Settings: the
logo turned over, milk-white on charcoal, with the two threads lightened
until they read on it.

| Token | Dark value |
|---|---|
| Ground | `#1B1C1F` |
| Surface | `#26272B` |
| Field | `#121315` |
| Ink | `#EDECE8` |
| Muted | `#A3A4A8` |
| Hairline | `#3A3B40` |
| Field border | `#7C7D82` |
| Thread navy | `#8FA5DB` |
| Thread crimson | `#E08A9B` |
| Gold | `#C7A566` |

A QR code stays dark on white in both themes: that is what scanners read.

## Type

| Role | Typeface | Use |
|---|---|---|
| Display | Cormorant Garamond | Screen titles, the balance, amounts |
| Interface | Jost | Labels, buttons, body text |
| Monospace | JetBrains Mono | Addresses, word numbers |

Screen titles and button labels are uppercase with wide letter spacing, as
in the logo's wordmark. Numbers use lining, tabular figures.

All three typefaces cover Latin and Cyrillic and are licensed under the SIL
Open Font License. They are bundled with the app; nothing is fetched at
runtime.

## Controls

- Buttons are 54 px high with a 2 px corner radius. The primary button is
  filled with Ink; the secondary button is outlined. What a screen offers
  without pressing it on anyone (going back, locking) is a button of words
  only.
- A QR code is drawn in Ink on Field, each module a whole number of
  pixels, with the quiet zone the standard asks for.
- Every touch target is at least 44 px.
- Body text contrast is at least 4.5:1 against its background.

## Screens

1. **Start.** Logo, Create wallet, Restore from recovery phrase. Restore
   opens a form for the phrase and, for 25-word phrases, the creation date.
2. **Recovery phrase.** Sixteen numbered words, a warning, and a
   confirmation button. Followed by a check of three words.
3. **Home.** Balance (tap to hide, which hides every amount on the
   screen), available and pending amounts, Tor and sync status, Receive and
   Send, recent activity, and a quiet way to lock the wallet. A
   synchronisation under way shows a line that fills.
4. **Receive.** QR code, the number of the address, the address itself,
   Copy, and a way to ask for a new address. On a phone also Share, which
   hands the address to another app; a desktop has nothing to share into.
5. **Send.** A form: the address, with Paste, and the amount, or
   everything. Then a review: the address in groups of four, the amount,
   the fee, the total, a warning if coins from several addresses are
   combined, and the one button that sends. Then the outcome: sent, or not
   known, which is a warning and never a failure. While a transaction is
   being built or handed over, a line moves and words say what is waited
   for.
6. **Settings.** One line for each thing, saying how it stands and leading
   to where it is changed: Tor status, node, bridges, password, auto-lock,
   show recovery phrase, language, theme, and removing the wallet. A choice
   among a few is a list in which the one in force says so in a word.
   Removing the wallet and showing the phrase ask for the password. On a
   phone that can, there is a line for opening the wallet with a
   fingerprint or a face; its screen says what that means before the one
   button that switches it on, for the password, and the screen that
   opens the wallet then has a second button under the first. The
   question for the fingerprint itself is Android's and looks like
   Android's.

## Language

The interface ships in English and Russian. By default it follows the device
language and falls back to English.

## To do

- A vector version of the logo. The repository currently has a raster image,
  whose white background shows as a faint box on the start screen.
- A simplified mark for the app icon. The full illustration does not read at
  icon size. Until there is one, the icon on Android is the profile under
  its veil cut out of the logo, without the wordmark.
  The mark next to the notice that a wallet is open, which Android draws
  in one colour from a shape, is a ring with one half filled until then.
