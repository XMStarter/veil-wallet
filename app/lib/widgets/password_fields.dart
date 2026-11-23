import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import 'notice.dart';
import 'veil_field.dart';

/// The shortest password the app accepts.
const minPasswordLength = 8;

/// A new password, typed twice. [onChanged] gets the password once both
/// fields hold the same acceptable one, and null otherwise.
class PasswordFields extends StatefulWidget {
  const PasswordFields({
    super.key,
    required this.onChanged,
    this.enabled = true,
  });

  final ValueChanged<String?> onChanged;
  final bool enabled;

  @override
  State<PasswordFields> createState() => _PasswordFieldsState();
}

class _PasswordFieldsState extends State<PasswordFields> {
  final _first = TextEditingController();
  final _second = TextEditingController();

  @override
  void dispose() {
    _first.dispose();
    _second.dispose();
    super.dispose();
  }

  bool get _tooShort =>
      _first.text.isNotEmpty && _first.text.length < minPasswordLength;
  bool get _differ => _second.text.isNotEmpty && _second.text != _first.text;

  void _changed(String _) {
    setState(() {});
    final good =
        _first.text.length >= minPasswordLength && _first.text == _second.text;
    widget.onChanged(good ? _first.text : null);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        VeilField(
          label: text.passwordLabel,
          controller: _first,
          secret: true,
          enabled: widget.enabled,
          onChanged: _changed,
        ),
        if (_tooShort) ProblemText(text.passwordTooShort),
        const SizedBox(height: 16),
        VeilField(
          label: text.passwordRepeatLabel,
          controller: _second,
          secret: true,
          enabled: widget.enabled,
          onChanged: _changed,
        ),
        if (_differ) ProblemText(text.passwordsDiffer),
        const SizedBox(height: 16),
        Notice(text.passwordNotice),
      ],
    );
  }
}
