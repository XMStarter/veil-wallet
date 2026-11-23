import '../core/backend.dart';
import '../core/core_backend.dart' show libraryMissing;
import '../l10n/app_localizations.dart';

/// A problem in the user's language. Kinds without words of their own are
/// shown with the core's detail, which is in English.
String problemWords(AppLocalizations text, Problem problem) {
  return switch (problem.kind) {
    ProblemKind.wrongPassword => text.problemWrongPassword,
    ProblemKind.walletExists => text.problemWalletExists,
    ProblemKind.invalidPhrase => text.problemInvalidPhrase,
    ProblemKind.torFailed => text.problemTorFailed,
    libraryMissing =>
      '${text.problemLibraryMissing} ${text.problemDetail(problem.detail)}',
    _ => '${text.problemOther} ${text.problemDetail(problem.detail)}',
  };
}
