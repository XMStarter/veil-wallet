import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/app.dart';
import 'package:veil/core/device.dart';
import 'package:veil/core/backend.dart';

/// Loads the app's own typefaces, so that tests lay text out as the app
/// does and not in the test framework's stand-in font.
Future<void> loadFonts() async {
  for (final (family, file) in [
    ('Cormorant Garamond', 'CormorantGaramond.ttf'),
    ('Jost', 'Jost.ttf'),
    ('JetBrains Mono', 'JetBrainsMono.ttf'),
  ]) {
    final bytes = File('fonts/$file').readAsBytesSync();
    final loader = FontLoader(family)
      ..addFont(Future.value(ByteData.sublistView(bytes)));
    await loader.load();
  }
}

/// A phone-sized window, which is also the app's desktop window.
const windowSize = Size(420, 800);

const testPhrase =
    'unaware yard donate shallow slot sing oil oxygen loyal bench near hill '
    'surround forum execute lamp';
const testPassword = 'correct horse battery';

/// A device that only pretends to be a phone when told to, and notes what
/// was asked of it. Its clipboard is the system's, which tests stand in
/// for with [clipboard].
class FakeDevice extends ThisDevice {
  FakeDevice() : super(android: false);

  @override
  bool canShare = false;

  @override
  bool canScan = false;

  /// What was handed to other apps.
  final shared = <String>[];

  /// What the next scan reads; null for a user who gives up.
  String? scanReads;

  /// What was shown while scanning.
  final prompts = <String>[];

  /// Every time the guard on the screen was put up (true) or taken down.
  final guards = <bool>[];

  /// Whether the system would take no picture of the app right now.
  bool get guarded => guards.isNotEmpty && guards.last;

  @override
  Future<void> share(String text) async => shared.add(text);

  @override
  Future<String?> scan(String prompt) async {
    prompts.add(prompt);
    return scanReads;
  }

  @override
  Future<void> guardScreen(bool on) async => guards.add(on);
}

/// A wallet that only pretends: no core, no files, no Tor.
class FakeBackend implements WalletBackend {
  FakeBackend({this.hasWallet = false, this.confirmed = false});

  @override
  final FakeDevice device = FakeDevice();

  /// Whether the device already has a wallet.
  bool hasWallet;
  bool confirmed;

  /// The password of the wallet on the device, when there is one.
  String password = testPassword;

  /// What the next start fails with, if anything.
  Problem? failNext;

  /// What was asked of the backend, in order.
  final calls = <String>[];
  WalletSource? startedWith;
  String? restoredPhrase;
  int? restoredHeight;
  final _state = ValueNotifier<SessionState?>(null);
  int _revision = 0;

  /// How many addresses in a row may go unpaid before another is refused.
  static const unusedAddresses = 3;

  @override
  ValueListenable<SessionState?> get state => _state;

  /// Changes what the open wallet reports, as the core would: what is
  /// named changes, the rest stays.
  void report({
    TorStatus? tor,
    SyncStatus? sync,
    String? total,
    String? unlocked,
    String? locked,
    ReceiveAddress? address,
    List<Transfer>? history,
    List<String>? ownNodes,
    SendStatus? send,
    bool? rebuilt,
    Problem? notSaved,
    Problem? lastProblem,
  }) {
    final now = _state.value!;
    _state.value = SessionState(
      revision: ++_revision,
      phase: Phase.ready,
      phaseProblem: null,
      tor: tor ?? now.tor,
      node: null,
      ownNodes: ownNodes ?? now.ownNodes,
      sync: sync ?? now.sync,
      syncsEnded: now.syncsEnded,
      balanceTotal: total ?? now.balanceTotal,
      balanceUnlocked: unlocked ?? now.balanceUnlocked,
      balanceLocked: locked ?? now.balanceLocked,
      receiveAddress: address ?? now.receiveAddress,
      history: history ?? now.history,
      send: send ?? now.send,
      rebuiltFromKeys: rebuilt ?? now.rebuiltFromKeys,
      notSaved: notSaved ?? now.notSaved,
      // Cleared by the next command, as in the core.
      lastProblem: lastProblem,
    );
  }

