import 'package:flutter/material.dart';

import '../theme/theme.dart';

/// The single column every screen is laid out in: as wide as a phone, also
/// in a desktop window.
class VeilColumn extends StatelessWidget {
  const VeilColumn({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: VeilSizes.columnWidth),
          child: Padding(
            padding: const EdgeInsets.all(VeilSizes.screenPadding),
            child: child,
          ),
        ),
      ),
    );
  }
}
