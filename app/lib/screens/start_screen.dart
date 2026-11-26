import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../theme/theme.dart';
import '../widgets/network_notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_column.dart';
import 'create_screen.dart';
import 'navigation.dart';
import 'restore_screen.dart';
import 'settings_lost_screen.dart';
import 'settings_screen.dart';

/// The first screen on a device without a wallet: the logo, and the two
/// ways to a wallet. Both start Tor, so the bridges Tor is to be reached
/// through can be set from here: on a network that blocks Tor there would
/// otherwise be no way to a wallet at all.
class StartScreen extends StatelessWidget {
  const StartScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final theme = Theme.of(context).textTheme;
    return Scaffold(
      body: VeilColumn(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Spacer(),
            Image.asset(
              'assets/brand/veil-logo.jpg',
              semanticLabel: text.appName,
            ),
            const SizedBox(height: 16),
            Text(
              text.startTagline,
              style: theme.bodyLarge,
              textAlign: TextAlign.center,
            ),
            const Spacer(),
            NetworkNotice(backend.network, whereAWalletBegins: true),
            const SizedBox(height: VeilSizes.screenPadding),
            VeilButton(
              label: text.startCreate,
              onPressed: () => open(context, CreateScreen(backend: backend)),
            ),
            const SizedBox(height: 12),
            VeilButton(
              label: text.startRestore,
              primary: false,
              onPressed: () => open(context, RestoreScreen(backend: backend)),
            ),
            ValueListenableBuilder(
              valueListenable: backend.settings,
              builder: (context, settings, _) => VeilQuietButton(
                label: bridgesBefore(text, settings),
                onPressed: () => open(context, BridgesScreen(backend: backend)),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
