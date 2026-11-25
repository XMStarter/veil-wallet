import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// What a phone shows while a wallet is open, in the place where it lists
/// what is running: the notice of the service that keeps the app alive
/// when it is not in front. It names no amount and no address.
@immutable
class Presence {
  const Presence({
    required this.kind,
    required this.title,
    required this.text,
    required this.busy,
  });

  /// What the system's settings call this kind of notice.
  final String kind;

  final String title;
  final String text;

  /// Whether the wallet is at work on something that a sleeping phone
  /// would cut off: connecting, synchronising, or sending. Only then is
  /// the phone kept from sleeping.
  final bool busy;

  @override
  bool operator ==(Object other) =>
      other is Presence &&
      other.kind == kind &&
      other.title == title &&
      other.text == text &&
      other.busy == busy;

  @override
  int get hashCode => Object.hash(kind, title, text, busy);

  @override
  String toString() => 'Presence($text, busy: $busy)';
}

/// What the app asks of the device it runs on, besides drawing and files:
/// to keep a screen out of pictures, to hold an address in the clipboard
/// for a while and no longer, to keep the app running while a wallet is
/// open, and, where there is a way to, to hand an address to another app
/// and to read a code with the camera.
///
/// The app gives the screens the real one; tests give them one that only
/// pretends.
abstract class Device {
  /// How long a copied address stays in the clipboard.
  static const clipboardFor = Duration(minutes: 1);

  /// How often a wallet that is still busy says so again. The device
  /// keeps itself awake for a limited time after each telling
  /// (`WalletService.AWAKE_FOR` on Android, which has to be longer), so
  /// that a wallet that has stopped reporting does not hold a phone awake
  /// for good.
  static const busyAgainAfter = Duration(minutes: 1);

  int _secrets = 0;
  String? _copied;
  Timer? _clear;
  Presence? _presence;
  DateTime? _toldAt;

  /// Whether the system is set to stop this app when it is not in front,
  /// so that an open wallet does nothing there. Looked at when a wallet
  /// is opened and whenever the app comes to the front.
  ValueListenable<bool> get stoppedInBackground => _stopped;
  final _stopped = ValueNotifier<bool>(false);

  /// The time by the device's clock, which goes on while a phone sleeps.
  /// A timer does not: its time stands still then.
  DateTime now() => DateTime.now();

  /// A wallet is open, with what the system is to show about it, or none
  /// is ([presence] null). While one is open a phone keeps the app
  /// running when it is not in front, and awake while the wallet is
  /// [Presence.busy]. Told as often as the state changes; passed on when
  /// something is new, or when it is time to say "still busy" again.
  void walletOpen(Presence? presence) {
    final now = this.now();
    final told = _toldAt;
    final stillBusy =
        presence != null &&
        presence.busy &&
        told != null &&
        now.difference(told) >= busyAgainAfter;
    if (presence == _presence && !stillBusy) return;
    final opened = _presence == null && presence != null;
    _presence = presence;
    _toldAt = now;
    _tell(presence);
    if (opened) unawaited(_lookForRestriction());
  }

  void _tell(Presence? presence) {
    // What cannot be shown or kept running is no reason for the wallet
    // to fail: it works for as long as the system lets the app run.
    unawaited(keepRunning(presence).then<void>((_) {}, onError: (_) {}));
  }

  Future<void> _lookForRestriction() async {
    try {
      _stopped.value = await restricted();
    } on PlatformException {
      // Not known; nothing is said.
    } on MissingPluginException {
      // The same.
    }
  }

  /// Whether [share] leads anywhere on this device.
  bool get canShare;

  /// Whether [scan] can read anything on this device.
  bool get canScan;

  /// A screen that shows or takes a recovery phrase has come up. Until
  /// the last such screen is gone again ([secretGone]) the system takes
  /// no picture of the app: no screenshot, no recording, and no preview
  /// among the recent apps.
  void secretShown() {
    if (_secrets++ == 0) unawaited(guardScreen(true));
  }

  /// The screen of a [secretShown] is gone.
  void secretGone() {
    if (--_secrets == 0) unawaited(guardScreen(false));
  }

