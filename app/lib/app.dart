import 'dart:ui';

import 'package:flutter/material.dart';

import 'core/backend.dart';
import 'l10n/app_localizations.dart';
import 'screens/start_screen.dart';
import 'screens/unlock_screen.dart';
import 'theme/theme.dart';

/// The app. Its language follows the device and falls back to English.
class VeilApp extends StatefulWidget {
  const VeilApp({super.key, required this.backend, this.locale});

  /// The wallet behind the screens.
  final WalletBackend backend;

  /// Fixes the language, for tests. The app itself leaves it to the device.
  final Locale? locale;

  @override
  State<VeilApp> createState() => _VeilAppState();
}

class _VeilAppState extends State<VeilApp> {
  late final Future<bool> _walletExists = widget.backend.walletExists();
  late final AppLifecycleListener _lifecycle;

  @override
  void initState() {
    super.initState();
    // Closing the window saves the wallet and stops Tor first.
    _lifecycle = AppLifecycleListener(
      onExitRequested: () async {
        await widget.backend.close();
        return AppExitResponse.exit;
      },
    );
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      onGenerateTitle: (context) => AppLocalizations.of(context)!.appName,
      debugShowCheckedModeBanner: false,
      theme: veilTheme(),
      locale: widget.locale,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      // English first: it is what an unsupported language falls back to.
      supportedLocales: const [Locale('en'), Locale('ru')],
      home: FutureBuilder<bool>(
        future: _walletExists,
        builder: (context, found) {
          if (!found.hasData) return const Scaffold();
          return found.data!
              ? UnlockScreen(backend: widget.backend)
              : StartScreen(backend: widget.backend);
        },
      ),
    );
  }
}
