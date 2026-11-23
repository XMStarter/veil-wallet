import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/theme.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_column.dart';
import 'placeholder_screen.dart';

/// The first screen: the logo, and the two ways to a wallet.
class StartScreen extends StatelessWidget {
  const StartScreen({super.key});

  void _open(BuildContext context, String title) {
    Navigator.of(context).push(
      PageRouteBuilder<void>(
        // No animation except progress.
        pageBuilder: (_, _, _) => PlaceholderScreen(title: title),
        transitionDuration: Duration.zero,
        reverseTransitionDuration: Duration.zero,
      ),
    );
  }

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
            Container(
              color: VeilColors.surface,
              padding: const EdgeInsets.all(12),
              child: Text(text.startStagenetNotice, style: theme.bodySmall),
            ),
            const SizedBox(height: VeilSizes.screenPadding),
            VeilButton(
              label: text.startCreate,
              onPressed: () => _open(context, text.createTitle),
            ),
            const SizedBox(height: 12),
            VeilButton(
              label: text.startRestore,
              primary: false,
              onPressed: () => _open(context, text.restoreTitle),
            ),
          ],
        ),
      ),
    );
  }
}
