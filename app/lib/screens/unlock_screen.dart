import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'home_screen.dart';
import 'navigation.dart';
import 'phrase_screen.dart';
import 'problem_words.dart';

/// The first screen on a device that has a wallet: its password.
class UnlockScreen extends StatefulWidget {
  const UnlockScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<UnlockScreen> createState() => _UnlockScreenState();
}

class _UnlockScreenState extends State<UnlockScreen> {
  final _password = TextEditingController();
  bool _working = false;
  Problem? _problem;

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  Future<void> _unlock() async {
    if (_password.text.isEmpty || _working) return;
    setState(() {
      _working = true;
      _problem = null;
    });
    final backend = widget.backend;
    try {
      await backend.start(WalletSource.open, password: _password.text);
      // A wallet whose phrase was never checked shows it again first.
      final confirmed = await backend.phraseConfirmed();
      final phrase = confirmed ? null : await backend.recoveryPhrase();
      if (!mounted) return;
      openAlone(
        context,
        phrase == null
            ? HomeScreen(backend: backend)
            : PhraseScreen(backend: backend, phrase: phrase),
      );
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
      title: text.unlockTitle,
      actions: [
        VeilButton(
          label: _working ? text.unlockWorking : text.unlockAction,
          onPressed: _password.text.isEmpty || _working ? null : _unlock,
        ),
      ],
      children: [
        VeilField(
          label: text.passwordLabel,
          controller: _password,
          secret: true,
          enabled: !_working,
          autofocus: true,
          onChanged: (_) => setState(() {}),
          onSubmitted: (_) => _unlock(),
        ),
        if (_working) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
        ],
        if (problem != null) ProblemText(problemWords(text, problem)),
        const SizedBox(height: 16),
        Notice(text.startStagenetNotice),
      ],
    );
  }
}
