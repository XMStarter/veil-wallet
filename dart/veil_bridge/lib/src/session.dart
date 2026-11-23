/// A session in the native core, driven through its C interface.
library;

import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';

import 'state.dart';

/// Where a session's wallet comes from.
enum WalletSource { create, open, restore }

/// What a session is started with.
class SessionConfig {
  const SessionConfig({
    required this.walletLibrary,
    required this.walletPath,
    required this.torStateDir,
    required this.torCacheDir,
    required this.source,
    this.nodes = const [],
    this.restoreHeight,
    this.syncEvery,
  });

  /// The Monero wallet library to load.
  final String walletLibrary;

  /// The wallet's data file; its keys file lies next to it.
  final String walletPath;

  /// Directories private to the app, for Tor's files.
  final String torStateDir;
  final String torCacheDir;

  /// The user's own nodes, as `host.onion:port`, in the order to try them.
  /// Empty: the public stagenet nodes, in an order nobody can predict.
  final List<String> nodes;
  final WalletSource source;

  /// Where scanning starts when restoring from a 25-word phrase.
  final int? restoreHeight;

  /// Synchronise by itself this often once Tor is ready. Whole seconds.
  final Duration? syncEvery;

  Map<String, Object?> toJson() => {
    // Until the release steps the app works on stagenet only.
    'network': 'stagenet',
    'source': source.name,
    'wallet_library': walletLibrary,
    'wallet_path': walletPath,
    'tor': {'state_dir': torStateDir, 'cache_dir': torCacheDir},
    'nodes': nodes,
    'restore_height': ?restoreHeight,
    'sync_every_seconds': ?syncEvery?.inSeconds,
  };
}

/// The core refused a request, and why.
class VeilException implements Exception {
  const VeilException(this.problem);

  final Problem problem;

  @override
  String toString() => 'VeilException($problem)';
}

final class _Handle extends Opaque {}

typedef _StartC = Pointer<_Handle> Function(
  Pointer<Utf8>,
  Pointer<Utf8>,
  Pointer<Utf8>,
  Pointer<Pointer<Utf8>>,
);
typedef _RevisionC = Uint64 Function(Pointer<_Handle>);
typedef _RevisionDart = int Function(Pointer<_Handle>);
typedef _TextOf = Pointer<Utf8> Function(Pointer<_Handle>);
typedef _CommandC = Pointer<Utf8> Function(Pointer<_Handle>, Pointer<Utf8>);
typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

/// The native library with Veil's core in it.
class VeilCore {
  /// Loads the library at [path].
  VeilCore.open(String path) : this._(DynamicLibrary.open(path));

  VeilCore._(DynamicLibrary library)
    : _start = library.lookupFunction<_StartC, _StartC>('veil_session_start'),
      _revision = library.lookupFunction<_RevisionC, _RevisionDart>(
        'veil_session_revision',
      ),
      _state = library.lookupFunction<_TextOf, _TextOf>('veil_session_state'),
      _command = library.lookupFunction<_CommandC, _CommandC>(
        'veil_session_command',
      ),
      _phrase = library.lookupFunction<_TextOf, _TextOf>(
        'veil_session_recovery_phrase',
      ),
      _close = library.lookupFunction<_TextOf, _TextOf>('veil_session_close'),
      _freeText = library.lookupFunction<_FreeC, _FreeDart>('veil_string_free'),
      _freeSecret = library.lookupFunction<_FreeC, _FreeDart>(
        'veil_secret_free',
      );

  final _StartC _start;
  final _RevisionDart _revision;
  final _TextOf _state;
  final _CommandC _command;
  final _TextOf _phrase;
  final _TextOf _close;
  final _FreeDart _freeText;
  final _FreeDart _freeSecret;

