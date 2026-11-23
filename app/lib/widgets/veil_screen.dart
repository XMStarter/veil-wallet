import 'package:flutter/material.dart';

import 'veil_column.dart';

/// A screen with its title at the top, its content in the middle, and its
/// actions at the bottom, in the app's single column. The content scrolls
/// when the window is too short for it.
class VeilScreen extends StatelessWidget {
  const VeilScreen({
    super.key,
    required this.title,
    required this.children,
    this.actions = const [],
  });

  /// Shown in uppercase.
  final String title;
  final List<Widget> children;

  /// Buttons, the main one first.
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: VeilColumn(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(height: 8),
            Text(
              title.toUpperCase(),
              style: Theme.of(context).textTheme.headlineMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 24),
            Expanded(
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: children,
                ),
              ),
            ),
            for (final action in actions) ...[
              const SizedBox(height: 12),
              action,
            ],
          ],
        ),
      ),
    );
  }
}
