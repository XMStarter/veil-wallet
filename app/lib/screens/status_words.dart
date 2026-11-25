import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import 'problem_words.dart';

/// What Tor is doing, in words.
String torWords(AppLocalizations text, TorStatus tor) => switch (tor.state) {
  TorState.ready => text.torReady,
  TorState.bootstrapping => text.torConnecting(tor.percent),
  TorState.failed => text.torFailed,
  TorState.lost => text.torLost,
};

/// How the wallet stands against the chain, in words.
String syncWords(AppLocalizations text, SessionState state) {
  final sync = state.sync;
  switch (sync.phase) {
    case SyncPhase.notYet:
      return state.tor.state == TorState.ready
          ? text.syncNotYet
          : text.syncWaitingForTor;
    case SyncPhase.running:
      final left = sync.blocksLeft;
      return left == null || left == 0
          ? text.syncStarting
          : text.syncRunning(left);
    case SyncPhase.done:
      return text.syncDone;
    case SyncPhase.failed:
      final problem = sync.problem;
      return text.syncFailed(
        problem == null ? text.problemOther : problemWords(text, problem),
      );
  }
}
