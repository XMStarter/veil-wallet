import '../core/backend.dart';
import '../core/core_backend.dart' show libraryMissing;
import '../l10n/app_localizations.dart';

/// What went wrong in a way nobody foresaw, as a problem to show. A screen
/// that waits for the wallet shows it rather than wait for ever.
Problem unforeseen(Object error) => Problem(ProblemKind.other, '$error');

/// A problem in the user's language. Kinds without words of their own are
/// shown with the core's detail, which is in English.
String problemWords(AppLocalizations text, Problem problem) {
  return switch (problem.kind) {
    ProblemKind.wrongPassword => text.problemWrongPassword,
    ProblemKind.walletExists => text.problemWalletExists,
    ProblemKind.invalidPhrase => text.problemInvalidPhrase,
    ProblemKind.torFailed => text.problemTorFailed,
    ProblemKind.torNotReady => text.problemTorNotReady,
    ProblemKind.noNode => text.problemNoNode,
    ProblemKind.tooManyUnusedAddresses => text.problemTooManyAddresses,
    ProblemKind.invalidAddress => text.problemInvalidAddress,
    ProblemKind.invalidAmount => text.problemInvalidAmount,
    ProblemKind.zeroAmount => text.problemZeroAmount,
    ProblemKind.notEnoughFunds => text.problemNotEnoughFunds,
    ProblemKind.notNow => text.problemNotNow,
    ProblemKind.invalidNode => text.problemInvalidNode,
    // The core says which line, or which program; in English, but it is
    // what the user needs to put it right.
    ProblemKind.invalidBridge =>
      '${text.problemInvalidBridge} ${text.problemDetail(problem.detail)}',
    ProblemKind.transportMissing =>
      '${text.problemTransportMissing} ${text.problemDetail(problem.detail)}',
    libraryMissing =>
      '${text.problemLibraryMissing} ${text.problemDetail(problem.detail)}',
    _ => '${text.problemOther} ${text.problemDetail(problem.detail)}',
  };
}
