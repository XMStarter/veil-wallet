import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../core/phrase.dart';
import '../l10n/app_localizations.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/password_fields.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'home_screen.dart';
import 'navigation.dart';
import 'problem_words.dart';

/// A wallet restored from its recovery phrase, with a new password.
class RestoreScreen extends StatefulWidget {
  const RestoreScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<RestoreScreen> createState() => _RestoreScreenState();
}

class _RestoreScreenState extends State<RestoreScreen> {
  final _phrase = TextEditingController();
  final _height = TextEditingController();
  String? _password;
  bool _working = false;
  Problem? _problem;

  @override
  void dispose() {
    _phrase.dispose();
    _height.dispose();
    super.dispose();
  }

  int get _wordCount => phraseWords(_phrase.text).length;

  /// A height, when one was typed for a 25-word phrase. Null means none.
  int? get _restoreHeight =>
      _wordCount == 25 ? int.tryParse(_height.text.trim()) : null;

  bool get _heightIsRight =>
      _height.text.trim().isEmpty || _restoreHeight != null;

  bool get _ready =>
      isPhraseLength(_wordCount) &&
      _password != null &&
      (_wordCount != 25 || _heightIsRight);

  Future<void> _restore() async {
    final password = _password;
    if (password == null || !_ready || _working) return;
    setState(() {
      _working = true;
      _problem = null;
    });
    try {
      await widget.backend.start(
        WalletSource.restore,
        password: password,
        phrase: phraseWords(_phrase.text).join(' '),
        restoreHeight: _restoreHeight,
      );
      if (!mounted) return;
      openAlone(context, HomeScreen(backend: widget.backend));
    } on StartFailure catch (failure) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _problem = failure.problem;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.restoreTitle,
      actions: [
        VeilButton(
          label: _working ? text.restoreWorking : text.restoreAction,
          onPressed: _ready && !_working ? _restore : null,
        ),
        VeilButton(
          label: text.back,
          primary: false,
          onPressed: _working ? null : () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        VeilField(
          label: text.restorePhraseLabel,
          controller: _phrase,
          monospace: true,
          lines: 4,
          enabled: !_working,
          autofocus: true,
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 6),
        Text(
          '${text.restoreWordCount(_wordCount)}. ${text.restorePhraseHint}',
          style: VeilText.secondary,
        ),
        if (_wordCount == 25) ...[
          const SizedBox(height: 16),
          VeilField(
            label: text.restoreHeightLabel,
            controller: _height,
            numeric: true,
            enabled: !_working,
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          // A warning for as long as no block is given: that is the choice
          // that costs an hour.
          Notice(
            text.restoreHeightNotice,
            warning: _height.text.trim().isEmpty,
          ),
        ],
        const SizedBox(height: 16),
        PasswordFields(
          enabled: !_working,
          onChanged: (password) => setState(() => _password = password),
        ),
        if (_working) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
        ],
        if (problem != null) ProblemText(problemWords(text, problem)),
      ],
    );
  }
}