  /// Opens a wallet and starts Tor on a thread of the library's own, and
  /// returns at once. [VeilSession.state] tells when the wallet is open, or
  /// why it is not.
  ///
  /// [phrase] is the recovery phrase, for [WalletSource.restore] only.
  /// Throws a [VeilException] if the request itself cannot be read.
  VeilSession start(
    SessionConfig config, {
    required String password,
    String? phrase,
  }) {
    final configC = jsonEncode(config.toJson()).toNativeUtf8();
    final passwordC = password.toNativeUtf8();
    final phraseC = phrase == null ? nullptr : phrase.toNativeUtf8();
    final problem = calloc<Pointer<Utf8>>();
    try {
      final handle = _start(configC, passwordC, phraseC, problem);
      if (handle == nullptr) {
        throw VeilException(_takeProblem(problem.value)!);
      }
      return VeilSession._(this, handle);
    } finally {
      calloc.free(configC);
      _wipe(passwordC);
      if (phraseC != nullptr) _wipe(phraseC);
      calloc.free(problem);
    }
  }

  /// Overwrites a secret this side copied for the call, then frees it.
  void _wipe(Pointer<Utf8> secret) {
    final bytes = secret.cast<Uint8>();
    for (var at = 0; bytes[at] != 0; at++) {
      bytes[at] = 0;
    }
    calloc.free(secret);
  }

  /// Reads text the library handed out and gives it back to be freed.
  String? _take(Pointer<Utf8> text) {
    if (text == nullptr) return null;
    try {
      return text.toDartString();
    } finally {
      _freeText(text);
    }
  }

  Problem? _takeProblem(Pointer<Utf8> text) {
    final json = _take(text);
    return json == null ? null : Problem.maybe(jsonDecode(json));
  }
}

/// A wallet running on a thread of the native library.
///
/// Commands return at once; what came of them shows in [state]. Reading
/// the state never waits for the network. [recoveryPhrase] and [close] do
/// wait, for the session's thread: for the end of a synchronisation if one
/// is running. An app calls them off its main isolate; [address] is what
/// to send to another isolate, and [VeilSession.fromAddress] takes it
/// there.
class VeilSession {
  VeilSession._(this._core, this._handle);

  /// The session at [address] in [core], for use from another isolate that
  /// has loaded the same library.
  VeilSession.fromAddress(VeilCore core, int address)
    : this._(core, Pointer.fromAddress(address));

  final VeilCore _core;
  Pointer<_Handle> _handle;

  int get address => _handle.address;

  /// Goes up with every change of the state. Cheap enough to ask on every
  /// frame; zero once the session is closed.
  int get revision => _core._revision(_handle);

  /// The state as it is now.
  SessionState state() {
    final json = _core._take(_core._state(_handle));
    if (json == null) throw StateError('the session is closed');
    return SessionState.fromJson(jsonDecode(json) as Map<String, dynamic>);
  }

  /// Synchronises with the first node that carries it through.
  void sync() => _send({'command': 'sync'});

  /// Issues another receive address although the current one is unused.
  void newAddress() => _send({'command': 'new_address'});

  /// Builds a transaction for review. [amount] is a number of coins as the
  /// user typed it, or `all`. Nothing is sent.
  ///
  /// Throws a [VeilException] at once if [amount] is not a number; other
  /// problems, such as an address that is not one, show in the state.
  void prepareSend(String to, String amount) =>
      _send({'command': 'prepare_send', 'to': to, 'amount': amount});

  /// Sends the transaction under review.
  void confirmSend() => _send({'command': 'confirm_send'});

  /// Drops the transaction under review, or clears the outcome of the last
  /// send from the state.
  void cancelSend() => _send({'command': 'cancel_send'});

  /// The wallet's recovery phrase, or null if it cannot be given now.
  String? recoveryPhrase() {
    final secret = _core._phrase(_handle);
    if (secret == nullptr) return null;
    try {
      return secret.toDartString();
    } finally {
      _core._freeSecret(secret);
    }
  }

  /// Saves and closes the wallet and stops Tor. Returns the last state;
  /// its phase is [Phase.closed], or [Phase.failed] with the reason. The
  /// session must not be used afterwards.
  SessionState close() {
    final json = _core._take(_core._close(_handle));
    _handle = nullptr;
    if (json == null) throw StateError('the session was closed already');
    return SessionState.fromJson(jsonDecode(json) as Map<String, dynamic>);
  }

  void _send(Map<String, Object?> command) {
    final text = jsonEncode(command).toNativeUtf8();
    try {
      final refused = _core._takeProblem(_core._command(_handle, text));
      if (refused != null) throw VeilException(refused);
    } finally {
      calloc.free(text);
    }
  }
}
