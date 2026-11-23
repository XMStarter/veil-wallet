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
}

/// The light theme, the default. No gradients, no shadows, and no
/// animation except progress.
ThemeData veilTheme() {
  const shape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(VeilSizes.cornerRadius)),
  );
  const buttonSize = Size(VeilSizes.minTouchTarget, VeilSizes.buttonHeight);
  return ThemeData(
    useMaterial3: true,
    brightness: Brightness.light,
    scaffoldBackgroundColor: VeilColors.ground,
    colorScheme: const ColorScheme.light(
      primary: VeilColors.ink,
      onPrimary: VeilColors.ground,
      secondary: VeilColors.threadNavy,
      onSecondary: VeilColors.ground,
      error: VeilColors.threadCrimson,
      onError: VeilColors.ground,
      surface: VeilColors.ground,
      onSurface: VeilColors.ink,
      outline: VeilColors.fieldBorder,
      outlineVariant: VeilColors.hairline,
    ),
    fontFamily: VeilFonts.interface,
    textTheme: const TextTheme(
      headlineMedium: VeilText.title,
      displaySmall: VeilText.amount,
      bodyLarge: VeilText.body,
      bodyMedium: VeilText.body,
      bodySmall: VeilText.secondary,
      labelLarge: VeilText.button,
    ),
    splashFactory: NoSplash.splashFactory,
    dividerTheme: const DividerThemeData(
      color: VeilColors.hairline,
      thickness: 1,
      space: 1,
    ),
    appBarTheme: const AppBarTheme(
      backgroundColor: VeilColors.ground,
      foregroundColor: VeilColors.ink,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: true,
      titleTextStyle: VeilText.title,
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: VeilColors.ink,
        foregroundColor: VeilColors.ground,
        minimumSize: buttonSize,
        fixedSize: const Size.fromHeight(VeilSizes.buttonHeight),
        shape: shape,
        elevation: 0,
        textStyle: VeilText.button,
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: VeilColors.ink,
        minimumSize: buttonSize,
        fixedSize: const Size.fromHeight(VeilSizes.buttonHeight),
        shape: shape,
        side: const BorderSide(color: VeilColors.ink),
        textStyle: VeilText.button,
      ),
    ),
  );
}
