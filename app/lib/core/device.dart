import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// What the app asks of the device it runs on, besides drawing and files:
/// to keep a screen out of pictures, to hold an address in the clipboard
/// for a while and no longer, and, where there is a way to, to hand an
/// address to another app and to read a code with the camera.
///
/// The app gives the screens the real one; tests give them one that only
/// pretends.
abstract class Device {
  /// How long a copied address stays in the clipboard.
  static const clipboardFor = Duration(minutes: 1);

  int _secrets = 0;
  String? _copied;
  Timer? _clear;

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
  /// there is seen to now.
  void resumed() {
    if (_copied != null && !(_clear?.isActive ?? false)) {
      unawaited(_clearCopied(inFront: true));
    }
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
/// with, nothing to share into, and no way to keep a window out of a
/// screenshot that the app could rely on.
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