  /// Puts [text] into the clipboard, and takes it out again after
  /// [clipboardFor] if it is still what the clipboard holds. Whoever has
  /// copied something else in the meantime keeps it.
  Future<void> copy(String text) async {
    await writeClipboard(text);
    _copied = text;
    _clear?.cancel();
    _clear = Timer(clipboardFor, () => unawaited(_clearCopied()));
  }

  /// The app is in front again. A phone does not let an app in the
  /// background look into the clipboard, so a copy whose time ran out
  /// there is seen to now. And a phone that had stopped keeping the app
  /// running, which it does after some hours, is asked again.
  void resumed() {
    if (_copied != null && !(_clear?.isActive ?? false)) {
      unawaited(_clearCopied(inFront: true));
    }
    final presence = _presence;
    if (presence != null) {
      _toldAt = now();
      _tell(presence);
    }
    unawaited(_lookForRestriction());
  }

  Future<void> _clearCopied({bool inFront = false}) async {
    final text = _copied;
    if (text == null) return;
    String? held;
    try {
      held = await readClipboard();
    } on PlatformException {
      held = null;
    } on MissingPluginException {
      held = null;
    }
    if (held == null) {
      // Nothing there, or nothing the app may see. In the background the
      // second is likely, and the matter waits for [resumed]; in front
      // there is nothing to clear.
      if (inFront) _copied = null;
      return;
    }
    _copied = null;
    if (held == text) await eraseClipboard();
  }

  /// Hands [text] to whichever app the user picks.
  Future<void> share(String text);

  /// Reads a code with the camera and returns what it says; null if the
  /// user gave up or nothing was read. [prompt] is shown while looking.
  Future<String?> scan(String prompt);

  @protected
  Future<void> guardScreen(bool on);

  /// Keeps the app running while it is not in front and shows [presence]
  /// meanwhile, or stops doing so (null).
  @protected
  Future<void> keepRunning(Presence? presence);

  /// Whether the system stops this app when it is not in front.
  @protected
  Future<bool> restricted();

  @protected
  Future<void> writeClipboard(String text);

  /// What the clipboard holds as text; null if nothing, or nothing the
  /// app may see.
  @protected
  Future<String?> readClipboard();

  @protected
  Future<void> eraseClipboard();
}

/// The device the app runs on. On Android its questions go to the app's
/// Android side (`MainActivity`); a desktop has no camera to read codes
/// with, nothing to share into, no way to keep a window out of a
/// screenshot that the app could rely on, and it does not stop a program
/// whose window is not in front.
class ThisDevice extends Device {
  ThisDevice({bool? android}) : _android = android ?? Platform.isAndroid;

  final bool _android;
  static const _channel = MethodChannel('veil/device');

  @override
  bool get canShare => _android;

  @override
  bool get canScan => _android;

  @override
  Future<void> share(String text) async {
    if (_android) await _channel.invokeMethod<void>('share', text);
  }

  @override
  Future<String?> scan(String prompt) async =>
      _android ? _channel.invokeMethod<String>('scan', prompt) : null;

  @override
  Future<void> guardScreen(bool on) async {
    if (_android) await _channel.invokeMethod<void>('secure', on);
  }

  @override
  Future<void> keepRunning(Presence? presence) async {
    if (!_android) return;
    await _channel.invokeMethod<void>(
      'running',
      presence == null
          ? null
          : {
              'kind': presence.kind,
              'title': presence.title,
              'text': presence.text,
              'busy': presence.busy,
            },
    );
  }

  @override
  Future<bool> restricted() async =>
      _android && (await _channel.invokeMethod<bool>('restricted') ?? false);

  @override
  Future<void> writeClipboard(String text) =>
      Clipboard.setData(ClipboardData(text: text));

  @override
  Future<String?> readClipboard() async =>
      (await Clipboard.getData(Clipboard.kTextPlain))?.text;

  @override
  Future<void> eraseClipboard() async {
    if (_android) {
      await _channel.invokeMethod<void>('clearClipboard');
    } else {
      await Clipboard.setData(const ClipboardData(text: ''));
    }
  }
}
