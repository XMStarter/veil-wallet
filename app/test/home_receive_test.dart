// Home and Receive, on a wallet that only pretends.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/screens/receive_screen.dart';
import 'package:veil/widgets/address_text.dart';
import 'package:veil/widgets/qr_view.dart';
import 'package:veil/core/backend.dart';

import 'support.dart';

void main() {
  setUpAll(loadFonts);

  testWidgets('home says in words what Tor and the synchronisation do', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    expect(find.text('WALLET'), findsOneWidget);
    expect(find.text('Tor is connecting: 15%'), findsOneWidget);
    expect(
      find.text('Synchronisation starts when Tor is connected'),
      findsOneWidget,
    );

    backend.tor(TorState.ready);
    await tester.pumpAndSettle();
    expect(find.text('Tor is connected'), findsOneWidget);
    expect(find.text('Not synchronised yet'), findsOneWidget);

    backend.report(sync: const SyncStatus(SyncPhase.running));
    await tester.pumpAndSettle();
    expect(find.text('Synchronising'), findsOneWidget);

    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 2250000,
        nodeHeight: 2256644,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Synchronising, 6644 blocks left'), findsOneWidget);
    // Progress is a line that fills, counted from where this
    // synchronisation was first seen.
    LinearProgressIndicator line() =>
        tester.widget(find.byType(LinearProgressIndicator));
    expect(line().value, 0);
    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 2253322,
        nodeHeight: 2256644,
      ),
    );
    await tester.pumpAndSettle();
    expect(line().value, closeTo(0.5, 0.001));
    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 2256643,
        nodeHeight: 2256644,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Synchronising, 1 block left'), findsOneWidget);

    backend.report(
      sync: const SyncStatus(
        SyncPhase.done,
        walletHeight: 2256644,
        nodeHeight: 2256644,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Synchronised'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsNothing);

    // A synchronisation that failed says why, and that it is tried again.
    backend.report(
      sync: const SyncStatus(
        SyncPhase.failed,
        problem: Problem(ProblemKind.noNode, 'none of the 4 nodes answered'),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.text(
        'Could not synchronise. No node answered. '
        'The wallet tries again by itself.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('home shows the balance as the core hands it over', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    expect(find.text('0.00 XMR'), findsOneWidget);
    // Until a synchronisation has finished, the amounts may be behind,
    // and the screen says so.
    expect(
      find.text('As of the last finished synchronisation.'),
      findsOneWidget,
    );

    backend.report(
      tor: const TorStatus(TorState.ready, 100, null),
      sync: const SyncStatus(
        SyncPhase.done,
        walletHeight: 2256644,
        nodeHeight: 2256644,
      ),
      total: '12.345678901234',
      unlocked: '10.50',
      locked: '1.845678901234',
    );
    await tester.pumpAndSettle();
    // To the last digit: the screen does not round money.
    expect(find.text('12.345678901234 XMR'), findsOneWidget);
    expect(find.text('Available: 10.50 XMR'), findsOneWidget);
    expect(find.text('Pending: 1.845678901234 XMR'), findsOneWidget);
    expect(find.text('As of the last finished synchronisation.'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a tap hides every amount, and another shows them again', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    backend.report(
      total: '12.50',
      unlocked: '12.50',
      history: [transfer(direction: Direction.incoming, amount: '12.50')],
    );
    await tester.pumpAndSettle();
    expect(find.text('12.50 XMR'), findsOneWidget);
    expect(find.text('+12.50'), findsOneWidget);

    await tester.tap(find.text('12.50 XMR'));
    await tester.pumpAndSettle();
    expect(find.text('Balance hidden'), findsOneWidget);
    expect(shown('12.50'), findsNothing);
    // What happened stays readable; how much does not.
    expect(find.text('Received'), findsOneWidget);

    await tester.tap(find.text('Balance hidden'));
    await tester.pumpAndSettle();
    expect(find.text('12.50 XMR'), findsOneWidget);
    expect(find.text('+12.50'), findsOneWidget);
  });

  testWidgets('recent activity lists payments, newest first, in words', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    expect(find.text('RECENT ACTIVITY'), findsOneWidget);
    expect(find.text('No payments yet.'), findsOneWidget);

    // Oldest first, as the core keeps them.
    backend.report(
      history: [
        transfer(direction: Direction.incoming, amount: '12.50'),
        transfer(direction: Direction.outgoing, amount: '0.15'),
        transfer(
          direction: Direction.incoming,
          amount: '3.00',
          confirmations: 4,
        ),
        transfer(
          direction: Direction.outgoing,
          amount: '7.00',
          confirmations: 0,
          failed: true,
        ),
        transfer(
          direction: Direction.incoming,
          amount: '1.25',
          confirmations: 0,
          pending: true,
        ),
      ],
    );
    await tester.pumpAndSettle();
    expect(find.text('No payments yet.'), findsNothing);
    // A sign and a label, not colour alone, tell the two ways apart.
    expect(find.text('+12.50'), findsOneWidget);
    expect(find.text('−0.15'), findsOneWidget);
    expect(find.text('Received'), findsNWidgets(3));
    expect(find.text('Sent'), findsNWidgets(2));
    expect(find.text('Not confirmed yet'), findsOneWidget);
    expect(find.text('4 of 10 confirmations'), findsOneWidget);
    expect(find.text('Failed'), findsOneWidget);

    double top(String amount) => tester.getTopLeft(find.text(amount)).dy;
    expect(top('+1.25'), lessThan(top('−7.00')));
    expect(top('−7.00'), lessThan(top('+3.00')));
    expect(top('+3.00'), lessThan(top('−0.15')));
    expect(top('−0.15'), lessThan(top('+12.50')));
    expect(tester.takeException(), isNull);
  });

  testWidgets('receive shows the address as a code and as text to check', (
    tester,
  ) async {
    final copied = clipboard(tester);

    await openWallet(tester, 'en');
    await press(tester, 'Receive');
    expect(find.text('RECEIVE'), findsOneWidget);
    expect(find.text('ADDRESS 1'), findsOneWidget);

    // The code holds a payment link to this address, nothing else.
    final address = testAddresses[0];
    expect(tester.widget<QrView>(find.byType(QrView)).data, 'monero:$address');
    // The text is the address in groups of four.
    expect(shown('73a4 nWuv kYoY'), findsOneWidget);
    expect(shown('r7X9 QrPu bkn'), findsOneWidget);
    expect(
      tester.widget<AddressText>(find.byType(AddressText)).address,
      address,
    );

    // Copied whole, without the gaps.
    expect(find.text('The address is in the clipboard.'), findsNothing);
    await press(tester, 'Copy address');
    expect(copied, [address]);
    expect(find.text('The address is in the clipboard.'), findsOneWidget);

    await press(tester, 'Back');
    expect(find.text('WALLET'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('another address can be asked for, up to a limit', (
    tester,
  ) async {
    clipboard(tester);
    final backend = await openWallet(tester, 'en');
    await press(tester, 'Receive');
    await press(tester, 'Copy address');
    expect(find.text('The address is in the clipboard.'), findsOneWidget);

    await press(tester, 'New address');
    expect(backend.calls.last, 'new address');
    expect(find.text('ADDRESS 2'), findsOneWidget);
    expect(
      tester.widget<QrView>(find.byType(QrView)).data,
      'monero:${testAddresses[1]}',
    );
    // What is in the clipboard is no longer the address on the screen.
    expect(find.text('The address is in the clipboard.'), findsNothing);

    await press(tester, 'New address');
    expect(find.text('ADDRESS 3'), findsOneWidget);
    // One more would be one too many unpaid in a row; the wallet says why
    // it gives none, and the address stays.
    await press(tester, 'New address');
    expect(find.text('ADDRESS 3'), findsOneWidget);
    expect(
      find.textContaining('Several addresses in a row have not been paid'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('once the address has been paid, the next one is shown', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    await press(tester, 'Receive');
    expect(find.text('ADDRESS 1'), findsOneWidget);

    // The core moves on after a synchronisation that found the payment.
    backend.report(
      address: ReceiveAddress(2, testAddresses[1]),
      total: '5.00',
      locked: '5.00',
      history: [
        transfer(
          direction: Direction.incoming,
          amount: '5.00',
          confirmations: 1,
        ),
      ],
    );
    await tester.pumpAndSettle();
    expect(find.text('ADDRESS 2'), findsOneWidget);
    expect(
      tester.widget<QrView>(find.byType(QrView)).data,
      'monero:${testAddresses[1]}',
    );
    // A refusal that had nothing to do with this screen is not shown on
    // it.
    backend.report(
      lastProblem: const Problem(
        ProblemKind.tooManyUnusedAddresses,
        'too many unused addresses',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('Several addresses in a row'), findsNothing);
  });

  testWidgets('a clipboard that does not take the address is not hidden', (
    tester,
  ) async {
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      // Only the clipboard refuses; the rest of the system is as ever.
      if (call.method == 'Clipboard.setData') {
        throw PlatformException(code: 'unavailable');
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );
    await openWallet(tester, 'en');
    await press(tester, 'Receive');
    await press(tester, 'Copy address');
    expect(find.text('The address is in the clipboard.'), findsNothing);
    expect(find.text('The address could not be copied.'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('an address is cut into groups of four, the last as it comes', (
    tester,
  ) async {
    expect(AddressText.groups('abcdefgh'), ['abcd', 'efgh']);
    expect(AddressText.groups('abcdefghij'), ['abcd', 'efgh', 'ij']);
    expect(AddressText.groups(''), isEmpty);
    final groups = AddressText.groups(testAddresses[0]);
    expect(groups, hasLength(24));
    expect(groups.join(), testAddresses[0]);
    expect(ReceiveScreen.link('abc'), 'monero:abc');
  });

  testWidgets('home and receive fit the window in Russian too', (tester) async {
    clipboard(tester);
    final backend = await openWallet(tester, 'ru');
    expect(find.text('КОШЕЛЁК'), findsOneWidget);
    backend.report(
      tor: const TorStatus(TorState.ready, 100, null),
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 2256642,
        nodeHeight: 2256644,
      ),
      total: '1234567.123456789012',
      unlocked: '1234566.123456789012',
      locked: '1.00',
      history: [
        transfer(direction: Direction.incoming, amount: '1234567.123456789012'),
        transfer(
          direction: Direction.outgoing,
          amount: '0.15',
          confirmations: 3,
        ),
      ],
    );
    await tester.pumpAndSettle();
    expect(find.text('Tor подключён'), findsOneWidget);
    // Russian counts blocks in its own three ways.
    expect(find.text('Синхронизация, осталось 2 блока'), findsOneWidget);
    expect(find.text('Доступно: 1234566.123456789012 XMR'), findsOneWidget);
    expect(find.text('3 из 10 подтверждений'), findsOneWidget);
    for (final (left, words) in [
      (1, 'Синхронизация, остался 1 блок'),
      (5, 'Синхронизация, осталось 5 блоков'),
      (21, 'Синхронизация, остался 21 блок'),
    ]) {
      backend.report(
        sync: SyncStatus(
          SyncPhase.running,
          walletHeight: 2256644 - left,
          nodeHeight: 2256644,
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text(words), findsOneWidget);
    }
    expect(tester.takeException(), isNull);

    await press(tester, 'Получить');
    expect(find.text('ПОЛУЧИТЬ'), findsOneWidget);
    expect(find.text('АДРЕС 1'), findsOneWidget);
    await press(tester, 'Скопировать адрес');
    expect(find.text('Адрес в буфере обмена.'), findsOneWidget);
    await press(tester, 'Новый адрес');
    await press(tester, 'Новый адрес');
    await press(tester, 'Новый адрес');
    expect(find.textContaining('Несколько адресов подряд'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await press(tester, 'Назад');
    await press(tester, 'Заблокировать кошелёк');
    expect(find.text('ОТКРЫТЬ КОШЕЛЁК'), findsOneWidget);
  });
}
