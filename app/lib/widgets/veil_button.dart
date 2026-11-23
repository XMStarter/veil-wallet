import 'package:flutter/material.dart';

/// A button of the full column width. The label is shown in uppercase, as
/// in the logo's wordmark. There is one primary button per screen.
class VeilButton extends StatelessWidget {
  const VeilButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.primary = true,
  });

  final String label;
  final VoidCallback? onPressed;

  /// Filled with ink; otherwise outlined.
  final bool primary;

  @override
  Widget build(BuildContext context) {
    final text = Text(label.toUpperCase(), textAlign: TextAlign.center);
    return SizedBox(
      width: double.infinity,
      child: primary
          ? FilledButton(onPressed: onPressed, child: text)
          : OutlinedButton(onPressed: onPressed, child: text),
    );
  }
}
