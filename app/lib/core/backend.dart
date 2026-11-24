import 'package:flutter/foundation.dart';
import 'package:veil_bridge/veil_bridge.dart';

import 'settings.dart';

export 'settings.dart' show AppSettings;

export 'package:veil_bridge/veil_bridge.dart'
    show
        Direction,
        Phase,
        Problem,
        ProblemKind,
        ReceiveAddress,
        SendPhase,
        SendStatus,
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

/// The amount that stands for "all that can be spent, less the fee".
const everything = 'all';

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
  /// [withoutBridges] leaves the bridges in the settings aside for this
  /// once: the way in when they have stopped working.
  Future<void> start(
    WalletSource source, {
    required String password,
    String? phrase,
    int? restoreHeight,
    bool withoutBridges = false,
  });

  /// How the app is set to behave. Loaded by [loadSettings].
  ValueListenable<AppSettings> get settings;

  /// Reads the settings from where they are kept. Called once, at the
  /// start.
  Future<void> loadSettings();

  /// Keeps [settings] and puts them into force. Bridges take effect the
  /// next time a wallet is opened.
  Future<void> saveSettings(AppSettings settings);

  /// Checks bridge lines without starting anything. Null if they can be
  /// used; otherwise the reason they cannot.
  Problem? checkBridges(List<String> lines);

  /// Uses the user's own nodes, as `host.onion:port` in the order to try
  /// them, and has the wallet remember them; none goes back to the
  /// public ones. Returns at once: what is in force appears in [state].
  /// What is returned here is only a node that is not written as one.
  Problem? setNodes(List<String> nodes);

  /// Whether [password] is the open wallet's password; null if that
  /// cannot be said now.
  Future<bool?> isPassword(String password);

  /// Gives the open wallet the password [next], if [old] is its password
  /// now. Null when it is changed; otherwise the reason it is not.
  Future<Problem?> changePassword(String old, String next);

  /// Closes the wallet and removes it from this device: its keys, its
  /// data, and what the app remembered about it. Only its recovery
  /// phrase brings it back.
  Future<void> removeWallet();

  /// The state of the open wallet; null while there is none.
  ValueListenable<SessionState?> get state;

  /// Asks for another receive address although the one shown has not been
  /// paid. Returns at once; the address appears in [state], or the reason
  /// there is none in its `lastProblem`.
  void newAddress();

  /// Builds a transaction that pays [amount] to [to], for the user to
  /// review. [amount] is a number as typed, or [everything]. Returns at
  /// once: the transaction, or the reason there is none, appears in
  /// [state] as its `send`. What is returned here is only a request the
  /// core could not even read, such as an amount that is not a number.
  Problem? prepareSend(String to, String amount);

  /// Sends the transaction under review.
  void confirmSend();

  /// Drops the transaction under review or being prepared, or clears the
  /// outcome of the last send.
  void cancelSend();

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
