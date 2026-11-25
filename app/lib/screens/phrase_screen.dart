import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../core/phrase.dart';
import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/secret.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'phrase_check_screen.dart';

/// The recovery phrase, to be written down. There is no way back from
/// here to a device without a wallet: the wallet exists.
class PhraseScreen extends StatelessWidget {
  const PhraseScreen({super.key, required this.backend, required this.phrase});

  final WalletBackend backend;
  final String phrase;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final words = phraseWords(phrase);
    final half = (words.length + 1) ~/ 2;
    Widget column(int from, int to) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var at = from; at < to; at++) _NumberedWord(at + 1, words[at]),
        ],
      ),
    );
    // No picture is taken of this screen: see [Device.secretShown].
    return Secret(
      device: backend.device,
      child: VeilScreen(
        title: text.phraseTitle,
        actions: [
          VeilButton(
            label: text.phraseWritten,
            onPressed: () => open(
              context,
              PhraseCheckScreen(backend: backend, phrase: phrase),
            ),
          ),
        ],
        children: [
          Notice(text.phraseNotice, warning: true),
          const SizedBox(height: 16),
          Container(
            color: VeilPalette.of(context).surface,
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                column(0, half),
                const SizedBox(width: 16),
                column(half, words.length),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _NumberedWord extends StatelessWidget {
  const _NumberedWord(this.number, this.word);

  final int number;
  final String word;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          SizedBox(
            width: 28,
            child: Text(
              '$number',
              style: VeilText.address.copyWith(
                color: VeilPalette.of(context).muted,
              ),
            ),
          ),
          Flexible(
            child: Text(word, style: VeilText.address.copyWith(fontSize: 16)),
          ),
        ],
      ),
    );
  }
}