  /// Changes what Tor is doing, as the core would report it.
  void tor(TorState state, {int percent = 100, bool rebuilt = false}) =>
      report(tor: TorStatus(state, percent, null), rebuilt: rebuilt);

  @override
  Future<bool> walletExists() async => hasWallet;

  @override
  Future<void> start(
    WalletSource source, {
    required String password,
    String? phrase,
    int? restoreHeight,
    bool withoutBridges = false,
  }) async {
    calls.add(
      'start ${source.name}${withoutBridges ? ' without bridges' : ''}',
    );
    // Bridges whose program has gone missing keep Tor from starting.
    if (!withoutBridges &&
        bridgesBroken &&
        _settings.value.bridges.isNotEmpty) {
      throw const StartFailure(
        Problem(ProblemKind.transportMissing, 'no program for obfs4'),
      );
    }
    await _work();
    final failure = failNext;
    failNext = null;
    if (failure != null) throw StartFailure(failure);
    if (source == WalletSource.open && password != this.password) {
      throw const StartFailure(
        Problem(ProblemKind.wrongPassword, 'wrong password'),
      );
    }
    startedWith = source;
    restoredPhrase = phrase;
    restoredHeight = restoreHeight;
    this.password = password;
    hasWallet = true;
    if (source == WalletSource.restore) confirmed = true;
    _state.value = _newSession(++_revision);
  }

  /// Whether the bridges in the settings have stopped working.
  bool bridgesBroken = false;
  final _settings = ValueNotifier<AppSettings>(const AppSettings());

  @override
  ValueListenable<AppSettings> get settings => _settings;

  @override
  Future<void> loadSettings() async {}

  @override
  Future<void> saveSettings(AppSettings settings) async {
    calls.add('save settings');
    _settings.value = settings;
  }

  @override
  Problem? checkBridges(List<String> lines) {
    for (final (number, line) in lines.indexed) {
      // The pretended core knows two transports and a plain bridge.
      final first = line.split(' ').first;
      if (first == 'snowflake') {
        return const Problem(
          ProblemKind.transportMissing,
          'none of its programs (snowflake-client) is installed',
        );
      }
      if (first != 'obfs4' && !first.contains(':')) {
        return Problem(
          ProblemKind.invalidBridge,
          'bridge line ${number + 1} is not a bridge',
        );
      }
    }
    return null;
  }

  @override
  Problem? setNodes(List<String> nodes) {
    calls.add('set nodes ${nodes.join(',')}');
    if (nodes.any((node) => !node.contains('.onion:'))) {
      return const Problem(ProblemKind.invalidNode, 'not a node');
    }
    report(ownNodes: nodes);
    return null;
  }

  /// Thrown by the next call that waits for the wallet, whichever it is:
  /// a failure of a kind nobody foresaw.
  Object? breakNext;

  /// How long the wallet takes over a call that waits for it. The real
  /// one takes long enough for the screen to be drawn meanwhile.
  Duration takes = Duration.zero;

  Future<void> _work() async {
    if (takes > Duration.zero) await Future<void>.delayed(takes);
    final broken = breakNext;
    breakNext = null;
    if (broken != null) throw broken;
  }

  /// The wallet cannot be asked about its password: it is busy with a
  /// transaction, as the real one can be.
  bool busy = false;

  @override
  Future<bool?> isPassword(String password) async {
    await _work();
    if (busy) return null;
    return _state.value == null ? null : password == this.password;
  }

  @override
  Future<Problem?> changePassword(String old, String next) async {
    await _work();
    calls.add('change password');
    if (old != password) {
      return const Problem(ProblemKind.wrongPassword, 'wrong password');
    }
    password = next;
    return null;
  }

