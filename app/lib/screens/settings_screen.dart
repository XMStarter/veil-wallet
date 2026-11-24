import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../core/phrase.dart';
import '../core/settings.dart';
import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/theme.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/password_fields.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'problem_words.dart';
import 'start_screen.dart';
import 'status_words.dart';

/// How the wallet and the app are set up: each thing on a line of its own
/// that says how it stands, and leads to where it is changed.
class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  /// What the screen the user has just come back from achieved.
  String? _done;

  Future<void> _go(Widget screen) async {
    setState(() => _done = null);
    final done = await openFor<String>(context, screen);
    if (mounted && done != null) setState(() => _done = done);
  }

  String _languageName(AppLocalizations text, String? language) =>
      switch (language) {
        'en' => text.languageEnglish,
        'ru' => text.languageRussian,
        _ => text.languageDevice,
      };

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final backend = widget.backend;
    return VeilScreen(
      title: text.settingsTitle,
      actions: [
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        if (_done != null) ...[Notice(_done!), const SizedBox(height: 8)],
        ValueListenableBuilder<SessionState?>(
          valueListenable: backend.state,
          builder: (context, state, _) => ValueListenableBuilder<AppSettings>(
            valueListenable: backend.settings,
            builder: (context, settings, _) {
              if (state == null) return const SizedBox.shrink();
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _Line(
                    title: text.settingsTor,
                    value: torWords(text, state.tor),
                  ),
                  _Line(
                    title: text.settingsNode,
                    value: state.ownNodes.isEmpty
                        ? text.settingsNodePublic
                        : text.settingsNodeOwn,
                    onTap: () => _go(NodeScreen(backend: backend)),
                  ),
                  _Line(
                    title: text.settingsBridges,
                    value: settings.bridges.isEmpty
                        ? text.settingsBridgesNone
                        : text.settingsBridgesSet(settings.bridges.length),
                    onTap: () => _go(BridgesScreen(backend: backend)),
                  ),
                  _Line(
                    title: text.settingsPassword,
                    value: text.settingsPasswordChange,
                    onTap: () => _go(PasswordScreen(backend: backend)),
                  ),
                  _Line(
                    title: text.settingsAutoLock,
                    value: text.autoLockAfter(settings.autoLockMinutes),
                    onTap: () => _go(
                      ChoiceScreen<int>(
                        title: text.settingsAutoLock,
                        notice: text.autoLockNotice,
                        current: settings.autoLockMinutes,
                        choices: [
                          for (final minutes in AppSettings.autoLockChoices)
                            (minutes, text.autoLockAfter(minutes)),
                        ],
                        onChosen: (minutes) => backend.saveSettings(
                          backend.settings.value.copyWith(
                            autoLockMinutes: minutes,
                          ),
                        ),
                      ),
                    ),
                  ),
                  _Line(
                    title: text.settingsPhrase,
                    value: text.settingsPhraseShow,
                    onTap: () => _go(ShowPhraseScreen(backend: backend)),
                  ),
                  _Line(
                    title: text.settingsLanguage,
                    value: _languageName(text, settings.language),
                    onTap: () => _go(
                      ChoiceScreen<String?>(
                        title: text.settingsLanguage,
                        current: settings.language,
                        choices: [
                          for (final language in [null, 'en', 'ru'])
                            (language, _languageName(text, language)),
                        ],
                        onChosen: (language) => backend.saveSettings(
                          backend.settings.value.copyWith(
                            language: () => language,
                          ),
                        ),
                      ),
                    ),
                  ),
                  _Line(
                    title: text.settingsTheme,
                    value: settings.dark ? text.themeDark : text.themeLight,
                    onTap: () => _go(
                      ChoiceScreen<bool>(
                        title: text.settingsTheme,
                        current: settings.dark,
                        choices: [
                          (false, text.themeLight),
                          (true, text.themeDark),
                        ],
                        onChosen: (dark) => backend.saveSettings(
                          backend.settings.value.copyWith(dark: dark),
                        ),
                      ),
                    ),
                  ),
                  _Line(
                    title: text.settingsRemove,
                    value: text.settingsRemoveValue,
                    onTap: () => _go(RemoveWalletScreen(backend: backend)),
                  ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}

/// One setting: its name, how it stands, and a way to where it is changed.
class _Line extends StatelessWidget {
  const _Line({required this.title, required this.value, this.onTap});

  final String title;
  final String value;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(
              minHeight: VeilSizes.buttonHeight,
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(title, style: VeilText.body),
                  Text(value, style: VeilText.secondaryIn(context)),
                ],
              ),
            ),
          ),
        ),
        const Divider(),
      ],
    );
  }
}

/// One thing chosen out of a few, each on a line of its own. The choice
/// is in force as soon as it is made.
class ChoiceScreen<T> extends StatefulWidget {
  const ChoiceScreen({
    super.key,
    required this.title,
    required this.current,
    required this.choices,
    required this.onChosen,
    this.notice,
  });

