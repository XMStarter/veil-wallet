// The visual language of docs/design.md, as numbers.

import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/theme/colors.dart';
import 'package:veil/theme/theme.dart';
import 'package:veil/theme/type.dart';

/// Contrast ratio of two colours, as WCAG defines it.
double contrast(Color a, Color b) {
  double luminance(Color color) {
    double channel(double value) => value <= 0.03928
        ? value / 12.92
        : math.pow((value + 0.055) / 1.055, 2.4).toDouble();
    return 0.2126 * channel(color.r) +
        0.7152 * channel(color.g) +
        0.0722 * channel(color.b);
  }

  final (light, dark) = (
    math.max(luminance(a), luminance(b)),
    math.min(luminance(a), luminance(b)),
  );
  return (light + 0.05) / (dark + 0.05);
}

void main() {
  test('the colours are the ones in the design document', () {
    expect(VeilColors.ground, const Color(0xFFFDFDFB));
    expect(VeilColors.surface, const Color(0xFFF4F3EF));
    expect(VeilColors.field, const Color(0xFFFFFFFF));
    expect(VeilColors.ink, const Color(0xFF2F3033));
    expect(VeilColors.muted, const Color(0xFF66676B));
    expect(VeilColors.hairline, const Color(0xFFE4E2DD));
    expect(VeilColors.fieldBorder, const Color(0xFF8A8B8F));
    expect(VeilColors.threadNavy, const Color(0xFF2C3E6B));
    expect(VeilColors.threadCrimson, const Color(0xFFA3324A));
    expect(VeilColors.gold, const Color(0xFFB08D4C));
  });

  test(
    'text has a contrast of at least 4.5 to 1 against what it stands on',
    () {
      for (final (text, background) in [
        (VeilColors.ink, VeilColors.ground),
        (VeilColors.ink, VeilColors.surface),
        (VeilColors.ink, VeilColors.field),
        (VeilColors.muted, VeilColors.ground),
        (VeilColors.muted, VeilColors.surface),
        // The primary button's label.
        (VeilColors.ground, VeilColors.ink),
        (VeilColors.threadNavy, VeilColors.ground),
        (VeilColors.threadCrimson, VeilColors.ground),
      ]) {
        expect(
          contrast(text, background),
          greaterThanOrEqualTo(4.5),
          reason: '$text on $background',
        );
      }
    },
  );

  test('each role has its typeface', () {
    expect(VeilText.title.fontFamily, 'Cormorant Garamond');
    expect(VeilText.amount.fontFamily, 'Cormorant Garamond');
    expect(VeilText.body.fontFamily, 'Jost');
    expect(VeilText.button.fontFamily, 'Jost');
    expect(VeilText.address.fontFamily, 'JetBrains Mono');
    // Titles and button labels are widely spaced, as in the wordmark.
    expect(VeilText.title.letterSpacing, greaterThan(2));
    expect(VeilText.button.letterSpacing, greaterThan(1));
  });

  test('the theme is light, flat, and built from those colours', () {
    final theme = veilTheme();
    expect(theme.brightness, Brightness.light);
    expect(theme.scaffoldBackgroundColor, VeilColors.ground);
    expect(theme.colorScheme.primary, VeilColors.ink);
    expect(theme.colorScheme.error, VeilColors.threadCrimson);
    expect(theme.appBarTheme.elevation, 0);
    expect(theme.splashFactory, NoSplash.splashFactory);
    // Gold is for small decorative marks, never for text or controls.
    final scheme = theme.colorScheme;
    for (final used in [
      scheme.primary,
      scheme.secondary,
      scheme.error,
      scheme.onSurface,
    ]) {
      expect(used, isNot(VeilColors.gold));
    }
  });
}
