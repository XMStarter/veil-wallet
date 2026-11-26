import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../widgets/network_notice.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'home_screen.dart';
import 'navigation.dart';
import 'phrase_screen.dart';
import 'problem_words.dart';
import 'settings_lost_screen.dart';
import 'settings_screen.dart';

/// The first screen on a device that has a wallet: its password.
class UnlockScreen extends StatefulWidget {
  const UnlockScreen({super.key, required this.backend, this.notSaved});

  final WalletBackend backend;

  /// Why the wallet could not be saved when it was closed just now, if it
  /// could not: said here, where closing it leads.
  final Problem? notSaved;

  @override
  State<UnlockScreen> createState() => _UnlockScreenState();
}

class _UnlockScreenState extends State<UnlockScreen> {
  final _password = TextEditingController();
  bool _working = false;
  Problem? _problem;

  /// Whether the phone opens the wallet for a fingerprint or a face.
  bool _byBiometrics = false;

  /// Said instead of a problem's own words: what became of opening with
  /// a fingerprint or a face.
  String? _said;

  /// The password the phone gave back, for opening without bridges after
  /// it was refused with them. Kept no longer than this screen.
  String? _recalled;

  @override
  void initState() {
    super.initState();
    _lookForBiometrics();
  }

  Future<void> _lookForBiometrics() async {
    Biometrics found;
    try {
      found = await widget.backend.device.biometrics();
    } catch (_) {
      // A phone that cannot say opens the wallet with the password.
      found = Biometrics.absent;
    }
    if (mounted) setState(() => _byBiometrics = found == Biometrics.on);
  }

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  /// Whether the last failure was Tor not starting through the bridges
  /// in the settings: then the wallet can be opened without them.
  bool get _bridgesFailed {
    final kind = _problem?.kind;
    return widget.backend.settings.value.bridges.isNotEmpty &&
        (kind == ProblemKind.transportMissing ||
            kind == ProblemKind.invalidBridge ||
            kind == ProblemKind.torFailed);
  }

  /// The password to open with: the one typed, or else the one the phone
  /// gave back a moment ago.
  String get _given =>
      _password.text.isEmpty ? _recalled ?? '' : _password.text;

  Future<void> _unlock({bool withoutBridges = false}) =>
      _open(_given, withoutBridges: withoutBridges);

  /// Asks the phone for the password, for a fingerprint or a face, and
  /// opens the wallet with it.
  Future<void> _unlockByBiometrics() async {
    if (_working) return;
    final text = AppLocalizations.of(context)!;
    final device = widget.backend.device;
    setState(() {
      _problem = null;
      _said = null;
    });
    BiometricResult result;
    try {
      result = await device.recallPassword(
        title: text.biometricsPromptOpen,
        cancel: text.biometricsPromptCancel,
      );
    } catch (error) {
      result = BiometricResult(BiometricOutcome.failed, detail: '$error');
    }
    if (!mounted) return;
    final password = result.password;
    switch (result.outcome) {
      case BiometricOutcome.done when password != null && password.isNotEmpty:
        _recalled = password;
        await _open(password, recalled: true);
      case BiometricOutcome.cancelled:
        break;
      case BiometricOutcome.changed:
        setState(() {
          _byBiometrics = false;
          _said = text.biometricsChanged;
        });
      case BiometricOutcome.done || BiometricOutcome.failed:
        setState(
          () => _said =
              '${text.biometricsFailed} '
              '${text.problemDetail(result.detail ?? '')}',
        );
    }
  }

  /// Opens the wallet with [password]. [recalled] says that the phone
  /// gave it back and nobody typed it.
  Future<void> _open(
    String password, {
    bool withoutBridges = false,
    bool recalled = false,
  }) async {
    if (password.isEmpty || _working) return;
    setState(() {
      _working = true;
      _problem = null;
      _said = null;
    });
    final backend = widget.backend;
    try {
      await backend.start(
        WalletSource.open,
        password: password,
        withoutBridges: withoutBridges,
      );
      // A wallet whose phrase was never checked shows it again first.
      final confirmed = await backend.phraseConfirmed();
      final phrase = confirmed ? null : await backend.recoveryPhrase();
      if (!confirmed && phrase == null) {
        // The phrase was never checked and cannot be shown now. The
        // wallet is not opened as if it had been: it is closed again, and
        // the next attempt shows the phrase.
        await backend.close();
        _refused(
          const Problem(
            ProblemKind.other,
            'the wallet gave no recovery phrase',
          ),
        );
        return;
      }
      if (!mounted) return;
      openAlone(
        context,
        phrase == null
            ? HomeScreen(backend: backend)
            : PhraseScreen(backend: backend, phrase: phrase),
      );
    } on StartFailure catch (failure) {
      if (recalled && failure.problem.kind == ProblemKind.wrongPassword) {
        // What the phone kept is not the wallet's password: it was
        // changed where the phone did not hear of it. Kept, it would be
        // refused every time.
        await _forgetRecalled();
        return;
      }
      _refused(failure.problem);
    } catch (error) {
      _refused(unforeseen(error));
    }
  }

  Future<void> _forgetRecalled() async {
    _recalled = null;
    try {
      await widget.backend.device.forgetPassword();
    } catch (_) {
      // Then it is refused again the next time, and forgotten then.
    }
    if (!mounted) return;
    final text = AppLocalizations.of(context)!;
    setState(() {
      _working = false;
      _byBiometrics = false;
      _said = text.biometricsStale;
    });
  }

  void _refused(Problem problem) {
    if (!mounted) return;
    setState(() {
      _working = false;
      _problem = problem;
    });
    markAll(_password);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final problem = _problem;
    return VeilScreen(
      title: text.unlockTitle,
      actions: [
        VeilButton(
          label: _working ? text.unlockWorking : text.unlockAction,
          onPressed: _password.text.isEmpty || _working ? null : _unlock,
        ),
        if (_byBiometrics)
          VeilButton(
            label: text.unlockBiometrics,
            primary: false,
            onPressed: _working ? null : _unlockByBiometrics,
          ),
        if (_bridgesFailed)
          VeilButton(
            label: text.unlockWithoutBridges,
            primary: false,
            onPressed: _given.isEmpty || _working
                ? null
                : () => _unlock(withoutBridges: true),
          ),
        // Opening the wallet starts Tor, so this is where the bridges
        // have to be within reach, not behind the open wallet only.
        ValueListenableBuilder(
          valueListenable: widget.backend.settings,
          builder: (context, settings, _) => VeilQuietButton(
            label: bridgesBefore(text, settings),
            onPressed: _working
                ? null
                : () => open(context, BridgesScreen(backend: widget.backend)),
          ),
        ),
      ],
      children: [
        if (widget.notSaved != null) ...[
          Notice(
            '${text.unlockNotSaved} '
            '${text.problemDetail(widget.notSaved!.detail)}',
            warning: true,
          ),
          const SizedBox(height: 16),
        ],
        VeilField(
          label: text.passwordLabel,
          controller: _password,
          secret: true,
          enabled: !_working,
          autofocus: true,
          onChanged: (_) => setState(() {}),
          onSubmitted: (_) => _unlock(),
        ),
        if (_working) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
        ],
        if (problem != null) ProblemText(problemWords(text, problem)),
        if (_said != null) ProblemText(_said!),
        if (_bridgesFailed) ...[
          const SizedBox(height: 16),
          Notice(text.unlockBridgesNotice, warning: true),
        ],
        if (widget.backend.network == WalletNetwork.stagenet) ...[
          const SizedBox(height: 16),
          NetworkNotice(widget.backend.network),
        ],
      ],
    );
  }
}