  final String title;
  final T current;
  final List<(T, String)> choices;
  final Future<void> Function(T) onChosen;
  final String? notice;

  @override
  State<ChoiceScreen<T>> createState() => _ChoiceScreenState<T>();
}

class _ChoiceScreenState<T> extends State<ChoiceScreen<T>> {
  late T _current = widget.current;

  Future<void> _choose(T value) async {
    setState(() => _current = value);
    await widget.onChosen(value);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final notice = widget.notice;
    return VeilScreen(
      title: widget.title,
      actions: [
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        for (final (value, label) in widget.choices)
          // Which one is chosen is said in words, not by a mark alone.
          _Line(
            title: label,
            value: value == _current ? text.choiceSelected : '',
            onTap: () => _choose(value),
          ),
        if (notice != null) ...[const SizedBox(height: 16), Notice(notice)],
      ],
    );
  }
}

/// The user's own nodes, or none for the public ones.
class NodeScreen extends StatefulWidget {
  const NodeScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<NodeScreen> createState() => _NodeScreenState();
}

class _NodeScreenState extends State<NodeScreen> {
  late final _nodes = TextEditingController(
    text: (widget.backend.state.value?.ownNodes ?? const []).join('\n'),
  );
  Problem? _problem;

  @override
  void dispose() {
    _nodes.dispose();
    super.dispose();
  }

  void _save() {
    final refused = widget.backend.setNodes(nodeLines(_nodes.text));
    if (refused == null) {
      Navigator.of(context).pop();
    } else {
      setState(() => _problem = refused);
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.settingsNode,
      actions: [
        VeilButton(label: text.save, onPressed: _save),
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        VeilField(
          label: text.nodeLabel,
          controller: _nodes,
          monospace: true,
          lines: 4,
          onChanged: (_) => setState(() => _problem = null),
        ),
        if (problem != null) ProblemText(problemWords(text, problem)),
        const SizedBox(height: 16),
        Notice(text.nodeNotice),
      ],
    );
  }
}

/// The bridges Tor is reached through, or none.
class BridgesScreen extends StatefulWidget {
  const BridgesScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<BridgesScreen> createState() => _BridgesScreenState();
}

class _BridgesScreenState extends State<BridgesScreen> {
  late final _bridges = TextEditingController(
    text: widget.backend.settings.value.bridges.join('\n'),
  );
  Problem? _problem;

  @override
  void dispose() {
    _bridges.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final text = AppLocalizations.of(context)!;
    final backend = widget.backend;
    final lines = bridgeLines(_bridges.text);
    // Bridges that cannot be used are not kept: they would keep the
    // wallet from opening the next time.
    final refused = backend.checkBridges(lines);
    if (refused != null) {
      setState(() => _problem = refused);
      return;
    }
    final changed =
        lines.join('\n') != backend.settings.value.bridges.join('\n');
    await backend.saveSettings(backend.settings.value.copyWith(bridges: lines));
    if (!mounted) return;
    Navigator.of(context).pop(changed ? text.bridgesSaved : null);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.settingsBridges,
      actions: [
        VeilButton(label: text.save, onPressed: _save),
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        VeilField(
          label: text.bridgesLabel,
          controller: _bridges,
          monospace: true,
          lines: 6,
          onChanged: (_) => setState(() => _problem = null),
        ),
        if (problem != null) ProblemText(problemWords(text, problem)),
        const SizedBox(height: 16),
        Notice(text.bridgesNotice),
      ],
    );
  }
}

/// A new password, for whoever knows the current one.
class PasswordScreen extends StatefulWidget {
  const PasswordScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<PasswordScreen> createState() => _PasswordScreenState();
}

class _PasswordScreenState extends State<PasswordScreen> {
  final _old = TextEditingController();
  String? _next;
  bool _working = false;
  Problem? _problem;

  @override
  void dispose() {
    _old.dispose();
    super.dispose();
  }

