import 'package:flutter/widgets.dart';

/// A route that appears at once: no animation except progress.
Route<T> _at<T>(Widget screen) => PageRouteBuilder<T>(
  pageBuilder: (_, _, _) => screen,
  transitionDuration: Duration.zero,
  reverseTransitionDuration: Duration.zero,
);

/// Opens [screen] on top of the current one.
Future<void> open(BuildContext context, Widget screen) =>
    Navigator.of(context).push(_at<void>(screen));

/// Opens [screen] in place of everything before it: there is no way back.
void openAlone(BuildContext context, Widget screen) =>
    Navigator.of(context).pushAndRemoveUntil(_at<void>(screen), (_) => false);