  @override
  Future<void> removeWallet() async {
    calls.add('remove wallet');
    _state.value = null;
    hasWallet = false;
    confirmed = false;
  }

  @override
  void newAddress() {
    calls.add('new address');
    final index = _state.value!.receiveAddress!.index;
    if (index >= unusedAddresses) {
      report(
        lastProblem: const Problem(
          ProblemKind.tooManyUnusedAddresses,
          'too many unused addresses',
        ),
      );
    } else {
      report(address: ReceiveAddress(index + 1, testAddresses[index]));
    }
  }

  /// What the next send that is confirmed ends as.
  SendPhase sendEndsAs = SendPhase.sent;

  /// What the wallet charges for a transaction.
  static const fee = '0.00003066';

  /// The amount a send of everything comes to, and its total.
  static const everythingAmount = '10.49996934';

  /// The wallet's funds are in so many small coins that a payment would
  /// take more than one transaction.
  bool manySmallCoins = false;

  @override
  Problem? prepareSend(String to, String amount) {
    calls.add('prepare $amount to $to');
    if (amount != everything && double.tryParse(amount) == null) {
      return const Problem(ProblemKind.invalidAmount, 'not an amount');
    }
    // The pretended wallet knows its test addresses and nothing else.
    if (!testAddresses.contains(to)) {
      report(
        send: const SendStatus(
          SendPhase.failed,
          problem: Problem(ProblemKind.invalidAddress, 'not an address'),
        ),
      );
    } else if (amount != everything && double.parse(amount) == 0) {
      report(
        send: const SendStatus(
          SendPhase.failed,
          problem: Problem(ProblemKind.zeroAmount, 'zero'),
        ),
      );
    } else if (manySmallCoins) {
      report(
        send: const SendStatus(
          SendPhase.failed,
          problem: Problem(ProblemKind.severalTransactions, 'needs 2'),
        ),
      );
    } else if (amount != everything && double.parse(amount) > 10) {
      report(
        send: const SendStatus(
          SendPhase.failed,
          problem: Problem(ProblemKind.notEnoughFunds, 'not enough money'),
        ),
      );
    } else {
      // Amounts as the core would write them; the test wallet's sums are
      // fixed, because nothing on this side calculates with money.
      final all = amount == everything;
      report(
        send: SendStatus(
          SendPhase.review,
          to: to,
          amount: all ? everythingAmount : '1.50',
          fee: fee,
          total: all ? '10.50' : '1.50003066',
          sourceAddresses: all ? const [0, 1, 3] : const [1],
        ),
      );
    }
    return null;
  }

  @override
  void confirmSend() {
    calls.add('confirm send');
    report(
      send: switch (sendEndsAs) {
        SendPhase.sent => SendStatus(SendPhase.sent, txids: ['a' * 64]),
        SendPhase.outcomeUnknown => SendStatus(
          SendPhase.outcomeUnknown,
          txids: ['a' * 64],
          settled: false,
        ),
        _ => const SendStatus(
          SendPhase.failed,
          problem: Problem(ProblemKind.other, 'the node refused it'),
        ),
      },
    );
  }

  @override
  void cancelSend() {
    calls.add('cancel send');
    final send = _state.value?.send;
    if (send == null) return;
    // As in the core: only what has gone nowhere is cancelled. A send
    // that went out, or may have, stays as it is.
    switch (send.phase) {
      case SendPhase.review || SendPhase.preparing || SendPhase.failed:
        report(send: const SendStatus(SendPhase.idle));
      case SendPhase.outcomeUnknown when send.settled != true:
        report(
          send: send,
          lastProblem: const Problem(ProblemKind.notNow, 'not known yet'),
        );
      default:
        break;
    }
  }

