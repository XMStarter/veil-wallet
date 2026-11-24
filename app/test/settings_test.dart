// Settings, on a wallet that only pretends.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/app.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/settings.dart';
import 'package:veil/theme/colors.dart';

import 'support.dart';

/// Opens the wallet and goes to Settings.
Future<FakeBackend> openSettings(WidgetTester tester, String language) async {
  final backend = await openWallet(tester, language);
  await press(tester, language == 'ru' ? 'Настройки' : 'Settings');
  return backend;
}

/// Opens the line of Settings named [title].
Future<void> openLine(WidgetTester tester, String title) async {
  await tester.ensureVisible(find.text(title));
  await tester.tap(find.text(title));
  await tester.pumpAndSettle();
}

const ownNode =
    'vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd.onion:38089';
const bridge = 'obfs4 192.0.2.55:443 316E643333645F6D79216558614D3931657A5F5F';

void main() {
  setUpAll(loadFonts);

  testWidgets('settings say how each thing stands', (tester) async {
    await openSettings(tester, 'en');
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(find.text('Tor is connecting: 15%'), findsOneWidget);
    expect(find.text('Public nodes'), findsOneWidget);
    expect(find.text('None'), findsOneWidget);
    expect(find.text('After 5 minutes'), findsOneWidget);
    expect(find.text('As on the device'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await press(tester, 'Back');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('a node of the user\'s own is set, refused, or taken back', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    await openLine(tester, 'Node');
    await type(tester, 'Your own nodes, one per line', 'example.com:18081');
    await press(tester, 'Save');
    // Not a node: said so, and nothing is set.
    expect(
      find.textContaining('One of the lines is not a node'),
      findsOneWidget,
    );
    expect(backend.state.value!.ownNodes, isEmpty);

    await type(tester, 'Your own nodes, one per line', ' $ownNode \n');
    expect(find.textContaining('is not a node'), findsNothing);
    await press(tester, 'Save');
    expect(backend.calls.last, 'set nodes $ownNode');
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(find.text('Your own'), findsOneWidget);

    // The field shows what is in force, and emptied, goes back to the
    // public nodes.
    await openLine(tester, 'Node');
    expect(find.text(ownNode), findsOneWidget);
    await type(tester, 'Your own nodes, one per line', '');
    await press(tester, 'Save');
    expect(backend.calls.last, 'set nodes ');
    expect(find.text('Public nodes'), findsOneWidget);
  });

  testWidgets('bridges are checked before they are kept', (tester) async {
    final backend = await openSettings(tester, 'en');
    await openLine(tester, 'Tor bridges');
    await type(tester, 'Bridge lines, one per line', '$bridge\nnonsense');
    await press(tester, 'Save');
    expect(
      find.textContaining('One of the lines is not a bridge'),
      findsOneWidget,
    );
    expect(find.textContaining('bridge line 2'), findsOneWidget);
    expect(backend.settings.value.bridges, isEmpty);

    await type(
      tester,
      'Bridge lines, one per line',
      'snowflake 192.0.2.3:80 AA',
    );
    await press(tester, 'Save');
    expect(
      find.textContaining('is not installed on this device'),
      findsOneWidget,
    );
    expect(backend.settings.value.bridges, isEmpty);

    // Good ones are kept, comments and gaps left out, and the screen says
    // when they take effect.
    await type(tester, 'Bridge lines, one per line', '# mine\n\n$bridge\n');
    await press(tester, 'Save');
    expect(backend.settings.value.bridges, [bridge]);
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(find.text('1 bridge'), findsOneWidget);
    expect(
      find.textContaining('Lock the wallet and open it again'),
      findsOneWidget,
    );
  });

  testWidgets('bridges that stopped working do not lock the user out', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await backend.saveSettings(const AppSettings(bridges: [bridge]));
    backend.bridgesBroken = true;
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text('OPEN WITHOUT BRIDGES'), findsNothing);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    // The wallet does not open, and the reason and a way on are there.
    expect(
      find.textContaining('is not installed on this device'),
      findsOneWidget,
    );
    expect(
      find.textContaining('Without them Tor is connected to directly'),
      findsOneWidget,
    );

    await press(tester, 'Open without bridges');
    expect(backend.calls.last, 'start open without bridges');
    expect(find.text('WALLET'), findsOneWidget);
    // The bridges are still in the settings, to be put right there.
    expect(backend.settings.value.bridges, [bridge]);
  });

  testWidgets('the password is changed by whoever knows the current one', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    await openLine(tester, 'Password');
    expect(enabled(tester, 'Change the password'), isFalse);
    await type(tester, 'Current password', 'not the password');
    await type(tester, 'Password', 'a new password');
    await type(tester, 'Repeat the password', 'a new password');
    await press(tester, 'Change the password');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(backend.password, testPassword);

    await type(tester, 'Current password', testPassword);
    await press(tester, 'Change the password');
    expect(backend.password, 'a new password');
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(find.text('The password is changed.'), findsOneWidget);
  });

  testWidgets('the recovery phrase is shown again only for the password', (
    tester,
  ) async {
    await openSettings(tester, 'en');
    await openLine(tester, 'Recovery phrase');
    final first = testPhrase.split(' ').first;
    expect(find.text(first), findsNothing);
    await type(tester, 'Password', 'not the password');
    await press(tester, 'Show');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(find.text(first), findsNothing);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Show');
    expect(find.text(first), findsOneWidget);
    expect(find.text(testPhrase.split(' ').last), findsOneWidget);
    expect(
      find.textContaining('Make sure nobody else can see'),
      findsOneWidget,
    );
    // The password is gone from the screen.
    expect(find.text('Password'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('the wallet locks itself when it is not used', (tester) async {
    final backend = await openSettings(tester, 'en');
    await openLine(tester, 'Auto-lock');
    expect(find.text('Selected'), findsOneWidget);
    await openLine(tester, 'After 1 minute');
    expect(backend.settings.value.autoLockMinutes, 1);
    await press(tester, 'Back');
    expect(find.text('After 1 minute'), findsOneWidget);
    await press(tester, 'Back');
    expect(find.text('WALLET'), findsOneWidget);

    // Used in the meantime, it stays open.
    await tester.pump(const Duration(seconds: 50));
    await tester.tap(find.text('WALLET'));
    await tester.pump(const Duration(seconds: 50));
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.state.value, isNotNull);

    // Left alone for the minute, it is closed and asks for the password.
    await tester.pump(const Duration(seconds: 11));
    await tester.pumpAndSettle();
    expect(backend.calls.last, 'close');
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(find.text('WALLET'), findsNothing);
  });

  testWidgets('what the wallet does by itself does not count as use', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    await backend.saveSettings(const AppSettings(autoLockMinutes: 1));
    await tester.pump();
    // The wallet synchronises by itself every so often, and its state
    // changes each time. That must not keep it open for ever.
    for (var round = 0; round < 4; round++) {
      await tester.pump(const Duration(seconds: 13));
      backend.report(
        sync: SyncStatus(
          SyncPhase.done,
          walletHeight: 2000000 + round,
          nodeHeight: 2000000 + round,
        ),
      );
      await tester.pump();
    }
    // Fifty-two seconds in, with a change of state every thirteen.
    expect(backend.state.value, isNotNull);
    await tester.pump(const Duration(seconds: 9));
    await tester.pumpAndSettle();
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
  });

  testWidgets('a synchronisation is not cut short by the auto-lock', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    await backend.saveSettings(const AppSettings(autoLockMinutes: 1));
    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 1000,
        nodeHeight: 2000000,
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(minutes: 3));
    await tester.pump();
    expect(backend.state.value, isNotNull);
    expect(find.text('WALLET'), findsOneWidget);

    // Once it has finished, the wallet locks if it is still left alone.
    backend.report(
      sync: const SyncStatus(
        SyncPhase.done,
        walletHeight: 2000000,
        nodeHeight: 2000000,
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(seconds: 61));
    await tester.pumpAndSettle();
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
  });

  testWidgets('the language is the device\'s until another is chosen', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    tester.view.physicalSize = windowSize;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(VeilApp(backend: backend));
    await tester.pumpAndSettle();
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    await press(tester, 'Settings');
    await openLine(tester, 'Language');
    await openLine(tester, 'Русский');
    expect(backend.settings.value.language, 'ru');
    await press(tester, 'Назад');
    expect(find.text('НАСТРОЙКИ'), findsOneWidget);
    expect(find.text('Русский'), findsOneWidget);

    await openLine(tester, 'Язык');
    await openLine(tester, 'English');
    await press(tester, 'Back');
    expect(find.text('SETTINGS'), findsOneWidget);
  });

  testWidgets('the dark theme is chosen in settings and stays chosen', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    Color ground() =>
        Theme.of(tester.element(find.text('SETTINGS'))).scaffoldBackgroundColor;
    expect(find.text('Light'), findsOneWidget);
    expect(ground(), VeilPalette.light.ground);

    await openLine(tester, 'Theme');
    await openLine(tester, 'Dark');
    expect(backend.settings.value.dark, isTrue);
    await press(tester, 'Back');
    expect(find.text('Dark'), findsOneWidget);
    expect(ground(), VeilPalette.dark.ground);
    // Text is drawn in the dark theme's ink, and nothing overflows.
    final title = tester.widget<Text>(find.text('SETTINGS'));
    expect(title.style!.color, VeilPalette.dark.ink);
    expect(tester.takeException(), isNull);

    await press(tester, 'Back');
    await press(tester, 'Receive');
    expect(tester.takeException(), isNull);
  });

  testWidgets('a refused password is typed over, on every screen that asks', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    final first = testPhrase.split(' ').first;
    backend.takes = const Duration(milliseconds: 300);
    await openLine(tester, 'Recovery phrase');
    await typeOn(tester, 'not the password');
    await pressAndWait(tester, 'Show');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(allMarked(tester), isTrue);
    await typeOn(tester, testPassword);
    await pressAndWait(tester, 'Show');
    expect(find.text(first), findsOneWidget);
    await press(tester, 'Back');

    await openLine(tester, 'Remove wallet');
    await type(tester, 'Password', 'not the password');
    await pressAndWait(tester, 'Remove the wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(allMarked(tester), isTrue);
    await typeOn(tester, testPassword);
    await pressAndWait(tester, 'Remove the wallet');
    expect(backend.hasWallet, isFalse);
  });

  testWidgets('a wallet too busy to be asked is not a wrong password', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    backend.busy = true;

    await openLine(tester, 'Recovery phrase');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Show');
    expect(find.text('Wrong password.'), findsNothing);
    expect(find.textContaining('Not now'), findsOneWidget);
    await press(tester, 'Back');

    await openLine(tester, 'Remove wallet');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Remove the wallet');
    expect(find.text('Wrong password.'), findsNothing);
    expect(find.textContaining('Not now'), findsOneWidget);
    expect(backend.hasWallet, isTrue);
  });

  testWidgets('a failure nobody foresaw leaves no screen waiting for ever', (
    tester,
  ) async {
    final backend = await openSettings(tester, 'en');
    final failure = ArgumentError('something that cannot happen');
    final said = find.textContaining('something that cannot happen');

    await openLine(tester, 'Recovery phrase');
    backend.breakNext = failure;
    await type(tester, 'Password', testPassword);
    await press(tester, 'Show');
    expect(said, findsOneWidget);
    expect(find.text('Wrong password.'), findsNothing);
    expect(enabled(tester, 'Show'), isTrue);
    await press(tester, 'Back');

    await openLine(tester, 'Password');
    backend.breakNext = failure;
    await type(tester, 'Current password', testPassword);
    await type(tester, 'Password', 'a new password');
    await type(tester, 'Repeat the password', 'a new password');
    await press(tester, 'Change the password');
    expect(said, findsOneWidget);
    expect(backend.password, testPassword);
    expect(enabled(tester, 'Change the password'), isTrue);
    await press(tester, 'Back');

    await openLine(tester, 'Remove wallet');
    backend.breakNext = failure;
    await type(tester, 'Password', testPassword);
    await press(tester, 'Remove the wallet');
    expect(said, findsOneWidget);
    expect(backend.hasWallet, isTrue);
    expect(enabled(tester, 'Remove the wallet'), isTrue);
  });

  testWidgets('the wallet is removed only for the password', (tester) async {
    final backend = await openSettings(tester, 'en');
    await openLine(tester, 'Remove wallet');
    expect(
      find.textContaining('Without the phrase they are lost for good'),
      findsOneWidget,
    );
    expect(enabled(tester, 'Remove the wallet'), isFalse);
    await type(tester, 'Password', 'not the password');
    await press(tester, 'Remove the wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(backend.calls, isNot(contains('remove wallet')));
    expect(backend.hasWallet, isTrue);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Remove the wallet');
    expect(backend.calls.last, 'remove wallet');
    expect(backend.hasWallet, isFalse);
    // The device is as before it had a wallet, with no way back.
    expect(find.text('CREATE WALLET'), findsOneWidget);
    expect(find.text('BACK'), findsNothing);
  });

  testWidgets('settings fit the window in Russian too', (tester) async {
    final backend = await openSettings(tester, 'ru');
    expect(find.text('НАСТРОЙКИ'), findsOneWidget);
    expect(find.text('Общедоступные узлы'), findsOneWidget);
    expect(find.text('Через 5 минут'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await backend.saveSettings(
      const AppSettings(bridges: [bridge, bridge], autoLockMinutes: 1),
    );
    await tester.pumpAndSettle();
    expect(find.text('2 моста'), findsOneWidget);
    expect(find.text('Через 1 минуту'), findsOneWidget);

    for (final line in ['Узел', 'Мосты Tor', 'Пароль', 'Удаление кошелька']) {
      await openLine(tester, line);
      expect(tester.takeException(), isNull, reason: line);
      await press(tester, 'Назад');
    }
    await openLine(tester, 'Автоблокировка');
    expect(find.text('Через 15 минут'), findsOneWidget);
    expect(find.text('Через 60 минут'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  test('settings survive being written and read, and a damaged file', () {
    const settings = AppSettings(
      language: 'ru',
      dark: true,
      autoLockMinutes: 15,
      bridges: [bridge],
    );
    final read = AppSettings.fromJson(settings.toJson());
    expect(read.language, 'ru');
    expect(read.dark, isTrue);
    expect(read.autoLockMinutes, 15);
    expect(read.bridges, [bridge]);

    // What is not what it should be falls back to the default.
    for (final damaged in <Object?>[
      null,
      'text',
      <String, dynamic>{},
      <String, dynamic>{
        'language': 'xx',
        'dark': 'yes',
        'auto_lock_minutes': 0,
        'bridges': 'obfs4',
      },
    ]) {
      final read = AppSettings.fromJson(damaged);
      expect(read.language, isNull);
      expect(read.dark, isFalse);
      // There is no way to switch the auto-lock off.
      expect(read.autoLockMinutes, AppSettings.defaultAutoLock);
      expect(read.bridges, isEmpty);
    }
    expect(const AppSettings().copyWith(language: () => 'en').language, 'en');
    expect(settings.copyWith(language: () => null).language, isNull);
    expect(settings.copyWith(dark: false).language, 'ru');
  });

  test('typed nodes and bridges are split into lines', () {
    expect(bridgeLines(' a b \n\n# c\n d'), ['a b', 'd']);
    expect(bridgeLines(''), isEmpty);
    expect(nodeLines(' a.onion:1 \n b.onion:2, c.onion:3\n'), [
      'a.onion:1',
      'b.onion:2',
      'c.onion:3',
    ]);
    expect(nodeLines(' \n'), isEmpty);
  });
}
