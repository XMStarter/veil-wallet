// The real backend against the real core, as far as it can go without
// opening a wallet: once a wallet is open the core starts Tor, and no test
// does that.
//
// Needs the core built as a shared library (`cargo build -p veil_core` in
// native/, or VEIL_CORE_LIB) and is skipped without it.

import 'dart:async';
import 'dart:io';
import 'dart:isolate';

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/core_backend.dart';
import 'package:veil/core/paths.dart';

String? coreLibrary() {
  final named = Platform.environment['VEIL_CORE_LIB'];
  if (named != null) return named;
  final file = Platform.isWindows ? 'veil_core.dll' : 'libveil_core.so';
  final built = File('../native/target/debug/$file');
  return built.existsSync() ? built.absolute.path : null;
}

void main() {
  final core = coreLibrary();
  final skip = core == null
      ? 'the core is not built as a shared library'
      : null;
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('veil-app-'));
  tearDown(() => dir.deleteSync(recursive: true));

  AppPaths paths({String? walletLibrary}) => AppPaths.from(
    environment: {
      'HOME': dir.path,
      'APPDATA': dir.path,
      'VEIL_CORE_LIB': ?core,
      // A file that exists and is no wallet library: the core then fails
      // on loading it, before any wallet is opened.
      'VEIL_WALLET_LIB': walletLibrary ?? core ?? '',
    },
    program: Platform.resolvedExecutable,
    windows: Platform.isWindows,
  );

  test(
    'a device without a wallet has none, and remembers what it is told',
    () async {
      final backend = CoreBackend(paths());
      expect(await backend.walletExists(), isFalse);
      expect(backend.state.value, isNull);
      expect(await backend.recoveryPhrase(), isNull);
      expect(await backend.phraseConfirmed(), isFalse);
      await Directory(paths().walletDir).create(recursive: true);
      await backend.confirmPhrase();
      expect(await backend.phraseConfirmed(), isTrue);
      await backend.close();
    },
    skip: skip,
  );

  test(
    'a missing part of the app is named before anything is started',
    () async {
      final backend = CoreBackend(
        paths(walletLibrary: '${dir.path}/not-there.so'),
      );
      await expectLater(
        backend.start(WalletSource.create, password: 'correct horse battery'),
        throwsA(
          isA<StartFailure>()
              .having((f) => f.problem.kind, 'kind', libraryMissing)
              .having(
                (f) => f.problem.detail,
                'detail',
                contains('not-there.so'),
              ),
        ),
      );
      expect(Directory(paths().torStateDir).existsSync(), isFalse);
    },
    skip: skip,
  );

  test(
    'a wallet that does not open fails cleanly, with the session closed',
    () async {
      final backend = CoreBackend(paths());
      await expectLater(
        backend.start(WalletSource.open, password: 'correct horse battery'),
        throwsA(isA<StartFailure>()),
      );
      expect(backend.state.value, isNull);
      // Tor was never started, and nothing was left behind as a wallet.
      expect(Directory(paths().torStateDir).existsSync(), isFalse);
      expect(await backend.walletExists(), isFalse);
      // The backend can be asked again.
      await expectLater(
        backend.start(WalletSource.open, password: 'correct horse battery'),
        throwsA(isA<StartFailure>()),
      );
      await backend.close();
    },
    skip: skip,
  );

  test(
    'a wallet that does not open fails the same way with the app listening',
    () async {
      // In the app, screens listen to the backend, and what listens leads
      // to things that cannot be handed to another isolate: timers, the
      // window. The backend closes a failed session on another isolate,
      // and nothing of the app may travel with that call.
      final backend = CoreBackend(paths());
      final watcher = _Watcher();
      backend.state.addListener(watcher.changed);
      backend.settings.addListener(watcher.changed);
      addTearDown(watcher.stop);
      for (var attempt = 0; attempt < 2; attempt++) {
        await expectLater(
          backend.start(WalletSource.open, password: 'correct horse battery'),
          throwsA(isA<StartFailure>()),
        );
      }
      expect(backend.state.value, isNull);
      expect(Directory(paths().torStateDir).existsSync(), isFalse);
      await backend.close();
    },
    skip: skip,
  );

  test(
    'settings are kept in a file, and a damaged one costs only them',
    () async {
      final backend = CoreBackend(paths());
      await backend.loadSettings();
      expect(
        backend.settings.value.autoLockMinutes,
        AppSettings.defaultAutoLock,
      );
      expect(backend.settings.value.bridges, isEmpty);

      const plain = '192.0.2.55:443 316E643333645F6D79216558614D3931657A5F5F';
      await backend.saveSettings(
        const AppSettings(language: 'ru', dark: true, bridges: [plain]),
      );
      // Another start of the app reads them back.
      final again = CoreBackend(paths());
      await again.loadSettings();
      expect(again.settings.value.language, 'ru');
      expect(again.settings.value.dark, isTrue);
      expect(again.settings.value.bridges, [plain]);

      await File(paths().settingsFile).writeAsString('{ not json');
      final damaged = CoreBackend(paths());
      await damaged.loadSettings();
      expect(damaged.settings.value.language, isNull);
      expect(damaged.settings.value.bridges, isEmpty);
    },
  );

  test('bridges are checked by the core itself', () {
    final backend = CoreBackend(paths());
    const plain = '192.0.2.55:443 316E643333645F6D79216558614D3931657A5F5F';
    expect(backend.checkBridges(const [plain]), isNull);
    expect(
      backend.checkBridges(const ['nonsense'])?.kind,
      ProblemKind.invalidBridge,
    );
    expect(
      backend.checkBridges(const ['conjure $plain'])?.kind,
      ProblemKind.transportMissing,
    );
  }, skip: skip);

  test(
    'bridges that are not bridges keep a wallet from being started with them',
    () async {
      final backend = CoreBackend(paths());
      await backend.saveSettings(const AppSettings(bridges: ['nonsense']));
      await expectLater(
        backend.start(WalletSource.open, password: 'correct horse battery'),
        throwsA(
          isA<StartFailure>().having(
            (f) => f.problem.kind,
            'kind',
            ProblemKind.invalidBridge,
          ),
        ),
      );
      // Without them the start gets as far as the wallet, which is not
      // there: a different failure.
      await expectLater(
        backend.start(
          WalletSource.open,
          password: 'correct horse battery',
          withoutBridges: true,
        ),
        throwsA(
          isA<StartFailure>().having(
            (f) => f.problem.kind,
            'kind',
            isNot(ProblemKind.invalidBridge),
          ),
        ),
      );
      await backend.close();
    },
    skip: skip,
  );

  test('removing the wallet takes its folder and nothing else', () async {
    final backend = CoreBackend(paths());
    final wallet = Directory(paths().walletDir);
    await wallet.create(recursive: true);
    // What a wallet leaves in its folder, by name; no wallet is opened.
    // The wallet library keeps nothing outside that folder, which
    // `native/veil_wallet/tests/offline.rs` checks with real wallets.
    for (final name in [
      'wallet',
      'wallet.keys',
      'wallet.lock',
      'wallet.address.txt',
      'wallet.unreadable-1',
      'wallet.unportable',
    ]) {
      await File('${wallet.path}/$name').writeAsString('x');
    }
    await backend.confirmPhrase();
    await backend.saveSettings(const AppSettings(language: 'ru'));
    expect(await backend.walletExists(), isTrue);

    await backend.removeWallet();
    expect(await backend.walletExists(), isFalse);
    expect(wallet.existsSync(), isFalse);
    expect(await backend.phraseConfirmed(), isFalse);
    // How the app is set to behave is not the wallet's.
    expect(File(paths().settingsFile).existsSync(), isTrue);
    // Removing what is not there is nothing.
    await backend.removeWallet();
  });
}

/// Stands for a screen: it listens, and holds what a screen holds.
class _Watcher {
  final Timer _timer = Timer.periodic(const Duration(hours: 1), (_) {});
  final ReceivePort _port = ReceivePort();
  int changes = 0;

  void changed() => changes++;

  void stop() {
    _timer.cancel();
    _port.close();
  }
}
