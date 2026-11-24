import 'package:flutter/material.dart';

import 'colors.dart';
import 'type.dart';

/// Sizes from docs/design.md.
abstract final class VeilSizes {
  /// Buttons are 54 px high with a 2 px corner radius.
  static const buttonHeight = 54.0;
  static const cornerRadius = 2.0;

  /// Every touch target is at least this high and wide.
  static const minTouchTarget = 44.0;

  /// Desktop uses the same single column as the phone.
  static const columnWidth = 420.0;
  static const screenPadding = 24.0;

  /// The side of the QR code on the Receive screen. An address makes a
  /// code of 41 modules, 49 with its quiet zone: five pixels each.
  static const qrCode = 245.0;

  /// How thick the line that shows progress is.
  static const progressLine = 2.0;
}

/// The theme: light, the default, or dark. No gradients, no shadows, and
/// no animation except progress. Its colours are one [VeilPalette], which
/// it also carries for the screens to ask for.
ThemeData veilTheme({bool dark = false}) {
  final palette = dark ? VeilPalette.dark : VeilPalette.light;
  const shape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(VeilSizes.cornerRadius)),
  );
  const buttonSize = Size(VeilSizes.minTouchTarget, VeilSizes.buttonHeight);
  final scheme = (dark ? const ColorScheme.dark() : const ColorScheme.light())
      .copyWith(
        primary: palette.ink,
        onPrimary: palette.ground,
        secondary: palette.threadNavy,
        onSecondary: palette.ground,
        error: palette.threadCrimson,
        onError: palette.ground,
        surface: palette.ground,
        onSurface: palette.ink,
        outline: palette.fieldBorder,
        outlineVariant: palette.hairline,
      );
  TextStyle ink(TextStyle style) => style.copyWith(color: palette.ink);
  return ThemeData(
    useMaterial3: true,
    brightness: dark ? Brightness.dark : Brightness.light,
    extensions: [palette],
    scaffoldBackgroundColor: palette.ground,
    colorScheme: scheme,
    fontFamily: VeilFonts.interface,
    textTheme: TextTheme(
      headlineMedium: ink(VeilText.title),
      displaySmall: ink(VeilText.amount),
      bodyLarge: ink(VeilText.body),
      bodyMedium: ink(VeilText.body),
      bodySmall: VeilText.secondary.copyWith(color: palette.muted),
      labelLarge: VeilText.button,
    ),
    splashFactory: NoSplash.splashFactory,
    dividerTheme: DividerThemeData(
      color: palette.hairline,
      thickness: 1,
      space: 1,
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: palette.ground,
      foregroundColor: palette.ink,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: true,
      titleTextStyle: ink(VeilText.title),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: palette.ink,
        foregroundColor: palette.ground,
        minimumSize: buttonSize,
        fixedSize: const Size.fromHeight(VeilSizes.buttonHeight),
        shape: shape,
        elevation: 0,
        textStyle: VeilText.button,
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: palette.ink,
        minimumSize: buttonSize,
        fixedSize: const Size.fromHeight(VeilSizes.buttonHeight),
        shape: shape,
        side: BorderSide(color: palette.ink),
        textStyle: VeilText.button,
      ),
    ),
    // The quiet button: words only.
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        foregroundColor: palette.ink,
        minimumSize: const Size(
          VeilSizes.minTouchTarget,
          VeilSizes.minTouchTarget,
        ),
        shape: shape,
        textStyle: VeilText.button,
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: palette.ink,
      linearTrackColor: palette.hairline,
      linearMinHeight: VeilSizes.progressLine,
    ),
  );
}
