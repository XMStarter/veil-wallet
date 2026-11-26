import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'problem_words.dart';
import 'settings_screen.dart';

/// Shown in place of the first screen when the settings file is there and
/// cannot be read. The bridges may have been in it, and without them Tor
/// is connected to directly, so nothing leads on from here but saying
/// which it is to be. Either way the settings are written again, and the
/// app's first screen takes this one's place.
class SettingsLostScreen extends StatefulWidget {
  const SettingsLostScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<SettingsLostScreen> createState() => _SettingsLostScreenState();
}

class _SettingsLostScreenState extends State<SettingsLostScreen> {
  Problem? _problem;

  /// Keeps the settings as they are now, which is without bridges.
  Future<void> _goOn() async {
    final backend = widget.backend;
    try {
      await backend.saveSettings(
        backend.settings.value.copyWith(bridges: const []),
      );
    } catch (error) {
      if (mounted) setState(() => _problem = unforeseen(error));
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.settingsLostTitle,
      actions: [
        VeilButton(
          label: text.settingsLostSetBridges,
          onPressed: () =>
              open(context, BridgesScreen(backend: widget.backend)),
        ),
        VeilButton(
          label: text.settingsLostGoOn,
          primary: false,
          onPressed: _goOn,
        ),
      ],
      children: [
        Notice(text.settingsLostNotice, warning: true),
        if (problem != null) ProblemText(problemWords(text, problem)),
      ],
    );
  }
}

/// How the bridges stand, for the screens before a wallet is open, which
/// lead to where they are set: "Tor bridges: none".
String bridgesBefore(AppLocalizations text, AppSettings settings) =>
    text.bridgesBefore(
      settings.bridges.isEmpty
          ? text.settingsBridgesNone
          : text.settingsBridgesSet(settings.bridges.length),
    );