  @override
  void clearSend() {
    calls.add('clear send');
    final send = _state.value?.send;
    if (send == null) return;
    // As in the core: an outcome that is unknown and not settled stays.
    if (send.phase == SendPhase.outcomeUnknown && send.settled != true) {
      report(
        send: send,
        lastProblem: const Problem(ProblemKind.notNow, 'not known yet'),
      );
    } else {
      report(send: const SendStatus(SendPhase.idle));
    }
  }

  @override
  void sync() {
    calls.add('sync');
    if (_state.value != null) {
      report(sync: const SyncStatus(SyncPhase.running));
    }
  }

  /// The wallet gives no recovery phrase when asked.
  bool noPhrase = false;

  @override
  Future<String?> recoveryPhrase() async =>
      _state.value == null || noPhrase ? null : testPhrase;

  @override
  Future<bool> phraseConfirmed() async => confirmed;

  @override
  Future<void> confirmPhrase() async {
    calls.add('confirm');
    confirmed = true;
  }

  /// What closing reports, if the wallet cannot be saved then.
  Problem? notSavedOnClose;

  /// Thrown by the next close, after the wallet is gone from the state:
  /// a failure of a kind nobody foresaw.
  Object? breakClose;

  @override
  Future<Problem?> close() async {
    calls.add('close');
    _state.value = null;
    final broken = breakClose;
    breakClose = null;
    if (broken != null) throw broken;
    return notSavedOnClose;
  }
}

/// A wallet that has just been opened: Tor is connecting, nothing has
/// been synchronised, and the first address is the one to receive at.
SessionState _newSession(int revision) => SessionState(
  revision: revision,
  phase: Phase.ready,
  phaseProblem: null,
  tor: const TorStatus(TorState.bootstrapping, 15, null),
  node: null,
  ownNodes: const [],
  sync: const SyncStatus(SyncPhase.notYet),
  syncsEnded: 0,
  balanceTotal: '0.00',
  balanceUnlocked: '0.00',
  balanceLocked: '0.00',
  receiveAddress: ReceiveAddress(1, testAddresses[0]),
  history: const [],
  send: const SendStatus(SendPhase.idle),
  rebuiltFromKeys: false,
  notSaved: null,
  lastProblem: null,
);

/// Addresses of the right shape for the test network, to draw and to tell
/// apart. Nobody holds their keys.
const testAddresses = [
  '73a4nWuvkYoYoksGurDjKZQcZkmaxLaKbbeiKzHnMmqKivrCzq5Q2JtJG1UZNZFqLPbQ3MiXCk2Q5bdwdUNSr7X9QrPubkn',
  '7BnERTpvL5MbCLtj5n9No7J5oE5hHiB3tVCK5cjSvCsYWD2WRJLFuWeKTLiXo5QJqt2ZwUaLy2Vh1Ad51K7FNgqcHgjW85o',
  '77xa6Dha7kzCQuvmd8iB5VYoMkdenwCNRU9khGhExXQ8KLL3z1N1ZATBD1sFPenyHWT9cm4fVFnCAUApY53peuoZFtwZiw5',
];

/// A payment as the core reports it.
Transfer transfer({
  required Direction direction,
  required String amount,
  int confirmations = 100,
  bool pending = false,
  bool failed = false,
  int timestamp = 1795400000,
}) => Transfer(
  txid: '${direction.name}-$amount-$timestamp',
  direction: direction,
  amount: amount,
  fee: direction == Direction.outgoing ? '0.00003066' : '0.00',
  blockHeight: pending ? 0 : 2256000,
  confirmations: confirmations,
  timestamp: timestamp,
  pending: pending,
  failed: failed,
  addressIndices: direction == Direction.incoming ? const [1] : const [],
);

/// Starts the app in [language] on top of [backend], in a window of
/// [size].
Future<void> pumpApp(
  WidgetTester tester,
  String language, {
  WalletBackend? backend,
  Size size = windowSize,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    VeilApp(backend: backend ?? FakeBackend(), locale: Locale(language)),
  );
  await tester.pumpAndSettle();
}

