// About Veil: what this is a build of, and the licence texts a package
// has to carry, read from the files the app is built with.

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/notices.dart';
import 'package:veil/l10n/app_localizations.dart';
import 'package:veil/screens/about_screen.dart';
import 'package:veil/theme/theme.dart';

import 'settings_test.dart' show openLine, openSettings;
import 'support.dart';

final _divider = '-' * 80;

/// A package whose files are these, and nothing else.
class _Files extends CachingAssetBundle {
  _Files(this.files);

  final Map<String, String> files;

  @override
  Future<ByteData> load(String key) async {
    final text = files[key];
    if (text == null) throw FlutterError('no asset $key');
    return ByteData.sublistView(Uint8List.fromList(text.codeUnits));
  }
}

Future<void> _show(
  WidgetTester tester,
  Widget screen, {
  String language = 'en',
}) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: veilTheme(),
      locale: Locale(language),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: screen,
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(loadFonts);

  test('the version said in the app is the one the package has', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    final version = RegExp(
      r'^version: ([0-9.]+)\+',
      multiLine: true,
    ).firstMatch(pubspec)!.group(1);
    expect(veilVersion, version);
  });

  test('a file of licence texts is read entry by entry', () {
    final texts = licenceTextsIn(
      'one 1.0\ntwo 2.0\n\nFirst text.\n\nIts second paragraph.\n'
      '$_divider\n'
      'three 3.0\r\n\r\nSecond text.\r\n'
      '$_divider\n'
      '\n\n'
      '$_divider\n'
      'A text that names nothing and has no empty line.\n',
    );
    expect(texts, hasLength(3));
    expect(texts[0].names, ['one 1.0', 'two 2.0']);
    expect(texts[0].text, 'First text.\n\nIts second paragraph.');
    expect(texts[1].names, ['three 3.0']);
    expect(texts[1].text, 'Second text.');
    // Not of the shape: kept whole, so that it can still be read.
    expect(texts[2].names, isEmpty);
    expect(texts[2].text, 'A text that names nothing and has no empty line.');
    expect(licenceTextsIn(''), isEmpty);
  });

  test('a line of dashes that is not eighty long divides nothing', () {
    final texts = licenceTextsIn(
      'part\n\nAbove.\n${'-' * 79}\nBelow.\n${'-' * 81}\nEnd.\n',
    );
    expect(texts, hasLength(1));
    expect(texts.single.text, contains('Below.'));
    expect(texts.single.text, endsWith('End.'));
  });

  // The files as they are in the repository, which is what a package is
  // built with. `native/notices.sh --check` holds them to their sources;
  // here it is that the app can read every one, and that the texts a
  // package may not be without are in them.
  group('the licence texts the app is built with', () {
    List<LicenceText> read(NoticePart part) =>
        licenceTextsIn(File('assets/licences/${part.file}').readAsStringSync());

    test('every part has its file, and every text names what it is for', () {
      for (final part in NoticePart.values) {
        if (part.file == null) continue;
        final texts = read(part);
        expect(texts, isNotEmpty, reason: '${part.file}');
        for (final licence in texts) {
          expect(licence.names, isNotEmpty, reason: '${part.file}');
          expect(licence.text, isNotEmpty, reason: '${part.file}');
        }
      }
    });

    test('Veil\'s own is the licence in the repository', () {
      final own = read(NoticePart.veil).single;
      expect(own.names, ['Veil']);
      expect(own.text, File('../LICENSE').readAsStringSync().trim());
    });

    test('the wallet library comes with both GNU licences and Monero\'s', () {
      final texts = read(NoticePart.walletLibrary);
      LicenceText of(String start) => texts.firstWhere(
        (licence) => licence.text.trimLeft().startsWith(start),
      );
      final lesser = of('GNU LESSER GENERAL PUBLIC LICENSE');
      expect(lesser.names.join('\n'), contains('monero_c'));
      expect(lesser.names, contains('polyseed'));
      // The Lesser licence builds on the General one and asks for it.
      final general = of('GNU GENERAL PUBLIC LICENSE');
      expect(general.names.join('\n'), contains('monero_c'));
      expect(
        texts.any(
          (licence) =>
              licence.names.contains('Monero') &&
              licence.text.contains('The Monero Project'),
        ),
        isTrue,
      );
    });

    test('the transport program comes with the GNU General Public License', () {
      final texts = read(NoticePart.transport);
      final general = texts.firstWhere(
        (licence) =>
            licence.text.trimLeft().startsWith('GNU GENERAL PUBLIC LICENSE'),
      );
      expect(general.names.single, startsWith('lyrebird'));
      expect(
        texts.any(
          (licence) => licence.names.any((name) => name.startsWith('Go ')),
        ),
        isTrue,
      );
    });

    test('the core comes with the texts of some hundreds of packages', () {
      final texts = read(NoticePart.core);
      final names = {for (final licence in texts) ...licence.names};
      expect(names.length, greaterThan(300));
      expect(names.any((name) => name.startsWith('arti-client ')), isTrue);
      // Veil's own crates are not other people's work.
      expect(names.any((name) => name.startsWith('veil_')), isFalse);
    });
  });

  testWidgets('settings lead to what this is a build of and to the licences', (
    tester,
  ) async {
    // A test is taken for Android unless told otherwise; this one is
    // about a desktop.
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    await openSettings(tester, 'en');
    expect(find.text('Version $veilVersion, and the licences'), findsOneWidget);
    await openLine(tester, 'About Veil');
    expect(find.text('ABOUT VEIL'), findsOneWidget);
    expect(find.textContaining('This is Veil $veilVersion'), findsOneWidget);
    expect(find.textContaining(veilSource), findsOneWidget);
    expect(
      find.textContaining('GNU Lesser General Public License'),
      findsOneWidget,
    );
    // A desktop's package has no transport program and no Android
    // libraries, and does not show texts for them.
    expect(find.text('Wallet library'), findsOneWidget);
    expect(find.text('Packages in the core'), findsOneWidget);
    expect(find.text('Transport program for bridges'), findsNothing);
    expect(find.text('Libraries for Android'), findsNothing);
    expect(tester.takeException(), isNull);

    await press(tester, 'Back');
    expect(find.text('SETTINGS'), findsOneWidget);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('a part\'s texts are listed by what they come with, and read', (
    tester,
  ) async {
    final files = _Files({
      'assets/licences/wallet-library.txt':
          'Library 1.0\nHelper 2.0\nThird 3.0\n\nThe first text, word for word.\n'
          '$_divider\n'
          'Library 1.0\n\nThe second text.\n',
    });
    await _show(
      tester,
      AboutScreen(
        onAndroid: true,
        network: WalletNetwork.stagenet,
        bundle: files,
      ),
    );
    // The app for Android says what else is in its package.
    expect(
      find.textContaining(
        'transport program for bridges under the GNU General',
      ),
      findsOneWidget,
    );
    expect(find.text('Transport program for bridges'), findsOneWidget);
    expect(find.text('Libraries for Android'), findsOneWidget);

    await openLine(tester, 'Wallet library');
    expect(find.text('WALLET LIBRARY'), findsOneWidget);
    expect(find.text('2 licence texts'), findsOneWidget);
    expect(find.text('Library 1.0'), findsOneWidget);
    await openLine(tester, 'Library 1.0 and 2 more');
    expect(find.text('Library 1.0\nHelper 2.0\nThird 3.0'), findsOneWidget);
    expect(find.text('The first text, word for word.'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await press(tester, 'Back');
    await press(tester, 'Back');
    expect(find.text('ABOUT VEIL'), findsOneWidget);
  });

  testWidgets('a package without a part\'s texts says that it is not right', (
    tester,
  ) async {
    await _show(
      tester,
      AboutScreen(
        onAndroid: false,
        network: WalletNetwork.stagenet,
        bundle: _Files({}),
      ),
    );
    await openLine(tester, 'Packages in the core');
    expect(find.textContaining('are not in this package'), findsOneWidget);
    expect(tester.takeException(), isNull);

    // The same for a file that is there and holds nothing.
    await press(tester, 'Back');
    await _show(
      tester,
      AboutScreen(
        onAndroid: false,
        network: WalletNetwork.mainnet,
        bundle: _Files({'assets/licences/veil.txt': '\n'}),
      ),
    );
    await openLine(tester, 'Veil');
    expect(find.textContaining('are not in this package'), findsOneWidget);
  });

  testWidgets('about fits the window in Russian too', (tester) async {
    final files = _Files({
      'assets/licences/core.txt': [
        for (var number = 1; number <= 21; number++)
          'package-$number 1.0\n\nText $number.\n',
      ].join('$_divider\n'),
    });
    await _show(
      tester,
      AboutScreen(
        onAndroid: true,
        network: WalletNetwork.stagenet,
        bundle: files,
      ),
      language: 'ru',
    );
    expect(find.text('О VEIL'), findsOneWidget);
    expect(find.text('Программа-транспорт для мостов'), findsOneWidget);
    await openLine(tester, 'Пакеты в ядре');
    expect(find.text('21 текст лицензии'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
