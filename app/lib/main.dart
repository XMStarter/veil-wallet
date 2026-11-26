import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'app.dart';
import 'core/core_backend.dart';
import 'core/network.dart';
import 'core/paths.dart';

Future<void> main() async {
  runApp(VeilApp(backend: CoreBackend(await _paths())));
}

/// Where the app keeps its files. A desktop says so in its environment; on
/// Android the system has to be asked, for the app's own storage and for
/// the folder its package's native parts were unpacked into.
Future<AppPaths> _paths() async {
  final network = networkNamed(builtFor);
  if (!Platform.isAndroid) return AppPaths.forThisDevice(network: network);
  WidgetsFlutterBinding.ensureInitialized();
  const device = MethodChannel('veil/device');
  final filesDir = await device.invokeMethod<String>('filesDir');
  final libraryDir = await device.invokeMethod<String>('libraryDir');
  return AppPaths.onAndroid(
    filesDir!,
    libraryDir: libraryDir,
    network: network,
  );
}
