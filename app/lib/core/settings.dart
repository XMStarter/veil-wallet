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
    this.lost = false,
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

  /// Whether these are what was left of a settings file that could not be
  /// read, or whose bridges could not. The bridges are then not known to
  /// be none: they may have been there and be gone, and without them Tor
  /// is connected to directly. The app says so and connects nowhere until
  /// the user has chosen. Not kept in the file; whatever is saved is not
  /// lost.
  final bool lost;

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
  /// nothing else. Bridges are the exception: where they cannot be read,
  /// the settings say that they were [lost] instead of saying that there
  /// are none.
  factory AppSettings.fromJson(Object? json) {
    if (json is! Map<String, dynamic>) return const AppSettings(lost: true);
    final language = json['language'];
    final dark = json['dark'];
    final minutes = json['auto_lock_minutes'];
    final bridges = json['bridges'];
    final readable =
        bridges == null ||
        (bridges is List && bridges.every((line) => line is String));
    return AppSettings(
      language: language == 'en' || language == 'ru'
          ? language as String
          : null,
      dark: dark == true,
      // A whole number, not `5.0`, which equals 5 and is no `int`.
      autoLockMinutes: minutes is int && autoLockChoices.contains(minutes)
          ? minutes
          : defaultAutoLock,
      bridges: bridges is List && readable
          ? bridges.cast<String>().toList()
          : const [],
      lost: !readable,
    );
  }

  /// The settings in [file]: the defaults if there is no such file, which
  /// is how every installation begins, and the defaults marked as [lost]
  /// if there is one and it cannot be read.
  static Future<AppSettings> load(String file) async {
    if (!await File(file).exists()) return const AppSettings();
    try {
      return AppSettings.fromJson(jsonDecode(await File(file).readAsString()));
    } on FileSystemException {
      return const AppSettings(lost: true);
    } on FormatException {
      return const AppSettings(lost: true);
    }
  }

  /// Writes the settings to [file], whole or not at all: to a file next
  /// to it first, which then takes its place. Written in place, a process
  /// ended in the middle would leave half a file, which reads as no
  /// settings, and with them the bridges would be gone unnoticed.
  Future<void> save(String file) async {
    await File(file).parent.create(recursive: true);
    final fresh = File('$file.new');
    await fresh.writeAsString(jsonEncode(toJson()), flush: true);
    await fresh.rename(file);
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
