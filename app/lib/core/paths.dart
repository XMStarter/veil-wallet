import 'dart:io';

import 'package:veil_bridge/veil_bridge.dart' show WalletNetwork;

/// Where the app keeps its files and finds its native parts.
///
/// On a desktop everything is worked out from the environment the system
/// gives the app. On Android the system is asked for the one folder that
/// is the app's own ([AppPaths.onAndroid]). No package is involved.
class AppPaths {
  AppPaths._(this.dataDir, this._programDir, this._environment, this.network);

  /// For Android, where [filesDir] is the app's private storage: no other
  /// app can read it. The two libraries are inside the app's package and
  /// are asked for by name; Android knows where they are. [libraryDir] is
  /// the folder Android unpacked the package's native parts into, the
  /// only place an app may start a program from: the transport program
  /// for bridges lies there.
  factory AppPaths.onAndroid(
    String filesDir, {
    String? libraryDir,
    WalletNetwork network = WalletNetwork.stagenet,
  }) => AppPaths._(filesDir, '', const {}, network)
    .._android = true
    .._transportDir = libraryDir;

  /// For the running app.
  factory AppPaths.forThisDevice({
    WalletNetwork network = WalletNetwork.stagenet,
  }) => AppPaths.from(
    environment: Platform.environment,
    program: Platform.resolvedExecutable,
    windows: Platform.isWindows,
    network: network,
  );

  /// From an [environment], for tests as well.
  factory AppPaths.from({
    required Map<String, String> environment,
    required String program,
    required bool windows,
    WalletNetwork network = WalletNetwork.stagenet,
  }) {
    final separator = windows ? r'\' : '/';
    final String base;
    if (windows) {
      base =
          environment['APPDATA'] ??
          '${environment['USERPROFILE']}\\AppData\\Roaming';
    } else {
      final xdg = environment['XDG_DATA_HOME'];
      base = (xdg != null && xdg.isNotEmpty)
          ? xdg
          : '${environment['HOME']}/.local/share';
    }
    final name = windows ? 'Veil' : 'veil';
    final programDir = program.substring(0, program.lastIndexOf(separator));
    return AppPaths._('$base$separator$name', programDir, environment, network)
      .._windows = windows;
  }

  /// The app's own folder among the user's data.
  final String dataDir;

  /// The network the wallet is on. Stagenet unless the app says otherwise:
  /// whoever makes these without thinking gets a wallet without money in
  /// it.
  final WalletNetwork network;
  final String _programDir;
  final Map<String, String> _environment;
  bool _windows = false;
  bool _android = false;
  String? _transportDir;

  /// The folder of the transport programs the app brings along, for
  /// bridges; null where it brings none and the ones installed on the
  /// machine are used.
  String? get transportDir => _transportDir;

  /// Whether [coreLibrary] and [walletLibrary] are files that can be
  /// looked for. On Android they are names, found by the system inside
  /// the app's package, where no file can be looked for.
  bool get librariesAreFiles => !_android;

  String get _separator => _windows ? r'\' : '/';

  /// The folder of the wallet, which is the network's own: a wallet with
  /// real money in it and one on stagenet never lie in the same place, and
  /// an app built for one does not see the other's.
  String get walletDir => '$dataDir$_separator${network.name}';

  /// The wallet's data file; its keys file lies next to it.
  String get walletFile => '$walletDir${_separator}wallet';

  String get walletKeysFile => '$walletFile.keys';

  /// What the app remembers about the wallet besides the wallet itself.
  String get notesFile => '$walletDir${_separator}app.json';

  /// What the app remembers apart from any wallet.
  String get settingsFile => '$dataDir${_separator}settings.json';

  String get torStateDir => '$dataDir${_separator}tor${_separator}state';

  String get torCacheDir => '$dataDir${_separator}tor${_separator}cache';

  /// The core: where `VEIL_CORE_LIB` says, or else shipped with the app.
  String get coreLibrary =>
      _environment['VEIL_CORE_LIB'] ??
      (_android
          ? 'libveil_core.so'
          : _windows
          ? '$_programDir\\veil_core.dll'
          : '$_programDir/lib/libveil_core.so');

  /// The Monero wallet library: where `VEIL_WALLET_LIB` says, or else
  /// shipped with the app.
  String get walletLibrary =>
      _environment['VEIL_WALLET_LIB'] ??
      (_android
          ? 'libmonero_wallet2_api_c.so'
          : _windows
          ? '$_programDir\\libmonero_wallet2_api_c.dll'
          : '$_programDir/lib/libmonero_wallet2_api_c.so');
}
