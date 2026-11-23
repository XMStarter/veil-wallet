import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/veil_screen.dart';

/// Where an open wallet lands. For now it says that the wallet is open
/// and what Tor is doing; the balance and the rest come with the next
/// build step.
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.homeTitle,
      children: [
        ValueListenableBuilder<SessionState?>(
          valueListenable: backend.state,
          builder: (context, state, _) {
            if (state == null) return const SizedBox.shrink();
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // Status in words; colour alone never carries meaning.
                Text(
                  switch (state.tor.state) {
                    TorState.ready => text.torReady,
                    TorState.bootstrapping => text.torConnecting(
                      state.tor.percent,
                    ),
                    TorState.failed => text.torFailed,
                  },
                  style: VeilText.body,
                  textAlign: TextAlign.center,
                ),
                if (state.rebuiltFromKeys) ...[
                  const SizedBox(height: 16),
                  Notice(text.homeRebuilt, warning: true),
                ],
              ],
            );
          },
        ),
        const SizedBox(height: 24),
        Text(
          text.homeNotBuilt,
          style: VeilText.body,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 24),
        Notice(text.startStagenetNotice),
      ],
    );
  }
}
