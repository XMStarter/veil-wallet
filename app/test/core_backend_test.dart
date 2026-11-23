// The real backend against the real core, as far as it can go without
// opening a wallet: once a wallet is open the core starts Tor, and no test
// does that.
//
// Needs the core built as a shared library (`cargo build -p veil_core` in
// native/, or VEIL_CORE_LIB) and is skipped without it.

import 'dart:io';

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
}
