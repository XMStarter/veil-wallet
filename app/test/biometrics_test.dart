// Opening the wallet with a fingerprint or a face, on a wallet and a
// phone that only pretend.

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';

import 'support.dart';

/// Opens the wallet with its password and goes to Settings.
Future<FakeBackend> openSettingsOn(
  WidgetTester tester, {
  Biometrics phone = Biometrics.off,
  String language = 'en',
}) async {
  final backend = FakeBackend(hasWallet: true, confirmed: true);
  backend.device.biometricsNow = phone;
  await pumpApp(tester, language, backend: backend);
  await type(tester, language == 'ru' ? 'Пароль' : 'Password', testPassword);
  await press(tester, language == 'ru' ? 'Открыть кошелёк' : 'Open wallet');
  await press(tester, language == 'ru' ? 'Настройки' : 'Settings');
  return backend;
}

Future<void> openLine(WidgetTester tester, String title) async {
  await tester.ensureVisible(find.text(title));
  await tester.tap(find.text(title));
  await tester.pumpAndSettle();
}

/// Switches it on in Settings, locks the wallet, and stands on the screen
/// that asks for the password.
Future<FakeBackend> lockedWithBiometrics(WidgetTester tester) async {
  final backend = await openSettingsOn(tester);
  await openLine(tester, 'Fingerprint or face');
  await type(tester, 'Password', testPassword);
  await press(tester, 'Switch on');
  await press(tester, 'Back');
  await press(tester, 'Lock wallet');
  expect(find.text('OPEN WALLET'), findsOneWidget);
  return backend;
}

