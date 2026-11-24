import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../widgets/notice.dart';
import '../widgets/password_fields.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'phrase_screen.dart';
import 'problem_words.dart';

/// A new wallet: its password, then the wallet itself.
class CreateScreen extends StatefulWidget {
  const CreateScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<CreateScreen> createState() => _CreateScreenState();
}

class _CreateScreenState extends State<CreateScreen> {
  String? _password;
  bool _working = false;
  Problem? _problem;

  Future<void> _create() async {
    final password = _password;
    if (password == null || _working) return;
    setState(() {
      _working = true;
      _problem = null;
    });
    try {
      await widget.backend.start(WalletSource.create, password: password);
      final phrase = await widget.backend.recoveryPhrase();
      if (!mounted) return;
      if (phrase == null) {
        setState(() {
          _working = false;
          _problem = const Problem(
            ProblemKind.other,
            'the wallet gave no recovery phrase',
          );
        });
        return;
      }
      openAlone(context, PhraseScreen(backend: widget.backend, phrase: phrase));
    } on StartFailure catch (failure) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _problem = failure.problem;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _problem = unforeseen(error);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.createTitle,
      actions: [
        VeilButton(
          label: _working ? text.createWorking : text.createAction,
          onPressed: _password == null || _working ? null : _create,
        ),
        VeilButton(
          label: text.back,
          primary: false,
          onPressed: _working ? null : () => Navigator.of(context).pop(),
        ),
      ],
      children: [
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
