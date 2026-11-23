import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/app.dart';

/// Loads the app's own typefaces, so that tests lay text out as the app
/// does and not in the test framework's stand-in font.
Future<void> loadFonts() async {
  for (final (family, file) in [
    ('Cormorant Garamond', 'CormorantGaramond.ttf'),
    ('Jost', 'Jost.ttf'),
    ('JetBrains Mono', 'JetBrainsMono.ttf'),
  ]) {
    final bytes = File('fonts/$file').readAsBytesSync();
    final loader = FontLoader(family)
      ..addFont(Future.value(ByteData.sublistView(bytes)));
    await loader.load();
  }
}

/// A phone-sized window, which is also the app's desktop window.
const windowSize = Size(420, 800);

/// Starts the app in [language] in a window of [windowSize].
Future<void> pumpApp(WidgetTester tester, String language) async {
  tester.view.physicalSize = windowSize;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(VeilApp(locale: Locale(language)));
  await tester.pumpAndSettle();
}

/// Starts the app in English in whatever window the test has set up.
Future<void> pumpAppWide(WidgetTester tester) async {
  await tester.pumpWidget(const VeilApp(locale: Locale('en')));
  await tester.pumpAndSettle();
}
