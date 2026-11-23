import 'package:flutter/painting.dart';

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
