import 'package:flutter/material.dart';

import '../theme/colors.dart';
import '../theme/theme.dart';
import '../theme/type.dart';

/// A text field with its label above it.
class VeilField extends StatelessWidget {
  const VeilField({
    super.key,
    required this.label,
    required this.controller,
    this.secret = false,
    this.monospace = false,
    this.lines = 1,
    this.numeric = false,
    this.enabled = true,
    this.autofocus = false,
    this.onChanged,
    this.onSubmitted,
  });

  final String label;
  final TextEditingController controller;

  /// A password: shown as dots, kept out of suggestions and corrections.
  final bool secret;

  /// For words and addresses that are compared letter by letter.
  final bool monospace;
  final int lines;
  final bool numeric;
  final bool enabled;
  final bool autofocus;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;

  @override
  Widget build(BuildContext context) {
    final palette = VeilPalette.of(context);
    const corners = BorderRadius.all(Radius.circular(VeilSizes.cornerRadius));
    final border = OutlineInputBorder(
      borderRadius: corners,
      borderSide: BorderSide(color: palette.fieldBorder),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: VeilText.secondaryIn(context)),
        const SizedBox(height: 6),
        TextField(
          controller: controller,
          obscureText: secret,
          // What is typed here is a secret or has to be exact: nothing is
          // suggested, corrected, or remembered by the keyboard.
          autocorrect: false,
          enableSuggestions: false,
          enableIMEPersonalizedLearning: false,
          maxLines: secret ? 1 : lines,
          minLines: secret ? 1 : lines,
          keyboardType: numeric ? TextInputType.number : null,
          enabled: enabled,
          autofocus: autofocus,
          onChanged: onChanged,
          onSubmitted: onSubmitted,
          style: monospace ? VeilText.address : VeilText.body,
          cursorColor: palette.ink,
          decoration: InputDecoration(
            filled: true,
            fillColor: palette.field,
            isDense: true,
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 12,
              vertical: 14,
            ),
            border: border,
            enabledBorder: border,
            disabledBorder: border,
            focusedBorder: OutlineInputBorder(
              borderRadius: corners,
              borderSide: BorderSide(color: palette.ink, width: 2),
            ),
          ),
        ),
      ],
    );
  }
}
