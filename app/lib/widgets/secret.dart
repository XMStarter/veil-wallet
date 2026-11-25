import 'package:flutter/widgets.dart';

import '../core/device.dart';

/// Wraps a screen that shows or takes a recovery phrase: for as long as it
/// is there, the system takes no picture of the app. See
/// [Device.secretShown].
class Secret extends StatefulWidget {
  const Secret({super.key, required this.device, required this.child});

  final Device device;
  final Widget child;

  @override
  State<Secret> createState() => _SecretState();
}

class _SecretState extends State<Secret> {
  @override
  void initState() {
    super.initState();
    widget.device.secretShown();
  }

  @override
  void dispose() {
    widget.device.secretGone();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
