import 'dart:convert';
import 'dart:io';

/// What the app remembers about how it is to behave, apart from any
/// wallet. It is kept in a plain file: nothing in it is a secret, and the
/// bridges have to be known before a wallet is open. The user's own nodes
/// are not here; the wallet keeps those, encrypted.
class AppSettings {
  const AppSettings({
    this.language,
    this.dark = false,
    this.autoLockMinutes = defaultAutoLock,
    this.bridges = const [],
  });

  /// Lock after five minutes without use, unless the user says otherwise.
  static const defaultAutoLock = 5;

  /// What the auto-lock can be set to. There is no "never".
  static const autoLockChoices = [1, 5, 15, 60];

  /// `en` or `ru`; null follows the device.
  final String? language;
  final bool dark;

  /// Minutes without use after which the wallet locks itself.
  final int autoLockMinutes;

  /// Bridge lines to reach Tor through. Empty: directly.
  final List<String> bridges;

  AppSettings copyWith({
    String? Function()? language,
    bool? dark,
    int? autoLockMinutes,
    List<String>? bridges,
  }) => AppSettings(
    language: language == null ? this.language : language(),
    dark: dark ?? this.dark,
    autoLockMinutes: autoLockMinutes ?? this.autoLockMinutes,
    bridges: bridges ?? this.bridges,
  );

  Map<String, Object?> toJson() => {
    'language': language,
    'dark': dark,
    'auto_lock_minutes': autoLockMinutes,
    'bridges': bridges,
  };

  /// Reads what [toJson] wrote. Anything missing or of the wrong kind
  /// falls back to its default, so a damaged file costs settings and
  /// nothing else.
  factory AppSettings.fromJson(Object? json) {
    if (json is! Map<String, dynamic>) return const AppSettings();
    final language = json['language'];
    final dark = json['dark'];
    final minutes = json['auto_lock_minutes'];
    final bridges = json['bridges'];
    return AppSettings(
      language: language == 'en' || language == 'ru'
          ? language as String
          : null,
      dark: dark == true,
      autoLockMinutes: autoLockChoices.contains(minutes)
          ? minutes as int
          : defaultAutoLock,
      bridges: bridges is List
          ? bridges.whereType<String>().toList()
          : const [],
    );
  }

  /// The settings in [file]; the defaults if there is none or it cannot
  /// be read.
  static Future<AppSettings> load(String file) async {
    try {
      return AppSettings.fromJson(jsonDecode(await File(file).readAsString()));
    } on FileSystemException {
      return const AppSettings();
    } on FormatException {
      return const AppSettings();
    }
  }

  Future<void> save(String file) async {
    await File(file).parent.create(recursive: true);
    await File(file).writeAsString(jsonEncode(toJson()), flush: true);
  }
}

/// Bridge lines out of text with one bridge per line. Empty lines and
/// lines that start with `#` are left out, as the core does.
List<String> bridgeLines(String text) => [
  for (final line in text.split('\n'))
    if (line.trim().isNotEmpty && !line.trim().startsWith('#')) line.trim(),
];

/// Nodes out of text with one node per line.
List<String> nodeLines(String text) => [
  for (final line in text.split(RegExp(r'[\n,]')))
    if (line.trim().isNotEmpty) line.trim(),
];
