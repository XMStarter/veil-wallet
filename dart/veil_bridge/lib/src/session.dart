/// A session in the native core, driven through its C interface.
library;

import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';

import 'state.dart';

/// Where a session's wallet comes from.
enum WalletSource { create, open, restore }

/// Which Monero network a wallet is on: the main one, where the coins are
/// real money, or stagenet, where they are worth nothing. A session is
/// always told which; there is no default.
enum WalletNetwork { mainnet, stagenet }

/// What a session is started with.
class SessionConfig {
  const SessionConfig({
    required this.network,
    required this.walletLibrary,
    required this.walletPath,
    required this.torStateDir,
    required this.torCacheDir,
    required this.source,
    this.nodes = const [],
    this.bridges = const [],
    this.transportDir,
    this.restoreHeight,
    this.syncEvery,
  });

  /// The network the wallet is on.
  final WalletNetwork network;

  /// The Monero wallet library to load.
  final String walletLibrary;

  /// The wallet's data file; its keys file lies next to it.
  final String walletPath;

  /// Directories private to the app, for Tor's files.
  final String torStateDir;
  final String torCacheDir;

  /// The user's own nodes, as `host.onion:port`, in the order to try them.
  /// Empty: the public nodes of [network], in an order nobody can predict.
  final List<String> nodes;

  /// Bridge lines to reach Tor through, one bridge each. Empty: Tor is
  /// connected to directly. With bridges set they are the only way in.
  final List<String> bridges;

  /// The folder the app keeps the transport programs it brings along in,
  /// if it brings any: looked in before the programs installed on the
  /// machine. A program may lie there as `lib<name>.so`, which is how one
  /// travels in a package for Android.
  final String? transportDir;
  final WalletSource source;

  /// Where scanning starts when restoring from a 25-word phrase.
  final int? restoreHeight;

  /// Synchronise by itself this often once Tor is ready. Whole seconds.
  final Duration? syncEvery;

  Map<String, Object?> toJson() => {
    'network': network.name,
    'source': source.name,
    'wallet_library': walletLibrary,
    'wallet_path': walletPath,
    'tor': {'state_dir': torStateDir, 'cache_dir': torCacheDir},
    'nodes': nodes,
    'bridges': bridges,
    'transport_dir': ?transportDir,
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
typedef _CheckC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _IsPasswordC = Int32 Function(Pointer<_Handle>, Pointer<Utf8>);
typedef _IsPasswordDart = int Function(Pointer<_Handle>, Pointer<Utf8>);
typedef _ChangePasswordC = Pointer<Utf8> Function(
  Pointer<_Handle>,
  Pointer<Utf8>,
  Pointer<Utf8>,
);
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
      _checkBridges = library.lookupFunction<_CheckC, _CheckC>(
        'veil_check_bridges',
      ),
      _isPassword = library.lookupFunction<_IsPasswordC, _IsPasswordDart>(
        'veil_session_is_password',
      ),
      _changePassword = library
          .lookupFunction<_ChangePasswordC, _ChangePasswordC>(
            'veil_session_change_password',
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
  final _CheckC _checkBridges;
  final _IsPasswordDart _isPassword;
  final _ChangePasswordC _changePassword;
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

  /// Checks bridge lines without starting anything. Null if every line is
  /// a bridge and a program is installed for every transport they need;
  /// otherwise the problem, of the kind [ProblemKind.invalidBridge] or
  /// [ProblemKind.transportMissing]. [transportDir] is the folder of the
  /// programs the app brings along, as in [SessionConfig.transportDir].
  Problem? checkBridges(List<String> lines, {String? transportDir}) {
    final request = jsonEncode({
      'bridges': lines,
      'transport_dir': ?transportDir,
    }).toNativeUtf8();
    try {
      return _takeProblem(_checkBridges(request));
    } finally {
      calloc.free(request);
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
/// the state never waits for the network. [recoveryPhrase], [isPassword],
/// [changePassword] and [close] do wait, for the session's thread, which
/// stops a synchronisation for them, normally within seconds. An app calls
/// them off its main isolate; [address] is what to send to another
/// isolate, and [VeilSession.fromAddress] takes it there.
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

  /// Drops the transaction under review or the send waiting to be built,
  /// or takes a send that failed off the state. It never touches a send
  /// that went out or may have.
  void cancelSend() => _send({'command': 'cancel_send'});

  /// Takes the outcome of the last send off the state, once the user has
  /// seen it: sent, failed, or unknown and settled.
  void clearSend() => _send({'command': 'clear_send'});

  /// Uses the user's own nodes, as `host.onion:port` in the order to try
  /// them, and keeps them in the wallet; an empty list goes back to the
  /// nodes the session was started with. What is in force shows in the
  /// state's `ownNodes`.
  ///
  /// Throws a [VeilException] at once if one of them is not a node.
  void setNodes(List<String> nodes) =>
      _send({'command': 'set_nodes', 'nodes': nodes});

  /// Whether [password] is the wallet's password; null if that cannot be
  /// said now.
  bool? isPassword(String password) {
    final passwordC = password.toNativeUtf8();
    try {
      return switch (_core._isPassword(_handle, passwordC)) {
        1 => true,
        0 => false,
        _ => null,
      };
    } finally {
      _core._wipe(passwordC);
    }
  }

  /// Gives the wallet the password [next], if [old] is its password now.
  /// Returns null when it is changed; otherwise the problem, of the kind
  /// [ProblemKind.wrongPassword] when [old] is not the password, and
  /// nothing is changed.
  Problem? changePassword(String old, String next) {
    final oldC = old.toNativeUtf8();
    final nextC = next.toNativeUtf8();
    try {
      return _core._takeProblem(_core._changePassword(_handle, oldC, nextC));
    } finally {
      _core._wipe(oldC);
      _core._wipe(nextC);
    }
  }

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