/// Types [text] into the field labelled [label].
Future<void> type(WidgetTester tester, String label, String text) async {
  final field = find.descendant(
    of: find
        .ancestor(of: find.text(label), matching: find.byType(Column))
        .first,
    matching: find.byType(TextField),
  );
  await tester.enterText(field.first, text);
  await tester.pumpAndSettle();
}

/// Presses the button labelled [label], which is shown in uppercase.
Future<void> press(WidgetTester tester, String label) async {
  final button = find.text(label.toUpperCase());
  await tester.ensureVisible(button);
  await tester.tap(button);
  await tester.pumpAndSettle();
}

/// Presses the button labelled [label] and waits for a wallet that takes
/// its time ([FakeBackend.takes]) to answer, with the screen drawn while
/// it works, as it is in the app.
Future<void> pressAndWait(WidgetTester tester, String label) async {
  final button = find.text(label.toUpperCase());
  await tester.ensureVisible(button);
  await tester.tap(button);
  await tester.pump();
  await tester.pump(const Duration(seconds: 1));
  await tester.pumpAndSettle();
}

/// Types [text] on the keyboard as it is, without choosing a field first:
/// it goes to whichever field has the keyboard, and nowhere if none has.
Future<void> typeOn(WidgetTester tester, String text) async {
  expect(
    tester.testTextInput.hasAnyClients,
    isTrue,
    reason: 'no field has the keyboard',
  );
  tester.testTextInput.enterText(text);
  await tester.pumpAndSettle();
}

/// Whether everything typed into the only field on the screen is marked,
/// so that the next key replaces it.
bool allMarked(WidgetTester tester) {
  final field = tester.widget<TextField>(find.byType(TextField)).controller!;
  return field.text.isNotEmpty &&
      field.selection.start == 0 &&
      field.selection.end == field.text.length;
}

/// Whether the button labelled [label] can be pressed.
bool enabled(WidgetTester tester, String label) {
  final text = find.text(label.toUpperCase());
  final button = find.ancestor(
    of: text,
    matching: find.bySubtype<ButtonStyleButton>(),
  );
  return tester.widget<ButtonStyleButton>(button.first).onPressed != null;
}

/// Opens the wallet on the device and lands on Home.
Future<FakeBackend> openWallet(WidgetTester tester, String language) async {
  final backend = FakeBackend(hasWallet: true, confirmed: true);
  await pumpApp(tester, language, backend: backend);
  await type(tester, language == 'ru' ? 'Пароль' : 'Password', testPassword);
  await press(tester, language == 'ru' ? 'Открыть кошелёк' : 'Open wallet');
  return backend;
}

/// Stands in for the system's clipboard, holding [text] to begin with,
/// and returns everything that is put into it, in order. It holds what
/// was put into it last, as a clipboard does; an empty text is a
/// clipboard that was cleared.
List<String> clipboard(WidgetTester tester, {String? text}) {
  final copied = <String>[];
  var held = text;
  final messenger = tester.binding.defaultBinaryMessenger;
  messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
    if (call.method == 'Clipboard.setData') {
      held = (call.arguments as Map<Object?, Object?>)['text']! as String;
      copied.add(held!);
    }
    if (call.method == 'Clipboard.getData') {
      return held == null || held!.isEmpty
          ? null
          : <String, Object?>{'text': held};
    }
    return null;
  });
  addTearDown(
    () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
  );
  return copied;
}

/// Whether [text] is on the screen, in plain text or inside styled text.
Finder shown(String text) => find.textContaining(text, findRichText: true);

/// Presses the button labelled [label] and draws one frame, for screens
/// that then show progress, which never comes to rest.
Future<void> pressOnce(WidgetTester tester, String label) async {
  final button = find.text(label.toUpperCase());
  await tester.ensureVisible(button);
  await tester.tap(button);
  await tester.pump();
}
