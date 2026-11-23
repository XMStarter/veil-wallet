import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_column.dart';

/// Stands where a screen of a later build step will be.
class PlaceholderScreen extends StatelessWidget {
  const PlaceholderScreen({super.key, required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final theme = Theme.of(context).textTheme;
    return Scaffold(
      body: VeilColumn(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(height: 16),
            Text(
              title.toUpperCase(),
              style: theme.headlineMedium,
              textAlign: TextAlign.center,
            ),
            const Spacer(),
            Text(
              text.notBuiltYet,
              style: theme.bodyLarge,
              textAlign: TextAlign.center,
            ),
            const Spacer(),
            VeilButton(
              label: text.back,
              primary: false,
              onPressed: () => Navigator.of(context).pop(),
            ),
          ],
        ),
      ),
    );
  }
}
