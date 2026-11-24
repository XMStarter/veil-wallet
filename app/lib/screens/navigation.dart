import 'package:flutter/widgets.dart';

/// A route that appears at once: no animation except progress.
Route<T> at<T>(Widget screen) => PageRouteBuilder<T>(
  pageBuilder: (_, _, _) => screen,
  transitionDuration: Duration.zero,
  reverseTransitionDuration: Duration.zero,
);

/// Opens [screen] on top of the current one.
Future<void> open(BuildContext context, Widget screen) =>
    Navigator.of(context).push(at<void>(screen));

/// Opens [screen] on top of the current one and completes with what it
/// was closed with.
Future<T?> openFor<T>(BuildContext context, Widget screen) =>
    Navigator.of(context).push(at<T>(screen));

/// Opens [screen] in place of everything before it: there is no way back.
void openAlone(BuildContext context, Widget screen) =>
    Navigator.of(context).pushAndRemoveUntil(at<void>(screen), (_) => false);
