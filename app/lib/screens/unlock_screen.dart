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

  /// Whether the last failure was Tor not starting through the bridges
  /// in the settings: then the wallet can be opened without them.
  bool get _bridgesFailed {
    final kind = _problem?.kind;
    return widget.backend.settings.value.bridges.isNotEmpty &&
        (kind == ProblemKind.transportMissing ||
            kind == ProblemKind.invalidBridge ||
            kind == ProblemKind.torFailed);
  }

  Future<void> _unlock({bool withoutBridges = false}) async {
    if (_password.text.isEmpty || _working) return;
    setState(() {
      _working = true;
      _problem = null;
    });
    final backend = widget.backend;
    try {
      await backend.start(
        WalletSource.open,
        password: _password.text,
        withoutBridges: withoutBridges,
      );
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
      _refused(failure.problem);
    } catch (error) {
      _refused(unforeseen(error));
    }
  }

  void _refused(Problem problem) {
    if (!mounted) return;
    setState(() {
      _working = false;
      _problem = problem;
    });
    markAll(_password);
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
        if (_bridgesFailed)
          VeilButton(
            label: text.unlockWithoutBridges,
            primary: false,
            onPressed: _password.text.isEmpty || _working
                ? null
                : () => _unlock(withoutBridges: true),
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
        if (_bridgesFailed) ...[
          const SizedBox(height: 16),
          Notice(text.unlockBridgesNotice, warning: true),
        ],
        const SizedBox(height: 16),
        Notice(text.startStagenetNotice),
      ],
    );
  }
}
