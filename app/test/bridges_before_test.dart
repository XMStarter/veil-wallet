// Bridges before a wallet is open: on the first screens, which start Tor,
// and after a settings file that could not be read.

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';

import 'support.dart';

const bridge = '192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956';

/// The calls that started a wallet, and with it Tor.
Iterable<String> starts(FakeBackend backend) =>
    backend.calls.where((call) => call.startsWith('start'));

void main() {
  setUpAll(loadFonts);

  testWidgets('settings that could not be read are said before a password '
      'is asked for, and nothing is opened meanwhile', (tester) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true)
      ..settingsAre(const AppSettings(lost: true));
    await pumpApp(tester, 'en', backend: backend);

    expect(find.text('SETTINGS LOST'), findsOneWidget);
    expect(find.textContaining('could not be read'), findsOneWidget);
    expect(find.textContaining('connected to directly'), findsOneWidget);
    // No way to the wallet from here.
    expect(find.text('Password'), findsNothing);
    expect(find.text('OPEN WALLET'), findsNothing);

    await press(tester, 'Go on without bridges');
    expect(find.text('SETTINGS LOST'), findsNothing);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    // Said once: the choice is written down.
    expect(backend.calls, contains('save settings'));
    expect(backend.settings.value.lost, isFalse);
    expect(backend.settings.value.bridges, isEmpty);
    expect(starts(backend), isEmpty);
  });

  testWidgets('lost bridges are set again before anything is opened', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true)
      ..settingsAre(const AppSettings(lost: true));
    await pumpApp(tester, 'en', backend: backend);

    await press(tester, 'Set bridges');
    expect(find.text('TOR BRIDGES'), findsOneWidget);
    // Going back without saying anything leaves the question open.
    await press(tester, 'Back');
    expect(find.text('SETTINGS LOST'), findsOneWidget);
    expect(backend.settings.value.lost, isTrue);

    await press(tester, 'Set bridges');
    await type(tester, 'Bridge lines, one per line', bridge);
    await press(tester, 'Save');
    expect(find.text('SETTINGS LOST'), findsNothing);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(find.text('TOR BRIDGES: 1 BRIDGE'), findsOneWidget);
    expect(backend.settings.value.bridges, [bridge]);
    expect(backend.settings.value.lost, isFalse);
    expect(starts(backend), isEmpty);

    // And the wallet then opens through them.
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(starts(backend), ['start open']);
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('a device without a wallet can be given bridges first', (
    tester,
  ) async {
    final backend = FakeBackend();
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text('CREATE WALLET'), findsOneWidget);

    await press(tester, 'Tor bridges: None');
    expect(find.text('TOR BRIDGES'), findsOneWidget);
    // What is no bridge is refused here as in Settings, and not kept.
    await type(tester, 'Bridge lines, one per line', 'not a bridge');
    await press(tester, 'Save');
    expect(find.text('TOR BRIDGES'), findsOneWidget);
    expect(backend.settings.value.bridges, isEmpty);

    await type(tester, 'Bridge lines, one per line', '$bridge\n# a note\n');
    await press(tester, 'Save');
    expect(find.text('CREATE WALLET'), findsOneWidget);
    expect(find.text('TOR BRIDGES: 1 BRIDGE'), findsOneWidget);
    expect(backend.settings.value.bridges, [bridge]);
    // Nothing was started for it.
    expect(starts(backend), isEmpty);
  });

  testWidgets('the screen that opens a wallet leads to the bridges too', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true)
      ..settingsAre(const AppSettings(bridges: [bridge]));
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text('OPEN WALLET'), findsOneWidget);

    await press(tester, 'Tor bridges: 1 bridge');
    await type(tester, 'Bridge lines, one per line', '');
    await press(tester, 'Save');
    expect(find.text('TOR BRIDGES: NONE'), findsOneWidget);
    expect(backend.settings.value.bridges, isEmpty);
    expect(starts(backend), isEmpty);
  });

  testWidgets('a device without a wallet is asked the same, and goes on to '
      'its first screen', (tester) async {
    final backend = FakeBackend()..settingsAre(const AppSettings(lost: true));
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text('SETTINGS LOST'), findsOneWidget);
    expect(find.text('CREATE WALLET'), findsNothing);
    await press(tester, 'Go on without bridges');
    expect(find.text('CREATE WALLET'), findsOneWidget);
  });

  testWidgets('its screens fit the window in Russian too', (tester) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true)
      ..settingsAre(const AppSettings(lost: true));
    await pumpApp(tester, 'ru', backend: backend);
    expect(find.text('НАСТРОЙКИ ПОТЕРЯНЫ'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await press(tester, 'Задать мосты');
    await type(tester, 'Строки мостов, по одной в строке', bridge);
    await press(tester, 'Сохранить');
    expect(find.text('ОТКРЫТЬ КОШЕЛЁК'), findsOneWidget);
    expect(find.text('МОСТЫ TOR: 1 МОСТ'), findsOneWidget);
    expect(tester.takeException(), isNull);

    final fresh = FakeBackend();
    await pumpApp(tester, 'ru', backend: fresh);
    expect(find.text('МОСТЫ TOR: НЕТ'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
