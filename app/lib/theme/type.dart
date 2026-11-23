import 'package:flutter/painting.dart';

import 'colors.dart';

/// The typefaces of docs/design.md. All three are bundled with the app.
abstract final class VeilFonts {
  /// Screen titles, the balance, amounts.
  static const display = 'Cormorant Garamond';

  /// Labels, buttons, body text.
  static const interface = 'Jost';

  /// Addresses, word numbers.
  static const monospace = 'JetBrains Mono';
}

/// Lining, tabular figures, for everything that shows a number.
const _figures = [FontFeature.liningFigures(), FontFeature.tabularFigures()];

abstract final class VeilText {
  /// A screen title: uppercase by the caller, widely spaced.
  static const title = TextStyle(
    fontFamily: VeilFonts.display,
    fontSize: 28,
    fontVariations: [FontVariation('wght', 500)],
    letterSpacing: 5,
    height: 1.2,
    color: VeilColors.ink,
    fontFeatures: _figures,
  );

  /// The balance and other amounts.
  static const amount = TextStyle(
    fontFamily: VeilFonts.display,
    fontSize: 40,
    fontVariations: [FontVariation('wght', 500)],
    height: 1.1,
    color: VeilColors.ink,
    fontFeatures: _figures,
  );

  static const body = TextStyle(
    fontFamily: VeilFonts.interface,
    fontSize: 16,
    fontVariations: [FontVariation('wght', 400)],
    height: 1.4,
    color: VeilColors.ink,
    fontFeatures: _figures,
  );

  static const secondary = TextStyle(
    fontFamily: VeilFonts.interface,
    fontSize: 14,
    fontVariations: [FontVariation('wght', 400)],
    height: 1.4,
    color: VeilColors.muted,
    fontFeatures: _figures,
  );

  /// A button label: uppercase by the button, widely spaced.
  static const button = TextStyle(
    fontFamily: VeilFonts.interface,
    fontSize: 14,
    fontVariations: [FontVariation('wght', 500)],
    letterSpacing: 2.5,
    height: 1.2,
    fontFeatures: _figures,
  );

  static const address = TextStyle(
    fontFamily: VeilFonts.monospace,
    fontSize: 14,
    fontVariations: [FontVariation('wght', 400)],
    height: 1.5,
    color: VeilColors.ink,
  );

  /// The beginning and the end of an address, which are what people
  /// compare.
  static const addressEdge = TextStyle(
    fontFamily: VeilFonts.monospace,
    fontSize: 14,
    fontVariations: [FontVariation('wght', 700)],
    height: 1.5,
    color: VeilColors.ink,
  );

  /// The middle of an address.
  static const addressMiddle = TextStyle(
    fontFamily: VeilFonts.monospace,
    fontSize: 14,
    fontVariations: [FontVariation('wght', 400)],
    height: 1.5,
    color: VeilColors.muted,
  );

  /// The amount of one entry in the activity list.
  static const listAmount = TextStyle(
    fontFamily: VeilFonts.display,
    fontSize: 22,
    fontVariations: [FontVariation('wght', 600)],
    height: 1.2,
    color: VeilColors.ink,
    fontFeatures: _figures,
  );

  /// The heading of a part of a screen: uppercase by the caller.
  static const heading = TextStyle(
    fontFamily: VeilFonts.interface,
    fontSize: 12,
    fontVariations: [FontVariation('wght', 500)],
    letterSpacing: 2,
    height: 1.4,
    color: VeilColors.muted,
  );
}