  Future<void> _change() async {
    final next = _next;
    if (next == null || _old.text.isEmpty || _working) return;
    final text = AppLocalizations.of(context)!;
    setState(() {
      _working = true;
      _problem = null;
    });
    final refused = await widget.backend.changePassword(_old.text, next);
    if (!mounted) return;
    if (refused == null) {
      Navigator.of(context).pop(text.passwordChanged);
    } else {
      setState(() {
        _working = false;
        _problem = refused;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.settingsPassword,
      actions: [
        VeilButton(
          label: _working
              ? text.passwordChangeWorking
              : text.passwordChangeAction,
          onPressed: _next == null || _old.text.isEmpty || _working
              ? null
              : _change,
        ),
        VeilQuietButton(
          label: text.back,
          onPressed: _working ? null : () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        VeilField(
          label: text.passwordOldLabel,
          controller: _old,
          secret: true,
          enabled: !_working,
          autofocus: true,
          onChanged: (_) => setState(() => _problem = null),
        ),
        if (problem != null) ProblemText(problemWords(text, problem)),
        const SizedBox(height: 16),
        PasswordFields(
          enabled: !_working,
          onChanged: (password) => setState(() => _next = password),
        ),
      ],
    );
  }
}

/// The recovery phrase again, for whoever knows the password.
class ShowPhraseScreen extends StatefulWidget {
  const ShowPhraseScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<ShowPhraseScreen> createState() => _ShowPhraseScreenState();
}

class _ShowPhraseScreenState extends State<ShowPhraseScreen> {
  final _password = TextEditingController();
  bool _working = false;
  bool _wrong = false;
  String? _phrase;

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  Future<void> _show() async {
    if (_password.text.isEmpty || _working) return;
    setState(() {
      _working = true;
      _wrong = false;
    });
    final backend = widget.backend;
    final right = await backend.isPassword(_password.text) ?? false;
    final phrase = right ? await backend.recoveryPhrase() : null;
    if (!mounted) return;
    setState(() {
      _working = false;
      _wrong = phrase == null;
      _phrase = phrase;
      // The password has done its work and is not kept on the screen.
      if (phrase != null) _password.clear();
    });
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final phrase = _phrase;
    final back = VeilQuietButton(
      label: text.back,
      onPressed: () => Navigator.of(context).pop(),
    );
    if (phrase != null) {
      return VeilScreen(
        title: text.settingsPhrase,
        actions: [back],
        children: [
          Notice(text.phraseShowNotice, warning: true),
          const SizedBox(height: 16),
          PhraseWords(phrase),
        ],
      );
    }
    return VeilScreen(
      title: text.settingsPhrase,
      actions: [
        VeilButton(
          label: text.settingsPhraseShow,
          onPressed: _password.text.isEmpty || _working ? null : _show,
        ),
        back,
      ],
      children: [
        Text(text.phraseAskPassword, style: VeilText.body),
        const SizedBox(height: 16),
        VeilField(
          label: text.passwordLabel,
          controller: _password,
          secret: true,
          enabled: !_working,
          autofocus: true,
          onChanged: (_) => setState(() => _wrong = false),
          onSubmitted: (_) => _show(),
        ),
        if (_wrong) ProblemText(text.problemWrongPassword),
      ],
    );
  }
}

/// The words of a recovery phrase, numbered, in two columns.
class PhraseWords extends StatelessWidget {
  const PhraseWords(this.phrase, {super.key});

  final String phrase;

  @override
  Widget build(BuildContext context) {
    final words = phraseWords(phrase);
    final half = (words.length + 1) ~/ 2;
    Widget column(int from, int to) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var at = from; at < to; at++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Row(
                children: [
                  SizedBox(
                    width: 28,
                    child: Text(
                      '${at + 1}',
                      style: VeilText.address.copyWith(
                        color: VeilPalette.of(context).muted,
                      ),
                    ),
                  ),
                  Flexible(
                    child: Text(
                      words[at],
                      style: VeilText.address.copyWith(fontSize: 16),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
    return Container(
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
    );
  }
}

/// The wallet taken off this device, for whoever knows the password and
/// has read what that means.
class RemoveWalletScreen extends StatefulWidget {
  const RemoveWalletScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<RemoveWalletScreen> createState() => _RemoveWalletScreenState();
}

class _RemoveWalletScreenState extends State<RemoveWalletScreen> {
  final _password = TextEditingController();
  bool _working = false;
  bool _wrong = false;

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  Future<void> _remove() async {
    if (_password.text.isEmpty || _working) return;
    setState(() {
      _working = true;
      _wrong = false;
    });
    final backend = widget.backend;
    // Anything but a clear yes leaves the wallet where it is.
    final right = await backend.isPassword(_password.text) ?? false;
    if (!right) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _wrong = true;
      });
      return;
    }
    await backend.removeWallet();
    if (!mounted) return;
    openAlone(context, StartScreen(backend: backend));
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.settingsRemove,
      actions: [
        VeilButton(
          label: _working ? text.removeWorking : text.removeAction,
          onPressed: _password.text.isEmpty || _working ? null : _remove,
        ),
        VeilQuietButton(
          label: text.back,
          onPressed: _working ? null : () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        Notice(text.removeNotice, warning: true),
        const SizedBox(height: 16),
        Text(text.removeAskPassword, style: VeilText.body),
        const SizedBox(height: 16),
        VeilField(
          label: text.passwordLabel,
          controller: _password,
          secret: true,
          enabled: !_working,
          onChanged: (_) => setState(() => _wrong = false),
        ),
        if (_wrong) ProblemText(text.problemWrongPassword),
        if (_working) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
        ],
      ],
    );
  }
}
