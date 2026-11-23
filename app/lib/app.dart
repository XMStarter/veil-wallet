import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'screens/start_screen.dart';
import 'theme/theme.dart';

/// The app. Its language follows the device and falls back to English.
class VeilApp extends StatelessWidget {
  const VeilApp({super.key, this.locale});

  /// Fixes the language, for tests. The app itself leaves it to the device.
  final Locale? locale;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      onGenerateTitle: (context) => AppLocalizations.of(context)!.appName,
      debugShowCheckedModeBanner: false,
      theme: veilTheme(),
      locale: locale,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      // English first: it is what an unsupported language falls back to.
      supportedLocales: const [Locale('en'), Locale('ru')],
      home: const StartScreen(),
    );
  }
}
