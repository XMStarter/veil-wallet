import 'dart:io';

/// Where the app keeps its files and finds its native parts.
///
/// Everything is worked out from the environment the system gives the
/// app; no package is involved.
class AppPaths {
  AppPaths._(this.dataDir, this._programDir, this._environment);

  /// For the running app.
  factory AppPaths.forThisDevice() => AppPaths.from(
    environment: Platform.environment,
    program: Platform.resolvedExecutable,
    windows: Platform.isWindows,
  );

  /// From an [environment], for tests as well.
  factory AppPaths.from({
    required Map<String, String> environment,
    required String program,
    required bool windows,
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
    return AppPaths._('$base$separator$name', programDir, environment)
      .._windows = windows;
  }

  /// The app's own folder among the user's data.
  final String dataDir;
  final String _programDir;
  final Map<String, String> _environment;
  bool _windows = false;

  String get _separator => _windows ? r'\' : '/';

  /// The folder of the stagenet wallet. A wallet for real funds will have
  /// a folder of its own.
  String get walletDir => '$dataDir${_separator}stagenet';

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
      (_windows
          ? '$_programDir\\veil_core.dll'
          : '$_programDir/lib/libveil_core.so');

  /// The Monero wallet library: where `VEIL_WALLET_LIB` says, or else
  /// shipped with the app.
  String get walletLibrary =>
      _environment['VEIL_WALLET_LIB'] ??
      (_windows
          ? '$_programDir\\libmonero_wallet2_api_c.dll'
          : '$_programDir/lib/libmonero_wallet2_api_c.so');
}
