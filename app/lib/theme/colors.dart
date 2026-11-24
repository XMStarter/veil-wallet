import 'package:flutter/material.dart';

/// The colours of docs/design.md, by the names used there.
abstract final class VeilColors {
  /// Screen background.
  static const ground = Color(0xFFFDFDFB);

  /// Address blocks, notices.
  static const surface = Color(0xFFF4F3EF);

  /// Input fields, QR code background.
  static const field = Color(0xFFFFFFFF);

  /// Text, primary button.
  static const ink = Color(0xFF2F3033);

  /// Secondary text.
  static const muted = Color(0xFF66676B);

  /// Dividers.
  static const hairline = Color(0xFFE4E2DD);

  /// Input outlines.
  static const fieldBorder = Color(0xFF8A8B8F);

  /// Incoming amounts.
  static const threadNavy = Color(0xFF2C3E6B);

  /// Outgoing amounts, warnings.
  static const threadCrimson = Color(0xFFA3324A);

  /// Small decorative marks only, never text.
  static const gold = Color(0xFFB08D4C);
}

/// The colours a screen is drawn in: those of [VeilColors] in the light
/// theme, and their counterparts in the dark one. Screens ask for them
/// with [VeilPalette.of] and never name a colour themselves.
@immutable
class VeilPalette extends ThemeExtension<VeilPalette> {
  const VeilPalette({
    required this.ground,
    required this.surface,
    required this.field,
    required this.ink,
    required this.muted,
    required this.hairline,
    required this.fieldBorder,
    required this.threadNavy,
    required this.threadCrimson,
    required this.gold,
  });

  /// The light theme, the default: the logo's own colours.
  static const light = VeilPalette(
    ground: VeilColors.ground,
    surface: VeilColors.surface,
    field: VeilColors.field,
    ink: VeilColors.ink,
    muted: VeilColors.muted,
    hairline: VeilColors.hairline,
    fieldBorder: VeilColors.fieldBorder,
    threadNavy: VeilColors.threadNavy,
    threadCrimson: VeilColors.threadCrimson,
    gold: VeilColors.gold,
  );

  /// The dark theme: the logo turned over, milk-white on charcoal, with
  /// the two threads lightened until they read on it.
  static const dark = VeilPalette(
    ground: Color(0xFF1B1C1F),
    surface: Color(0xFF26272B),
    field: Color(0xFF121315),
    ink: Color(0xFFEDECE8),
    muted: Color(0xFFA3A4A8),
    hairline: Color(0xFF3A3B40),
    fieldBorder: Color(0xFF7C7D82),
    threadNavy: Color(0xFF8FA5DB),
    threadCrimson: Color(0xFFE08A9B),
    gold: Color(0xFFC7A566),
  );

  final Color ground;
  final Color surface;
  final Color field;
  final Color ink;
  final Color muted;
  final Color hairline;
  final Color fieldBorder;
  final Color threadNavy;
  final Color threadCrimson;
  final Color gold;

  /// The palette of the theme in force.
  static VeilPalette of(BuildContext context) =>
      Theme.of(context).extension<VeilPalette>() ?? light;

  @override
  VeilPalette copyWith() => this;

  /// One palette or the other; nothing in between is ever shown.
  @override
  VeilPalette lerp(VeilPalette? other, double t) =>
      other == null || t < 0.5 ? this : other;
}
