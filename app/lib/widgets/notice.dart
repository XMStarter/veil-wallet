import 'package:flutter/material.dart';

import '../theme/colors.dart';
import '../theme/type.dart';

/// A block of text set apart from the rest: an explanation, or a warning.
class Notice extends StatelessWidget {
  const Notice(this.text, {super.key, this.warning = false});

  final String text;

  /// Marked with a crimson line. The words carry the warning; the colour
  /// only draws the eye.
  final bool warning;

  @override
  Widget build(BuildContext context) {
    final palette = VeilPalette.of(context);
    return Container(
      decoration: BoxDecoration(
        color: palette.surface,
        border: warning
            ? Border(left: BorderSide(color: palette.threadCrimson, width: 3))
            : null,
      ),
      padding: const EdgeInsets.all(12),
      // In ink, not muted: a notice is to be read.
      child: Text(text, style: VeilText.secondary.copyWith(color: palette.ink)),
    );
  }
}

/// What went wrong, said under the field or button it concerns.
class ProblemText extends StatelessWidget {
  const ProblemText(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Text(
        text,
        style: VeilText.secondary.copyWith(
          color: VeilPalette.of(context).threadCrimson,
        ),
      ),
    );
  }
}
