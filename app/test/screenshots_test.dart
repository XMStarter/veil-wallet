// Not a check: draws the screens with the app's own fonts and writes them
// to build/screenshots/ as PNG files, to look at without running the app.

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:veil/core/backend.dart';

import 'support.dart';

Future<void> capture(WidgetTester tester, String name) async {
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byType(RepaintBoundary).first,
  );
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 2);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    final file = File('build/screenshots/$name.png')
      ..createSync(recursive: true);
    file.writeAsBytesSync(bytes!.buffer.asUint8List());
  });
}

/// The texts the walk below presses and types by, per language.
const labels = {
  'en': (
    create: 'Create wallet',
    password: 'Password',
    repeat: 'Repeat the password',
    written: 'I wrote them down',
    restore: 'Restore from recovery phrase',
    phrase: 'Recovery phrase',
    open: 'Open wallet',
    receive: 'Receive',
    back: 'Back',
    send: 'Send',
    payee: 'Address to pay to',
    everything: 'Send everything',
    next: 'Continue',
    pay: 'Send payment',
  ),
  'ru': (
    create: 'Создать кошелёк',
    password: 'Пароль',
    repeat: 'Пароль ещё раз',
    written: 'Слова записаны',
    restore: 'Восстановить по фразе',
    phrase: 'Фраза восстановления',
    open: 'Открыть кошелёк',
    receive: 'Получить',
    back: 'Назад',
    send: 'Отправить',
    payee: 'Адрес получателя',
    everything: 'Отправить всё',
    next: 'Продолжить',
    pay: 'Отправить платёж',
  ),
};

void main() {
  setUpAll(loadFonts);

  for (final language in ['en', 'ru']) {
    final label = labels[language]!;

    testWidgets('the way to a new wallet, drawn ($language)', (tester) async {
      await pumpApp(tester, language);
      // The logo is decoded off the test's fake clock.
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
      await tester.pumpAndSettle();
      await capture(tester, '1-start-$language');

      await press(tester, label.create);
      await type(tester, label.password, testPassword);
      await type(tester, label.repeat, testPassword);
      await capture(tester, '2-create-$language');

      await press(tester, label.create);
      await capture(tester, '3-phrase-$language');

      await press(tester, label.written);
      await capture(tester, '4-check-$language');
    });

    testWidgets('restoring, drawn ($language)', (tester) async {
      await pumpApp(tester, language);
      await press(tester, label.restore);
      await type(tester, label.phrase, testPhrase);
      await capture(tester, '5-restore-$language');
    });

    testWidgets('unlocking, drawn ($language)', (tester) async {
      await pumpApp(
        tester,
        language,
        backend: FakeBackend(hasWallet: true, confirmed: true),
      );
      await capture(tester, '6-unlock-$language');
    });

    testWidgets('home and receive, drawn ($language)', (tester) async {
      final backend = FakeBackend(hasWallet: true, confirmed: true);
      await pumpApp(tester, language, backend: backend);
      await type(tester, label.password, testPassword);
      await press(tester, label.open);
      await capture(tester, '7-home-new-$language');

      backend.report(
        tor: const TorStatus(TorState.ready, 100, null),
        sync: const SyncStatus(
          SyncPhase.running,
          walletHeight: 2250000,
          nodeHeight: 2256644,
        ),
        total: '12.345678901234',
        unlocked: '10.50',
        locked: '1.845678901234',
        history: [
          transfer(direction: Direction.incoming, amount: '12.50'),
          transfer(direction: Direction.outgoing, amount: '0.15'),
          transfer(
            direction: Direction.incoming,
            amount: '1.845678901234',
            confirmations: 4,
          ),
        ],
      );
      await tester.pumpAndSettle();
      backend.report(
        sync: const SyncStatus(
          SyncPhase.running,
          walletHeight: 2254000,
          nodeHeight: 2256644,
        ),
      );
      await tester.pumpAndSettle();
      await capture(tester, '8-home-$language');

      await press(tester, label.receive);
      await capture(tester, '9-receive-$language');
      await press(tester, label.back);

      await press(tester, label.send);
      await type(tester, label.payee, testAddresses[1]);
      await capture(tester, '10-send-$language');
      await press(tester, label.everything);
      await press(tester, label.next);
      await capture(tester, '11-review-$language');
      backend.sendEndsAs = SendPhase.outcomeUnknown;
      await press(tester, label.pay);
      await capture(tester, '12-unknown-$language');
    });
  }
}
