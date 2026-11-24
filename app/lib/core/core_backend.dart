import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';

import 'package:flutter/foundation.dart';
import 'package:veil_bridge/veil_bridge.dart';

import 'backend.dart';
import 'paths.dart';

/// The wallet, run by the native core.
///
/// Starting a session, giving it commands and reading its state return at
/// once. The two calls that wait for the session's thread, the recovery
/// phrase and closing, are made on another isolate, so the interface never
/// stands still. Neither waits for a synchronisation to end: the core
/// stops one for them.
class CoreBackend implements WalletBackend {
  CoreBackend(this._paths);

  final AppPaths _paths;
  VeilCore? _core;
  VeilSession? _session;
  Timer? _watch;
  int _seen = -1;
  final _state = ValueNotifier<SessionState?>(null);

  /// How often the state's revision is looked at.
  static const _watchEvery = Duration(milliseconds: 200);

  /// How long the core waits after a synchronisation has ended before it
  /// starts the next. A block comes every two minutes.
  static const _syncEvery = Duration(seconds: 60);

  @override
  ValueListenable<SessionState?> get state => _state;

  @override
  Future<bool> walletExists() => File(_paths.walletKeysFile).exists();

  @override
  Future<void> start(
    WalletSource source, {
    required String password,
    String? phrase,
    int? restoreHeight,
  }) async {
    await close();
    for (final library in [_paths.coreLibrary, _paths.walletLibrary]) {
      if (!File(library).existsSync()) {
        throw StartFailure(Problem(libraryMissing, library));
      }
    }
    await Directory(_paths.walletDir).create(recursive: true);
    final core = _core ??= VeilCore.open(_paths.coreLibrary);
    final VeilSession session;
    try {
      session = core.start(
        SessionConfig(
          walletLibrary: _paths.walletLibrary,
          walletPath: _paths.walletFile,
          torStateDir: _paths.torStateDir,
          torCacheDir: _paths.torCacheDir,
          source: source,
          restoreHeight: restoreHeight,
          syncEvery: _syncEvery,
        ),
        password: password,
        phrase: phrase,
      );
    } on VeilException catch (refused) {
      throw StartFailure(refused.problem);
    }
    // Opening a wallet takes a moment: its password is stretched on purpose.
    var state = session.state();
    while (state.phase == Phase.opening) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
      state = session.state();
    }
    if (state.phase != Phase.ready) {
      final address = session.address;
      final library = _paths.coreLibrary;
      await Isolate.run(
        () => VeilSession.fromAddress(VeilCore.open(library), address).close(),
      );
      throw StartFailure(
        state.phaseProblem ?? const Problem(ProblemKind.other, 'not opened'),
      );
    }
    if (source == WalletSource.restore) {
      await confirmPhrase();
    }
    _session = session;
    _seen = state.revision;
    _state.value = state;
    _watch = Timer.periodic(_watchEvery, (_) => _look());
  }

  void _look() {
    final session = _session;
    if (session == null) return;
    final revision = session.revision;
    if (revision != _seen) {
      _seen = revision;
      _state.value = session.state();
    }
  }

  @override
  void newAddress() => _session?.newAddress();

  @override
  Problem? prepareSend(String to, String amount) {
    try {
      _session?.prepareSend(to, amount);
      return null;
    } on VeilException catch (refused) {
      return refused.problem;
    }
  }

  @override
  void confirmSend() => _session?.confirmSend();

  @override
  void cancelSend() => _session?.cancelSend();

  @override
  Future<String?> recoveryPhrase() async {
    final session = _session;
    if (session == null) return null;
    final address = session.address;
    final library = _paths.coreLibrary;
    return Isolate.run(
      () => VeilSession.fromAddress(
        VeilCore.open(library),
        address,
      ).recoveryPhrase(),
    );
  }

  @override
  Future<bool> phraseConfirmed() async {
    final file = File(_paths.notesFile);
    if (!await file.exists()) return false;
    try {
      final notes =
          jsonDecode(await file.readAsString()) as Map<String, dynamic>;
      return notes['phrase_confirmed'] == true;
    } on FormatException {
      return false;
    }
  }

  @override
  Future<void> confirmPhrase() =>
      File(_paths.notesFile)
          .writeAsString(jsonEncode({'phrase_confirmed': true}), flush: true);

  @override
  Future<void> close() async {
    _watch?.cancel();
    _watch = null;
    final session = _session;
    _session = null;
    _state.value = null;
    if (session == null) return;
    final address = session.address;
    final library = _paths.coreLibrary;
    await Isolate.run(
      () => VeilSession.fromAddress(VeilCore.open(library), address).close(),
    );
  }
}

/// A kind of problem only this side can have: a native part of the app is
/// not where it should be.
const libraryMissing = 'library_missing';
