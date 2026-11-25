import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'core/backend.dart';
import 'l10n/app_localizations.dart';
import 'screens/navigation.dart';
import 'screens/problem_words.dart';
import 'screens/start_screen.dart';
import 'screens/unlock_screen.dart';
import 'theme/theme.dart';

/// The app. Its language follows the device and falls back to English,
/// unless the settings name one.
class VeilApp extends StatefulWidget {
  const VeilApp({super.key, required this.backend, this.locale});

  /// The wallet behind the screens.
  final WalletBackend backend;

  /// Fixes the language, for tests. The app itself leaves it to the
  /// settings and the device.
  final Locale? locale;

  @override
  State<VeilApp> createState() => _VeilAppState();
}

class _VeilAppState extends State<VeilApp> {
  late final Future<bool> _walletExists = _begin();
  late final AppLifecycleListener _lifecycle;
  final _navigator = GlobalKey<NavigatorState>();

  /// Runs out when the wallet has not been used for as long as the
  /// settings say.
  Timer? _idle;

  /// When the wallet was last used, by the device's clock. A phone that
  /// sleeps stops the timer above and not the clock.
  DateTime? _usedAt;

  /// How often an auto-lock that was put off looks again.
  static const _lookAgain = Duration(seconds: 5);

  Future<bool> _begin() async {
    await widget.backend.loadSettings();
    return widget.backend.walletExists();
  }

  @override
  void initState() {
    super.initState();
    // Closing the window saves the wallet and stops Tor first.
    _lifecycle = AppLifecycleListener(
      // A copied address whose minute ran out while the app was in the
      // background is taken out of the clipboard now.
      onResume: () {
        widget.backend.device.resumed();
        _lockIfOverdue();
      },
      onExitRequested: () async {
        try {
          await widget.backend.close();
        } catch (_) {
          // Nothing more can be done for the wallet, and a window that
          // will not close helps nobody.
        }
        return AppExitResponse.exit;
      },
    );
    widget.backend.settings.addListener(_settingsChanged);
    widget.backend.state.addListener(_walletChanged);
    HardwareKeyboard.instance.addHandler(_key);
  }

  @override
  void dispose() {
    _idle?.cancel();
    HardwareKeyboard.instance.removeHandler(_key);
    widget.backend.state.removeListener(_walletChanged);
    widget.backend.settings.removeListener(_settingsChanged);
    _lifecycle.dispose();
    super.dispose();
  }

  void _settingsChanged() {
    setState(() {});
    _used();
    // The language may be another now.
    _present();
  }

  bool _key(KeyEvent event) {
    _used();
    // Only watched, never taken: the key goes on to whoever it is for.
    return false;
  }

  /// The wallet was opened or closed, or its state changed. Opening it
  /// starts the time to the auto-lock and closing it ends it; a change of
  /// state is not use, or a wallet that synchronises every minute by
  /// itself would never lock.
  void _walletChanged() {
    final open = widget.backend.state.value != null;
    if (open && _idle == null) {
      _used();
    } else if (!open) {
      _idle?.cancel();
      _idle = null;
      _usedAt = null;
    }
    _present();
  }

  /// Tells the device whether a wallet is open and what it is doing, for
  /// a phone to keep the app running meanwhile and to say so in its list
  /// of what is running. Nothing of the wallet's contents is in it.
  void _present() {
    final state = widget.backend.state.value;
    final device = widget.backend.device;
    if (state == null) {
      device.walletOpen(null);
      return;
    }
    final text = lookupAppLocalizations(_language());
    final syncing = state.sync.phase == SyncPhase.running;
    device.walletOpen(
      Presence(
        kind: text.backgroundKind,
        title: text.appName,
        text: syncing ? text.backgroundSyncing : text.backgroundOpen,
        // What a phone that goes to sleep would cut off in the middle.
        busy:
            syncing ||
            state.tor.state == TorState.bootstrapping ||
            state.send.phase == SendPhase.preparing ||
            state.send.phase == SendPhase.sending,
      ),
    );
  }

  /// The language the app speaks: the one fixed for a test, the one
  /// chosen in the settings, or the device's; English if it is none the
  /// app has.
  Locale _language() {
    final chosen = widget.backend.settings.value.language;
    final wanted =
        widget.locale ??
        (chosen == null ? PlatformDispatcher.instance.locale : Locale(chosen));
    final known = AppLocalizations.supportedLocales.any(
      (locale) => locale.languageCode == wanted.languageCode,
    );
    return known ? Locale(wanted.languageCode) : const Locale('en');
  }

  /// The wallet was used just now: the time to the auto-lock starts
  /// again. Without an open wallet there is nothing to lock and no timer.
  void _used() {
    _idle?.cancel();
    _idle = null;
    if (widget.backend.state.value == null) return;
    _usedAt = widget.backend.device.now();
    _idle = Timer(
      Duration(minutes: widget.backend.settings.value.autoLockMinutes),
      _lockIfIdle,
    );
  }

  /// The app is in front again. If the time to the auto-lock has passed
  /// by the clock, the wallet locks now: the timer alone would give a
  /// phone that slept through it that much longer.
  void _lockIfOverdue() {
    final usedAt = _usedAt;
    if (usedAt == null || widget.backend.state.value == null) return;
    final limit = Duration(
      minutes: widget.backend.settings.value.autoLockMinutes,
    );
    if (widget.backend.device.now().difference(usedAt) < limit) return;
    // Cancelled and not forgotten: a wallet with a timer is one whose
    // time is not started again by a change of its state.
    _idle?.cancel();
    unawaited(_lockIfIdle());
  }

  /// Locks the wallet, unless it is in the middle of a synchronisation:
  /// a long first one would never finish if the wallet were closed under
  /// it every few minutes. Then it looks again shortly.
  Future<void> _lockIfIdle() async {
    final state = widget.backend.state.value;
    if (state == null) return;
    if (state.sync.phase == SyncPhase.running) {
      _idle = Timer(_lookAgain, _lockIfIdle);
      return;
    }
    Problem? notSaved;
    try {
      notSaved = await widget.backend.close();
    } catch (error) {
      notSaved = unforeseen(error);
    }
    final navigator = _navigator.currentState;
    if (navigator == null || !mounted) return;
    navigator.pushAndRemoveUntil(
      at<void>(UnlockScreen(backend: widget.backend, notSaved: notSaved)),
      (_) => false,
    );
  }

  @override
  Widget build(BuildContext context) {
    final language = widget.backend.settings.value.language;
    return Listener(
      // A touch or a click anywhere counts as use.
      onPointerDown: (_) => _used(),
      behavior: HitTestBehavior.translucent,
      child: MaterialApp(
        navigatorKey: _navigator,
        onGenerateTitle: (context) => AppLocalizations.of(context)!.appName,
        debugShowCheckedModeBanner: false,
        theme: veilTheme(dark: widget.backend.settings.value.dark),
        locale: widget.locale ?? (language == null ? null : Locale(language)),
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
      ),
    );
  }
}
