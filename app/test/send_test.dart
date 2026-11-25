// Sending, on a wallet that only pretends.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/payment.dart';
import 'package:veil/widgets/address_text.dart';

import 'support.dart';

/// Opens the wallet with funds in it and goes to the Send form.
Future<FakeBackend> openSend(WidgetTester tester, String language) async {
  final backend = await openWallet(tester, language);
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
  await press(tester, language == 'ru' ? 'Отправить' : 'Send');
  return backend;
}

/// What the field labelled [label] holds.
String fieldText(WidgetTester tester, String label) {
  final field = find.descendant(
    of: find
        .ancestor(of: find.text(label), matching: find.byType(Column))
        .first,
    matching: find.byType(TextField),
  );
  return tester.widget<TextField>(field.first).controller!.text;
}

void main() {
  setUpAll(loadFonts);

  final payee = testAddresses[1];

  testWidgets('a payment goes from the form through a review to sent', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    expect(find.text('SEND'), findsOneWidget);
    expect(find.text('Available: 10.50 XMR'), findsOneWidget);
    expect(enabled(tester, 'Continue'), isFalse);

    // Typed the way this app shows addresses, in groups: the gaps go.
    await type(
      tester,
      'Address to pay to',
      ' ${AddressText.groups(payee).join(' ')} ',
    );
    expect(enabled(tester, 'Continue'), isFalse);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    expect(backend.calls.last, 'prepare 1.5 to $payee');

    // The review: where to, how much, the fee, and what leaves in all.
    expect(find.text('CONFIRM'), findsOneWidget);
    expect(tester.widget<AddressText>(find.byType(AddressText)).address, payee);
    expect(find.text('1.50 XMR'), findsOneWidget);
    expect(find.text('0.00003066 XMR'), findsOneWidget);
    expect(find.text('1.50003066 XMR'), findsOneWidget);
    expect(find.textContaining('Nothing has been sent yet'), findsOneWidget);
    expect(find.textContaining('of your addresses together'), findsNothing);
    // Up to here nothing was sent.
    expect(backend.calls, isNot(contains('confirm send')));

    await press(tester, 'Send payment');
    expect(backend.calls.last, 'confirm send');
    expect(find.text('SENT'), findsOneWidget);
    expect(find.textContaining('The payment is on its way'), findsOneWidget);

    // A cancel that arrives now, from a second click that was meant for
    // the review, does not turn "sent" back into an empty form.
    backend.cancelSend();
    await tester.pumpAndSettle();
    expect(find.text('SENT'), findsOneWidget);
    expect(backend.state.value!.send.phase, SendPhase.sent);

    await press(tester, 'Done');
    expect(backend.calls.last, 'clear send');
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.state.value!.send.phase, SendPhase.idle);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a review that is cancelled sends nothing and keeps the form', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    expect(find.text('CONFIRM'), findsOneWidget);

    await press(tester, 'Cancel');
    expect(backend.calls, isNot(contains('confirm send')));
    expect(find.text('SEND'), findsOneWidget);
    expect(fieldText(tester, 'Address to pay to'), payee);
    expect(fieldText(tester, 'Amount, XMR'), '1.5');

    await press(tester, 'Back');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('what the wallet objects to is said where it belongs', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    await type(tester, 'Address to pay to', 'not an address');
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    expect(find.text('SEND'), findsOneWidget);
    expect(
      find.text('This is not a Monero address of this network.'),
      findsOneWidget,
    );

    // Corrected, the objection goes, and the next one is about the amount.
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', 'a lot');
    await press(tester, 'Continue');
    expect(find.textContaining('not a Monero address'), findsNothing);
    expect(find.textContaining('This is not an amount.'), findsOneWidget);
    expect(find.textContaining('with a point'), findsOneWidget);

    await type(tester, 'Amount, XMR', '0');
    expect(find.textContaining('This is not an amount.'), findsNothing);
    await press(tester, 'Continue');
    expect(find.text('The amount must be more than zero.'), findsOneWidget);

    await type(tester, 'Amount, XMR', '50');
    await press(tester, 'Continue');
    expect(
      find.textContaining('There is not enough available'),
      findsOneWidget,
    );

    // A payment that would not fit into one transaction is not sent in
    // pieces: the wallet says so, by the amount.
    backend.manySmallCoins = true;
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    expect(find.text('SEND'), findsOneWidget);
    expect(find.textContaining('more than one transaction'), findsOneWidget);
    backend.manySmallCoins = false;

    await press(tester, 'Continue');
    expect(find.text('CONFIRM'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('everything can be sent, and mixed coins are warned about', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    await type(tester, 'Address to pay to', payee);
    await press(tester, 'Send everything');
    // No amount to type: the wallet works it out, fee and all.
    expect(find.text('Amount, XMR'), findsNothing);
    expect(
      find.text('Everything that is available, less the fee.'),
      findsOneWidget,
    );
    await press(tester, 'Continue');
    expect(backend.calls.last, 'prepare all to $payee');
    expect(find.text('${FakeBackend.everythingAmount} XMR'), findsOneWidget);
    expect(find.text('10.50 XMR'), findsOneWidget);
    expect(
      find.textContaining('spends coins from 3 of your addresses together'),
      findsOneWidget,
    );

    await press(tester, 'Cancel');
    await press(tester, 'Enter an amount');
    expect(find.text('Amount, XMR'), findsOneWidget);
  });

  testWidgets('a lost connection is never shown as a failure', (tester) async {
    final backend = await openSend(tester, 'en');
    backend.sendEndsAs = SendPhase.outcomeUnknown;
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    await press(tester, 'Send payment');

    expect(find.text('NOT KNOWN'), findsOneWidget);
    expect(find.textContaining('It may have gone out, or not'), findsOneWidget);
    // Nothing here offers to send again, or to make the warning go away.
    expect(find.text('I UNDERSTAND'), findsNothing);
    expect(find.text('CONTINUE'), findsNothing);

    // Home says it too, and Send leads back to it, not to a form.
    await press(tester, 'Back to the wallet');
    expect(find.text('WALLET'), findsOneWidget);
    expect(
      find.textContaining('A payment may or may not have gone out'),
      findsOneWidget,
    );
    await press(tester, 'Send');
    expect(find.text('NOT KNOWN'), findsOneWidget);

    // The core finds the transaction after a synchronisation: sent.
    backend.report(send: SendStatus(SendPhase.sent, txids: ['a' * 64]));
    await tester.pumpAndSettle();
    expect(find.text('SENT'), findsOneWidget);
    await press(tester, 'Done');
    expect(
      find.textContaining('A payment may or may not have gone out'),
      findsNothing,
    );
  });

  testWidgets('a payment that never shows up can be sent again, knowingly', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    backend.sendEndsAs = SendPhase.outcomeUnknown;
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    await press(tester, 'Send payment');

    // The core has synchronised well after the attempt and not found it.
    backend.report(
      send: SendStatus(
        SendPhase.outcomeUnknown,
        txids: ['a' * 64],
        settled: true,
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Most likely it never went out'),
      findsOneWidget,
    );
    expect(find.textContaining('paid twice'), findsOneWidget);

    await press(tester, 'I understand');
    // Back at the form, as it was filled in.
    expect(find.text('SEND'), findsOneWidget);
    expect(fieldText(tester, 'Address to pay to'), payee);
    expect(enabled(tester, 'Continue'), isTrue);
  });

  testWidgets('a send the node refuses says so and returns to the form', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    backend.sendEndsAs = SendPhase.failed;
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    await press(tester, 'Send payment');
    expect(find.text('SEND'), findsOneWidget);
    expect(find.textContaining('the node refused it'), findsOneWidget);
    // Having pressed Send, the user is told where the money is.
    expect(find.textContaining('Nothing was sent.'), findsOneWidget);

    // Left and opened again, the form does not bring the old refusal back.
    await press(tester, 'Back');
    await press(tester, 'Send');
    expect(find.textContaining('the node refused it'), findsNothing);
  });

  testWidgets('a send that fails because Tor lost its connection says so', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');
    await press(tester, 'Continue');
    // The device has lost its network by the time Send is pressed: no
    // node can be asked, and the core says that it was Tor.
    backend.report(
      tor: const TorStatus(TorState.lost, 100, 'error connecting to Tor'),
      send: const SendStatus(
        SendPhase.failed,
        problem: Problem(
          ProblemKind.torLost,
          'Tor cannot reach its network: error connecting to Tor',
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('SEND'), findsOneWidget);
    expect(
      find.text(
        'Tor cannot reach its network. Check the internet connection of '
        'this device. Nothing was sent. You can try again.',
      ),
      findsOneWidget,
    );
    // The form is as it was filled in, to be sent again later.
    expect(fieldText(tester, 'Address to pay to'), payee);
    expect(enabled(tester, 'Continue'), isTrue);
  });

  testWidgets('preparing and sending show progress, and wait in words', (
    tester,
  ) async {
    final backend = await openSend(tester, 'en');
    await type(tester, 'Address to pay to', payee);
    await type(tester, 'Amount, XMR', '1.5');

    // The core is still building the transaction.
    backend.report(send: const SendStatus(SendPhase.preparing));
    await tester.pump();
    expect(find.text('Preparing the transaction…'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    expect(enabled(tester, 'Continue'), isFalse);

    // During a synchronisation it waits for that, and says so.
    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 2250000,
        nodeHeight: 2256644,
      ),
    );
    await tester.pump();
    expect(
      find.text(
        'The transaction is prepared when the synchronisation has finished.',
      ),
      findsOneWidget,
    );

    // It can be called off.
    await press(tester, 'Cancel');
    expect(backend.calls.last, 'cancel send');
    expect(find.text('Preparing the transaction…'), findsNothing);
    expect(enabled(tester, 'Continue'), isTrue);

    // While it is being handed to the node, nothing can be pressed.
    backend.report(
      send: SendStatus(
        SendPhase.sending,
        to: payee,
        amount: '1.50',
        fee: FakeBackend.fee,
        total: '1.50003066',
        sourceAddresses: const [1],
      ),
    );
    await tester.pump();
    expect(find.text('Sending…'), findsOneWidget);
    expect(enabled(tester, 'Send payment'), isFalse);
    expect(enabled(tester, 'Cancel'), isFalse);
    backend.report(send: SendStatus(SendPhase.sent, txids: ['a' * 64]));
    await tester.pumpAndSettle();
    expect(find.text('SENT'), findsOneWidget);
  });

  testWidgets('paste takes an address or a payment link', (tester) async {
    clipboard(tester, text: 'monero:$payee?tx_amount=2.5&tx_description=tea');
    await openSend(tester, 'en');
    await press(tester, 'Paste');
    expect(fieldText(tester, 'Address to pay to'), payee);
    expect(fieldText(tester, 'Amount, XMR'), '2.5');
    expect(enabled(tester, 'Continue'), isTrue);
  });

  testWidgets('an empty clipboard is said to be empty', (tester) async {
    clipboard(tester);
    await openSend(tester, 'en');
    await press(tester, 'Paste');
    expect(find.text('There is no address in the clipboard.'), findsOneWidget);
    expect(fieldText(tester, 'Address to pay to'), '');
  });

  test('pasted text is taken apart, not judged', () {
    PaymentTarget read(String text) => readPayment(text);
    expect(read('  abc  ').address, 'abc');
    expect(read('ab cd\nef').address, 'abcdef');
    expect(read('abc').amount, isNull);
    expect(read('monero:abc').address, 'abc');
    expect(read('MONERO:abc').address, 'abc');
    expect(read('monero:abc?tx_amount=1.25').address, 'abc');
    expect(read('monero:abc?tx_amount=1.25').amount, '1.25');
    expect(read('monero:abc?recipient_name=x&tx_amount=3').amount, '3');
    // An amount that is not a plain number is not passed on.
    for (final amount in ['-1', '1e9', '1,5', '', 'all', '1.', '.5', '1.2.3']) {
      expect(read('monero:abc?tx_amount=$amount').amount, isNull);
      expect(read('monero:abc?tx_amount=$amount').address, 'abc');
    }
    expect(read('monero:abc?').address, 'abc');
    expect(read('').address, '');
  });

  testWidgets('sending fits the window in Russian too', (tester) async {
    final backend = await openSend(tester, 'ru');
    expect(find.text('ОТПРАВИТЬ'), findsOneWidget);
    expect(find.text('Доступно: 10.50 XMR'), findsOneWidget);
    await type(tester, 'Адрес получателя', payee);
    await press(tester, 'Отправить всё');
    await press(tester, 'Продолжить');
    expect(find.text('ПОДТВЕРЖДЕНИЕ'), findsOneWidget);
    expect(find.text('Итого'), findsOneWidget);
    expect(
      find.textContaining('с разных ваших адресов (их 3)'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);

    backend.sendEndsAs = SendPhase.outcomeUnknown;
    await press(tester, 'Отправить платёж');
    expect(find.text('НЕИЗВЕСТНО'), findsOneWidget);
    expect(find.textContaining('Он мог уйти, а мог и не уйти'), findsOneWidget);
    expect(tester.takeException(), isNull);

    backend.report(
      send: SendStatus(
        SendPhase.outcomeUnknown,
        txids: ['a' * 64],
        settled: true,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('Скорее всего, он не ушёл'), findsOneWidget);
    await press(tester, 'Понятно');
    expect(find.text('ОТПРАВИТЬ'), findsOneWidget);

    await press(tester, 'Указать сумму');
    await type(tester, 'Сумма, XMR', '50');
    await press(tester, 'Продолжить');
    expect(find.textContaining('Доступных средств не хватает'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
