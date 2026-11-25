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
/// once. The calls that wait for the session's thread (the recovery
/// phrase, the password checked or changed, and closing) are made on
/// another isolate, so the interface never stands still. None waits for a
/// synchronisation to end: the core stops one for them.
class CoreBackend implements WalletBackend {
  CoreBackend(this._paths);

  final AppPaths _paths;
  VeilCore? _core;
  VeilSession? _session;
  Timer? _watch;
  int _seen = -1;
  final _state = ValueNotifier<SessionState?>(null);
  final _settings = ValueNotifier<AppSettings>(const AppSettings());

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
    bool withoutBridges = false,
  }) async {
    await close();
    if (_paths.librariesAreFiles) {
      for (final library in [_paths.coreLibrary, _paths.walletLibrary]) {
        if (!File(library).existsSync()) {
          throw StartFailure(Problem(libraryMissing, library));
        }
      }
    }
    await Directory(_paths.walletDir).create(recursive: true);
    final VeilCore core;
    try {
      core = _core ??= VeilCore.open(_paths.coreLibrary);
    } on ArgumentError catch (notLoaded) {
      // Where the libraries are names, not files, this is how a missing
      // one shows: the system does not find it.
      throw StartFailure(
        Problem(libraryMissing, '${_paths.coreLibrary}: ${notLoaded.message}'),
      );
    }
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
          bridges: withoutBridges ? const [] : _settings.value.bridges,
          syncEvery: _syncEvery,
        ),
        password: password,
        phrase: phrase,
      );
    } on VeilException catch (refused) {
      throw StartFailure(refused.problem);
    }
    final SessionState state;
    try {
      state = await _opened(session);
      if (source == WalletSource.restore) {
        await confirmPhrase();
      }
    } catch (_) {
      // Whatever went wrong, the session is not left running, with Tor
      // and perhaps a wallet, and nobody knowing of it.
      await _away(_paths.coreLibrary, session.address, _close);
      rethrow;
    }
    _session = session;
    _seen = state.revision;
    _state.value = state;
    _watch = Timer.periodic(_watchEvery, (_) => _look());
  }

  /// Waits for [session] to have its wallet open, and returns the state
  /// it is in then; throws a [StartFailure] with the reason if it has not.
  Future<SessionState> _opened(VeilSession session) async {
    // Opening a wallet takes a moment: its password is stretched on purpose.
    var state = session.state();
    while (state.phase == Phase.opening) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
      state = session.state();
    }
    if (state.phase != Phase.ready) {
      throw StartFailure(
        state.phaseProblem ?? const Problem(ProblemKind.other, 'not opened'),
      );
    }
    return state;
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
  ValueListenable<AppSettings> get settings => _settings;

  @override
  Future<void> loadSettings() async {
    _settings.value = await AppSettings.load(_paths.settingsFile);
  }

  @override
  Future<void> saveSettings(AppSettings settings) async {
    await settings.save(_paths.settingsFile);
    _settings.value = settings;
  }

  @override
  Problem? checkBridges(List<String> lines) {
    if (_paths.librariesAreFiles && !File(_paths.coreLibrary).existsSync()) {
      return Problem(libraryMissing, _paths.coreLibrary);
    }
    return (_core ??= VeilCore.open(_paths.coreLibrary)).checkBridges(lines);
  }

  @override
  Problem? setNodes(List<String> nodes) {
    try {
      _session?.setNodes(nodes);
      return null;
    } on VeilException catch (refused) {
      return refused.problem;
    }
  }

  @override
  Future<bool?> isPassword(String password) async {
    final session = _session;
    if (session == null) return null;
    return _waited(
      _away(_paths.coreLibrary, session.address, _IsPassword(password).call),
    );
  }

  @override
  Future<Problem?> changePassword(String old, String next) async {
    final session = _session;
    if (session == null) {
      return const Problem(ProblemKind.notNow, 'no wallet is open');
    }
    return _waited(
      _away(
        _paths.coreLibrary,
        session.address,
        _ChangePassword(old, next).call,
      ),
    );
  }

  /// The wallet's folder is all there is to remove: the wallet library
  /// writes nowhere else. Its build for Veil leaves out the database it
  /// would otherwise keep outside this folder, for all wallets of the
  /// machine.
  @override
  Future<void> removeWallet() async {
    await close();
    final dir = Directory(_paths.walletDir);
    if (await dir.exists()) await dir.delete(recursive: true);
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
  void clearSend() => _session?.clearSend();

  @override
  void sync() => _session?.sync();

  @override
  Future<String?> recoveryPhrase() async {
    final session = _session;
    if (session == null) return null;
    return _waited(_away(_paths.coreLibrary, session.address, _phrase));
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
  Future<Problem?> close() async {
    _watch?.cancel();
    _watch = null;
    final session = _session;
    _session = null;
    _state.value = null;
    if (session == null) return null;
    // Closing releases the session. A call still under way on another
    // isolate gets its answer first.
    while (_underWay.isNotEmpty) {
      await Future.wait(_underWay.toList());
    }
    final last = await _away(_paths.coreLibrary, session.address, _close);
    // A session that could not save its wallet on closing says so with
    // its last state. The wallet is closed either way.
    return last.phase == Phase.failed
        ? last.phaseProblem ?? const Problem(ProblemKind.other, 'not saved')
        : null;
  }

  /// Calls that wait for the session's thread and have not returned yet.
  final Set<Future<void>> _underWay = {};

  /// Keeps [call] on record until it has returned, for [close] to wait for.
  Future<T> _waited<T>(Future<T> call) {
    late final Future<void> record;
    record = call.then<void>((_) {}, onError: (_) {}).whenComplete(() {
      _underWay.remove(record);
    });
    _underWay.add(record);
    return call;
  }
}

/// Makes [call] on the session at [address] on another isolate: for the
/// calls that wait for the session's thread, which the main isolate, the
/// one that draws, must never do.
///
/// A function of the library and not a method of the backend, on purpose.
/// What is handed to another isolate takes along everything it can reach,
/// and a function made inside a method can reach the backend, through it
/// whoever listens to the backend, and so the screens, which cannot be
/// handed over: the call then fails before it is made. From here nothing
/// of the backend is in reach. [call] has to be a function of the library
/// too, or a method of an object that holds only what the call needs.
Future<T> _away<T>(String library, int address, T Function(VeilSession) call) =>
    Isolate.run(
      () => call(VeilSession.fromAddress(VeilCore.open(library), address)),
    );

SessionState _close(VeilSession session) => session.close();

String? _phrase(VeilSession session) => session.recoveryPhrase();

class _IsPassword {
  const _IsPassword(this.password);

  final String password;

  bool? call(VeilSession session) => session.isPassword(password);
}

class _ChangePassword {
  const _ChangePassword(this.old, this.next);

  final String old;
  final String next;

  Problem? call(VeilSession session) => session.changePassword(old, next);
}

/// A kind of problem only this side can have: a native part of the app is
/// not where it should be.
const libraryMissing = 'library_missing';
