import 'dart:math';

import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../core/phrase.dart';
import '../l10n/app_localizations.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'home_screen.dart';
import 'navigation.dart';

/// Asks for three words of the phrase, to see that it was written down.
class PhraseCheckScreen extends StatefulWidget {
  const PhraseCheckScreen({
    super.key,
    required this.backend,
    required this.phrase,
    this.random,
  });

  final WalletBackend backend;
  final String phrase;

  /// Picks the words to ask for; tests fix it.
  final Random? random;

  @override
  State<PhraseCheckScreen> createState() => _PhraseCheckScreenState();
}

class _PhraseCheckScreenState extends State<PhraseCheckScreen> {
  late final List<String> _words = phraseWords(widget.phrase);
  late final List<int> _asked = wordsToCheck(
    _words.length,
    random: widget.random,
  );
  late final List<TextEditingController> _typed = [
    for (final _ in _asked) TextEditingController(),
  ];

  @override
  void initState() {
    super.initState();
    // No picture is taken of this screen: see [Device.secretShown].
    widget.backend.device.secretShown();
  }

  @override
  void dispose() {
    widget.backend.device.secretGone();
    for (final field in _typed) {
      field.dispose();
    }
    super.dispose();
  }

  bool _right(int at) => sameWord(_typed[at].text, _words[_asked[at] - 1]);

  bool get _allRight =>
      [for (var at = 0; at < _asked.length; at++) _right(at)].every((r) => r);

  /// A field with something in it that is not the word, once the user has
  /// typed as many letters as the word has.
  bool get _mismatch => [
    for (var at = 0; at < _asked.length; at++)
      _typed[at].text.trim().length >= _words[_asked[at] - 1].length &&
          !_right(at),
  ].any((wrong) => wrong);

  Future<void> _done() async {
    await widget.backend.confirmPhrase();
    if (!mounted) return;
    openAlone(context, HomeScreen(backend: widget.backend));
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.checkTitle,
      actions: [
        VeilButton(
          label: text.continueLabel,
          onPressed: _allRight ? _done : null,
        ),
        VeilButton(
          label: text.back,
          primary: false,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        Text(text.checkIntro, style: VeilText.body),
        for (var at = 0; at < _asked.length; at++) ...[
          const SizedBox(height: 16),
          VeilField(
            label: text.checkWordLabel(_asked[at]),
            controller: _typed[at],
            monospace: true,
            autofocus: at == 0,
            onChanged: (_) => setState(() {}),
          ),
        ],
        if (_mismatch) ProblemText(text.checkMismatch),
      ],
    );
  }
}
