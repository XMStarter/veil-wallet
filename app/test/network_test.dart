// Which network an app is for: said when it is built, kept apart on the
// disk, and said on the screens.

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/network.dart';
import 'package:veil/core/paths.dart';

import 'support.dart';

const _stagenetWords =
    'Test network. This version does not work with real funds.';
const _realMoneyWords = 'This wallet holds real money.';

void main() {
  setUpAll(loadFonts);

  test('an app that was told nothing is for stagenet', () {
    // The tests are built like an app nobody gave a network to.
    expect(builtFor, 'stagenet');
    expect(networkNamed(builtFor), WalletNetwork.stagenet);
    expect(networkNamed('mainnet'), WalletNetwork.mainnet);
    // A name that is neither is no network: the app does not start as
    // something its builder did not ask for.
    for (final name in ['', 'Mainnet', 'main', 'testnet', 'stagenet ']) {
      expect(() => networkNamed(name), throwsArgumentError, reason: name);
    }
  });

  test('each network has a folder of its own for its wallet', () {
    AppPaths paths(WalletNetwork? network) => network == null
        ? AppPaths.from(
            environment: const {'HOME': '/home/u'},
            program: '/opt/veil/veil',
            windows: false,
          )
        : AppPaths.from(
            environment: const {'HOME': '/home/u'},
            program: '/opt/veil/veil',
            windows: false,
            network: network,
          );
    final unsaid = paths(null);
    final stagenet = paths(WalletNetwork.stagenet);
    final mainnet = paths(WalletNetwork.mainnet);
    // Paths made without a network are stagenet's.
    expect(unsaid.network, WalletNetwork.stagenet);
    expect(unsaid.walletFile, stagenet.walletFile);
    expect(stagenet.walletFile, '/home/u/.local/share/veil/stagenet/wallet');
    expect(mainnet.walletFile, '/home/u/.local/share/veil/mainnet/wallet');
    expect(mainnet.walletKeysFile, isNot(stagenet.walletKeysFile));
    expect(mainnet.notesFile, isNot(stagenet.notesFile));
    // What is no wallet's is shared: the settings, with the bridges, and
    // Tor's own files.
    expect(mainnet.settingsFile, stagenet.settingsFile);
    expect(mainnet.torStateDir, stagenet.torStateDir);

    final phone = AppPaths.onAndroid(
      '/data/veil',
      network: WalletNetwork.mainnet,
    );
    expect(phone.walletFile, '/data/veil/mainnet/wallet');
    expect(
      AppPaths.onAndroid('/data/veil').walletFile,
      '/data/veil/stagenet/wallet',
    );
  });

  testWidgets('an app for stagenet says so wherever a balance is near', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text(_stagenetWords), findsOneWidget);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
    expect(find.text(_stagenetWords), findsOneWidget);
    expect(find.textContaining(_realMoneyWords), findsNothing);
  });

  testWidgets('an app for the main network says what real money means where a '
      'wallet begins, and nothing about a test', (tester) async {
    final backend = FakeBackend(network: WalletNetwork.mainnet);
    await pumpApp(tester, 'en', backend: backend);
    expect(find.textContaining(_realMoneyWords), findsOneWidget);
    expect(find.textContaining('has audited Veil'), findsOneWidget);
    expect(find.text(_stagenetWords), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('with a wallet open the main network is not said again under '
      'every balance, and is said under About', (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    final backend = FakeBackend(
      hasWallet: true,
      confirmed: true,
      network: WalletNetwork.mainnet,
    );
    await pumpApp(tester, 'en', backend: backend);
    expect(find.text(_stagenetWords), findsNothing);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
    expect(find.text(_stagenetWords), findsNothing);
    expect(find.textContaining(_realMoneyWords), findsNothing);

    await press(tester, 'Settings');
    await tester.ensureVisible(find.text('About Veil'));
    await tester.tap(find.text('About Veil'));
    await tester.pumpAndSettle();
    expect(find.textContaining(_realMoneyWords), findsOneWidget);
    expect(tester.takeException(), isNull);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('the words about real money fit the window in Russian too', (
    tester,
  ) async {
    final backend = FakeBackend(network: WalletNetwork.mainnet);
    await pumpApp(tester, 'ru', backend: backend);
    expect(find.textContaining('настоящие деньги'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