void main() {
  setUpAll(loadFonts);

  testWidgets('a device that cannot do it shows no line for it', (
    tester,
  ) async {
    await openSettingsOn(tester, phone: Biometrics.absent);
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(find.text('Fingerprint or face'), findsNothing);
  });

  testWidgets('a phone without a fingerprint or a face says what is missing', (
    tester,
  ) async {
    final backend = await openSettingsOn(tester, phone: Biometrics.notSetUp);
    expect(find.text('None set up on this phone'), findsOneWidget);
    await openLine(tester, 'Fingerprint or face');
    expect(find.textContaining('No fingerprint or face is set up'), findsOne);
    // Nothing to switch on, and no password asked for nothing.
    expect(find.text('SWITCH ON'), findsNothing);
    expect(find.text('Password'), findsNothing);
    expect(backend.device.asked, isEmpty);
  });

  testWidgets('it is switched on for the wallet\'s password and a fingerprint, '
      'and says what it means first', (tester) async {
    final backend = await openSettingsOn(tester);
    final device = backend.device;
    expect(find.text('Off'), findsOneWidget);
    await openLine(tester, 'Fingerprint or face');
    expect(
      find.textContaining('Whoever the phone recognises can then open'),
      findsOneWidget,
    );
    expect(enabled(tester, 'Switch on'), isFalse);

    // Another password is not given into the phone's keeping, and the
    // phone is not even asked.
    await type(tester, 'Password', 'not the password');
    await press(tester, 'Switch on');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(device.asked, isEmpty);
    expect(device.keptPassword, isNull);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Switch on');
    expect(device.asked, ['Let this phone open the Veil wallet']);
    expect(device.keptPassword, testPassword);
    expect(find.text('SETTINGS'), findsOneWidget);
    expect(
      find.text('The wallet now opens with a fingerprint or a face.'),
      findsOneWidget,
    );
    expect(find.text('Opens the wallet'), findsOneWidget);
  });

  testWidgets('giving up at the phone\'s question switches nothing on', (
    tester,
  ) async {
    final backend = await openSettingsOn(tester);
    final device = backend.device;
    await openLine(tester, 'Fingerprint or face');
    await type(tester, 'Password', testPassword);
    device.nextOutcome = BiometricOutcome.cancelled;
    await press(tester, 'Switch on');
    expect(device.keptPassword, isNull);
    // Still on its screen, with nothing called a failure.
    expect(find.text('SWITCH ON'), findsOneWidget);
    expect(find.textContaining('could not'), findsNothing);

    // A phone that cannot do it now says so, with its reason.
    device.nextOutcome = BiometricOutcome.failed;
    await press(tester, 'Switch on');
    expect(device.keptPassword, isNull);
    expect(
      find.text('The phone could not do that. Details: the reader is busy'),
      findsOneWidget,
    );
  });

  testWidgets('a wallet too busy to say whether the password is its own '
      'gives nothing into the phone\'s keeping', (tester) async {
    final backend = await openSettingsOn(tester);
    await openLine(tester, 'Fingerprint or face');
    await type(tester, 'Password', testPassword);
    backend.busy = true;
    await press(tester, 'Switch on');
    expect(backend.device.asked, isEmpty);
    expect(backend.device.keptPassword, isNull);
    expect(find.text('Wrong password.'), findsNothing);
  });

  testWidgets('a locked wallet opens for a fingerprint, and still for its '
      'password', (tester) async {
    final backend = await lockedWithBiometrics(tester);
    final device = backend.device;
    expect(backend.state.value, isNull);

    await press(tester, 'Open with fingerprint or face');
    expect(device.asked.last, 'Open the Veil wallet');
    expect(backend.state.value, isNotNull);
    expect(find.text('WALLET'), findsOneWidget);

    // The password opens it as before.
    await press(tester, 'Lock wallet');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('without it, the screen that asks for the password offers no '
      'fingerprint', (tester) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    backend.device.biometricsNow = Biometrics.off;
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(find.text('OPEN WITH FINGERPRINT OR FACE'), findsNothing);
  });

  testWidgets('giving up at the question leaves the wallet closed and says '
      'nothing', (tester) async {
    final backend = await lockedWithBiometrics(tester);
    backend.device.nextOutcome = BiometricOutcome.cancelled;
    await press(tester, 'Open with fingerprint or face');
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(find.textContaining('could not'), findsNothing);
    expect(find.text('OPEN WITH FINGERPRINT OR FACE'), findsOneWidget);
  });

  testWidgets('new fingerprints on the phone end it, and the password is '
      'asked for', (tester) async {
    final backend = await lockedWithBiometrics(tester);
    backend.device.nextOutcome = BiometricOutcome.changed;
    await press(tester, 'Open with fingerprint or face');
    expect(backend.state.value, isNull);
    expect(
      find.textContaining('The fingerprints or faces on this phone have'),
      findsOneWidget,
    );
    expect(find.text('OPEN WITH FINGERPRINT OR FACE'), findsNothing);
    expect(backend.device.keptPassword, isNull);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('a password the phone kept and the wallet no longer takes is '
      'forgotten, not tried for ever', (tester) async {
    final backend = await lockedWithBiometrics(tester);
    // Changed where the phone did not hear of it.
    backend.password = 'another password';
    await press(tester, 'Open with fingerprint or face');
    expect(backend.state.value, isNull);
    expect(backend.device.keptPassword, isNull);
    expect(
      find.textContaining('The password the phone kept is not'),
      findsOneWidget,
    );
    expect(find.text('Wrong password.'), findsNothing);
    expect(find.text('OPEN WITH FINGERPRINT OR FACE'), findsNothing);
  });

  testWidgets('a new password switches it off and says so', (tester) async {
    final backend = await openSettingsOn(tester);
    await openLine(tester, 'Fingerprint or face');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Switch on');
    expect(backend.device.keptPassword, testPassword);

    await openLine(tester, 'Password');
    await type(tester, 'Current password', testPassword);
    await type(tester, 'Password', 'a better password');
    await type(tester, 'Repeat the password', 'a better password');
    await press(tester, 'Change the password');
    expect(backend.password, 'a better password');
    expect(backend.device.keptPassword, isNull);
    expect(
      find.textContaining('Opening with a fingerprint or a face is switched'),
      findsOneWidget,
    );
    expect(find.text('Off'), findsOneWidget);
  });

  testWidgets('a new password with nothing kept says only that', (
    tester,
  ) async {
    final backend = await openSettingsOn(tester);
    await openLine(tester, 'Password');
    await type(tester, 'Current password', testPassword);
    await type(tester, 'Password', 'a better password');
    await type(tester, 'Repeat the password', 'a better password');
    await press(tester, 'Change the password');
    expect(backend.password, 'a better password');
    expect(find.text('The password is changed.'), findsOneWidget);
  });

  testWidgets('it is switched off without the password', (tester) async {
    final backend = await openSettingsOn(tester);
    await openLine(tester, 'Fingerprint or face');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Switch on');

    await openLine(tester, 'Fingerprint or face');
    expect(find.textContaining('Switching this off takes the'), findsOne);
    expect(find.text('Password'), findsNothing);
    await press(tester, 'Switch off');
    expect(backend.device.keptPassword, isNull);
    expect(
      find.text('The wallet now opens with the password only.'),
      findsOneWidget,
    );
    expect(find.text('Off'), findsOneWidget);
  });

  testWidgets('a removed wallet leaves no password in the phone\'s keeping', (
    tester,
  ) async {
    final backend = await openSettingsOn(tester);
    await openLine(tester, 'Fingerprint or face');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Switch on');

    await openLine(tester, 'Remove wallet');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Remove the wallet');
    expect(backend.hasWallet, isFalse);
    expect(backend.device.keptPassword, isNull);
    expect(backend.device.biometricsNow, Biometrics.off);
  });

  testWidgets('its screens fit the window in Russian too', (tester) async {
    final backend = await openSettingsOn(tester, language: 'ru');
    expect(find.text('Выключено'), findsOneWidget);
    await openLine(tester, 'Отпечаток или лицо');
    expect(tester.takeException(), isNull);
    await type(tester, 'Пароль', testPassword);
    await press(tester, 'Включить');
    expect(backend.device.asked, ['Разрешить телефону открывать кошелёк Veil']);
    expect(find.text('Открывает кошелёк'), findsOneWidget);
    await press(tester, 'Назад');
    await press(tester, 'Заблокировать кошелёк');
    expect(find.text('ОТКРЫТЬ ОТПЕЧАТКОМ ИЛИ ЛИЦОМ'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await press(tester, 'Открыть отпечатком или лицом');
    expect(backend.state.value, isNotNull);
  });
}
