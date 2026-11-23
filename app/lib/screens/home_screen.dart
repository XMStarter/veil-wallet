import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'unlock_screen.dart';

/// Where an open wallet lands. For now it says that the wallet is open
/// and what Tor is doing, and lets the user lock it; the balance and the
/// rest come with the next build step.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  bool _locking = false;

  /// Saves and closes the wallet, stops Tor, and asks for the password
  /// again. The app stays open.
  Future<void> _lock() async {
    if (_locking) return;
    setState(() => _locking = true);
    await widget.backend.close();
    if (!mounted) return;
    openAlone(context, UnlockScreen(backend: widget.backend));
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.homeTitle,
      actions: [
        VeilButton(
          label: _locking ? text.lockWorking : text.lockAction,
          primary: false,
          onPressed: _locking ? null : _lock,
        ),
      ],
      children: [
        ValueListenableBuilder<SessionState?>(
          valueListenable: widget.backend.state,
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
        if (_locking) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
        ],
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
