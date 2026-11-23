import 'package:flutter/foundation.dart';
import 'package:veil_bridge/veil_bridge.dart';

export 'package:veil_bridge/veil_bridge.dart'
    show
        Direction,
        Phase,
        Problem,
        ProblemKind,
        ReceiveAddress,
        SessionState,
        SyncPhase,
        SyncStatus,
        TorState,
        TorStatus,
        Transfer,
        WalletSource;

/// The wallet could not be created, restored or opened, and why.
class StartFailure implements Exception {
  const StartFailure(this.problem);

  final Problem problem;

  @override
  String toString() => 'StartFailure($problem)';
}

/// What the screens need from the wallet. The app gives them the real one,
/// which runs the core; tests give them one that only pretends.
abstract class WalletBackend {
  /// Whether this device already has a wallet.
  Future<bool> walletExists();

  /// Creates, restores or opens the wallet and starts Tor. Completes when
  /// the wallet is open; throws a [StartFailure] if it does not open.
  /// From then on the wallet synchronises by itself: as soon as Tor is
  /// connected, and again and again while it is open.
  ///
  /// [phrase] and [restoreHeight] are for [WalletSource.restore].
  Future<void> start(
    WalletSource source, {
    required String password,
    String? phrase,
    int? restoreHeight,
  });

  /// The state of the open wallet; null while there is none.
  ValueListenable<SessionState?> get state;

  /// Asks for another receive address although the one shown has not been
  /// paid. Returns at once; the address appears in [state], or the reason
  /// there is none in its `lastProblem`.
  void newAddress();

  /// The recovery phrase of the open wallet.
  Future<String?> recoveryPhrase();

  /// Whether the user has shown that they wrote the phrase down. A wallet
  /// that was restored from its phrase counts as confirmed.
  Future<bool> phraseConfirmed();

  Future<void> confirmPhrase();

  /// Saves and closes the wallet and stops Tor. A synchronisation under
  /// way is stopped; what it had scanned is kept.
  Future<void> close();
}
