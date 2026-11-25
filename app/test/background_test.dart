// What a phone is told while a wallet is open, so that it keeps the app
// running outside the screen, on a wallet and a device that only pretend.

import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';

import 'support.dart';

/// The app leaves the screen and comes back, as a phone reports it.
Future<void> leaveAndReturn(WidgetTester tester) async {
  for (final state in [
    AppLifecycleState.inactive,
    AppLifecycleState.hidden,
    AppLifecycleState.paused,
    AppLifecycleState.hidden,
    AppLifecycleState.inactive,
    AppLifecycleState.resumed,
  ]) {
    tester.binding.handleAppLifecycleStateChanged(state);
  }
  await tester.pump();
}

const synchronising = SyncStatus(
  SyncPhase.running,
  walletHeight: 1000,
  nodeHeight: 2000000,
);
const synchronised = SyncStatus(
  SyncPhase.done,
  walletHeight: 2000000,
  nodeHeight: 2000000,
);

void main() {
  setUpAll(loadFonts);

  testWidgets('a phone is told of an open wallet and of nothing in it', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    // No wallet is open on the screen that asks for the password.
    expect(backend.device.presence, isNull);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    // Open, and Tor is connecting: a phone that slept now would cut
    // that off.
    expect(
      backend.device.presence,
      const Presence(
        kind: 'Open wallet',
        title: 'Veil',
        text: 'The wallet is open.',
        busy: true,
      ),
    );

    backend.tor(TorState.ready);
    backend.report(sync: synchronising, total: '12.50', unlocked: '12.50');
    await tester.pump();
    expect(
      backend.device.presence!.text,
      'The wallet is open and synchronising.',
    );
    expect(backend.device.presence!.busy, isTrue);

    // Synchronised: the phone may sleep, the app is still kept.
    backend.report(sync: synchronised);
    await tester.pump();
    expect(backend.device.presence!.text, 'The wallet is open.');
    expect(backend.device.presence!.busy, isFalse);

    // A payment on its way is not left to a phone that goes to sleep.
    backend.report(send: const SendStatus(SendPhase.sending));
    await tester.pump();
    expect(backend.device.presence!.busy, isTrue);
    backend.report(send: const SendStatus(SendPhase.sent));
    await tester.pump();
    expect(backend.device.presence!.busy, isFalse);

    // Neither the balance nor an address was ever part of it.
    for (final presence in backend.device.presences.nonNulls) {
      final said = '${presence.kind} ${presence.title} ${presence.text}';
      expect(said, isNot(contains('12.50')));
      expect(said, isNot(contains(testAddresses[0])));
    }

    // Locked, the wallet is closed, and the phone is told that too.
    await press(tester, 'Lock wallet');
    expect(backend.state.value, isNull);
    expect(backend.device.presence, isNull);
    expect(backend.device.presences.last, isNull);
  });

  testWidgets('the phone is told in the language of the app', (tester) async {
    final backend = await openWallet(tester, 'ru');
    backend.tor(TorState.ready);
    backend.report(sync: synchronising);
    await tester.pump();
    expect(
      backend.device.presence,
      const Presence(
        kind: 'Открытый кошелёк',
        title: 'Veil',
        text: 'Кошелёк открыт и синхронизируется.',
        busy: true,
      ),
    );
  });

  testWidgets('a change that changes nothing to show is not passed on, '
      'and a busy wallet says so again every minute', (tester) async {
    final backend = await openWallet(tester, 'en');
    final device = backend.device;
    device.time = DateTime(2026, 11, 25, 12);
    backend.tor(TorState.ready);
    backend.report(sync: synchronising);
    await tester.pump();
    final told = device.presences.length;

    // Blocks come in; what the phone shows stays the same.
    for (var block = 0; block < 5; block++) {
      device.time = device.time!.add(const Duration(seconds: 5));
      backend.report(
        sync: SyncStatus(
          SyncPhase.running,
          walletHeight: 2000 + block,
          nodeHeight: 2000000,
        ),
      );
      await tester.pump();
    }
    expect(device.presences.length, told);

    // A minute after the last telling the next change says it again:
    // the phone stays awake only for a while after each.
    device.time = device.time!.add(Device.busyAgainAfter);
    backend.report(
      sync: const SyncStatus(
        SyncPhase.running,
        walletHeight: 3000,
        nodeHeight: 2000000,
      ),
    );
    await tester.pump();
    expect(device.presences.length, told + 1);
    expect(device.presences.last!.busy, isTrue);

    // A wallet that is not busy does not keep saying so.
    backend.report(sync: synchronised);
    await tester.pump();
    final quiet = device.presences.length;
    device.time = device.time!.add(const Duration(minutes: 3));
    backend.report(
      sync: const SyncStatus(
        SyncPhase.done,
        walletHeight: 2000001,
        nodeHeight: 2000001,
      ),
    );
    await tester.pump();
    expect(device.presences.length, quiet);
  });

  testWidgets('back in front, the phone is asked again to keep the app', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    backend.tor(TorState.ready);
    backend.report(sync: synchronised);
    await tester.pump();
    final told = backend.device.presences.length;

    // A phone stops keeping the app after some hours and does not say
    // so to this side. Asking again costs nothing when it still does.
    await leaveAndReturn(tester);
    expect(backend.device.presences.length, told + 1);
    expect(backend.device.presence!.text, 'The wallet is open.');
    expect(backend.state.value, isNotNull);
  });

  testWidgets('with no wallet open, coming back asks the phone for nothing', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    await leaveAndReturn(tester);
    expect(backend.device.presences.nonNulls, isEmpty);
  });

  testWidgets('a phone that slept through the time to the auto-lock '
      'locks the wallet when the app is in front again', (tester) async {
    final backend = await openWallet(tester, 'en');
    final device = backend.device;
    device.time = DateTime(2026, 11, 25, 12);
    // Used now, by that clock; the lock is left at its five minutes.
    await tester.tap(find.text('WALLET'));
    backend.tor(TorState.ready);
    backend.report(sync: synchronised);
    await tester.pump();

    // Back after four minutes by the clock: still open.
    device.time = device.time!.add(const Duration(minutes: 4));
    await leaveAndReturn(tester);
    await tester.pumpAndSettle();
    expect(backend.state.value, isNotNull);
    expect(find.text('WALLET'), findsOneWidget);

    // The phone sleeps. Its timers stand still, here as there: no time
    // passes for them. The clock goes on, and it is what counts.
    device.time = device.time!.add(const Duration(minutes: 2));
    await leaveAndReturn(tester);
    await tester.pumpAndSettle();
    expect(backend.calls.last, 'close');
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(device.presence, isNull);
  });

  testWidgets('coming back overdue does not cut a synchronisation short', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    final device = backend.device;
    device.time = DateTime(2026, 11, 25, 12);
    await backend.saveSettings(const AppSettings(autoLockMinutes: 1));
    await tester.tap(find.text('WALLET'));
    backend.tor(TorState.ready);
    backend.report(sync: synchronising);
    await tester.pump();

    device.time = device.time!.add(const Duration(hours: 1));
    await leaveAndReturn(tester);
    await tester.pump(const Duration(seconds: 20));
    expect(backend.state.value, isNotNull);

    // It locks when the synchronisation is over, without being used.
    backend.report(sync: synchronised);
    await tester.pump();
    await tester.pump(const Duration(seconds: 6));
    await tester.pumpAndSettle();
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
  });

  testWidgets('a phone set to stop the app outside the screen is said so', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    backend.device.restrictedNow = true;
    await pumpApp(tester, 'en', backend: backend);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.textContaining('Android is set to stop Veil'), findsOneWidget);

    // The user changes that in Android's settings and comes back.
    backend.device.restrictedNow = false;
    await leaveAndReturn(tester);
    await tester.pumpAndSettle();
    expect(find.textContaining('Android is set to stop Veil'), findsNothing);
  });

  testWidgets('a phone that is not set so says nothing, in Russian neither', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'ru');
    expect(find.textContaining('Android'), findsNothing);
    backend.device.restrictedNow = true;
    await leaveAndReturn(tester);
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Android настроен останавливать Veil'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}
