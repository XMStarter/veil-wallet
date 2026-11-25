import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'app.dart';
import 'core/core_backend.dart';
import 'core/paths.dart';

Future<void> main() async {
  runApp(VeilApp(backend: CoreBackend(await _paths())));
}

/// Where the app keeps its files. A desktop says so in its environment; on
/// Android the system has to be asked, which is the one thing the app's
/// Android side is there for.
Future<AppPaths> _paths() async {
  if (!Platform.isAndroid) return AppPaths.forThisDevice();
  WidgetsFlutterBinding.ensureInitialized();
  const device = MethodChannel('veil/device');
  final filesDir = await device.invokeMethod<String>('filesDir');
  return AppPaths.onAndroid(filesDir!);
}
