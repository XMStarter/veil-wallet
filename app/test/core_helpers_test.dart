import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/paths.dart';
import 'package:veil/core/phrase.dart';

void main() {
  test('a phrase is its words, however it was typed', () {
    expect(phraseWords('  Alpha  beta\n\tGAMMA '), ['alpha', 'beta', 'gamma']);
    expect(phraseWords(''), isEmpty);
    expect(isPhraseLength(16), isTrue);
    expect(isPhraseLength(25), isTrue);
    for (final count in [0, 1, 12, 15, 17, 24, 26]) {
      expect(isPhraseLength(count), isFalse, reason: '$count');
    }
    expect(sameWord(' Alpha ', 'alpha'), isTrue);
    expect(sameWord('alpha', 'alphabet'), isFalse);
  });

  test('on Linux the app keeps its files under the user\'s data folder', () {
    final paths = AppPaths.from(
      environment: {'HOME': '/home/anna'},
      program: '/opt/veil/veil',
      windows: false,
    );
    expect(paths.dataDir, '/home/anna/.local/share/veil');
    expect(paths.walletFile, '/home/anna/.local/share/veil/stagenet/wallet');
    expect(
      paths.walletKeysFile,
      '/home/anna/.local/share/veil/stagenet/wallet.keys',
    );
    expect(paths.torStateDir, '/home/anna/.local/share/veil/tor/state');
    expect(paths.torCacheDir, '/home/anna/.local/share/veil/tor/cache');
    expect(paths.coreLibrary, '/opt/veil/lib/libveil_core.so');
    expect(paths.walletLibrary, '/opt/veil/lib/libmonero_wallet2_api_c.so');

    final moved = AppPaths.from(
      environment: {'HOME': '/home/anna', 'XDG_DATA_HOME': '/data'},
      program: '/opt/veil/veil',
      windows: false,
    );
    expect(moved.dataDir, '/data/veil');
  });

  test('on Android, in the app\'s own storage, with libraries by name', () {
    final paths = AppPaths.onAndroid(
      '/data/user/0/io.github.xmstarter.veil/files',
    );
    const files = '/data/user/0/io.github.xmstarter.veil/files';
    expect(paths.dataDir, files);
    expect(paths.walletFile, '$files/stagenet/wallet');
    expect(paths.walletKeysFile, '$files/stagenet/wallet.keys');
    expect(paths.settingsFile, '$files/settings.json');
    expect(paths.torStateDir, '$files/tor/state');
    expect(paths.torCacheDir, '$files/tor/cache');
    // Inside the app's package there are no files to look for: Android
    // finds a library by its name.
    expect(paths.librariesAreFiles, isFalse);
    expect(paths.coreLibrary, 'libveil_core.so');
    expect(paths.walletLibrary, 'libmonero_wallet2_api_c.so');

    final desktop = AppPaths.from(
      environment: {'HOME': '/home/anna'},
      program: '/opt/veil/veil',
      windows: false,
    );
    expect(desktop.librariesAreFiles, isTrue);
  });

  test('on Windows, under the user\'s application data', () {
    final paths = AppPaths.from(
      environment: {'APPDATA': r'C:\Users\anna\AppData\Roaming'},
      program: r'C:\Program Files\Veil\veil.exe',
      windows: true,
    );
    expect(paths.dataDir, r'C:\Users\anna\AppData\Roaming\Veil');
    expect(
      paths.walletFile,
      r'C:\Users\anna\AppData\Roaming\Veil\stagenet\wallet',
    );
    expect(paths.torStateDir, r'C:\Users\anna\AppData\Roaming\Veil\tor\state');
    expect(paths.coreLibrary, r'C:\Program Files\Veil\veil_core.dll');
    expect(
      paths.walletLibrary,
      r'C:\Program Files\Veil\libmonero_wallet2_api_c.dll',
    );
  });

  test('the native parts can be named from outside, for development', () {
    final paths = AppPaths.from(
      environment: {
        'HOME': '/home/anna',
        'VEIL_CORE_LIB': '/x/core.so',
        'VEIL_WALLET_LIB': '/y/w.so',
      },
      program: '/opt/veil/veil',
      windows: false,
    );
    expect(paths.coreLibrary, '/x/core.so');
    expect(paths.walletLibrary, '/y/w.so');
  });
}
