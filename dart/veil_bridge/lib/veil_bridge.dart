/// The Dart side of Veil's core.
///
/// [VeilCore] loads the native library; [VeilCore.start] opens a wallet and
/// starts Tor on a thread of the library's own and returns a [VeilSession].
/// The session takes commands, which return at once, and hands out a
/// [SessionState] to draw from.
///
/// Amounts are text here, exact to the last digit, as the core wrote them.
/// This side shows them and passes them back; it never calculates with
/// them.
library;

export 'src/session.dart';
export 'src/state.dart';
