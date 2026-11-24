import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/app.dart';
import 'package:veil/core/backend.dart';
import 'package:veil_bridge/veil_bridge.dart';

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

/// A wallet that only pretends: no core, no files, no Tor.
class FakeBackend implements WalletBackend {
  FakeBackend({this.hasWallet = false, this.confirmed = false});

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
    bool? rebuilt,
    Problem? lastProblem,
  }) {
    final now = _state.value!;
    _state.value = SessionState(
      revision: ++_revision,
      phase: Phase.ready,
      phaseProblem: null,
      tor: tor ?? now.tor,
      node: null,
      sync: sync ?? now.sync,
      syncsEnded: now.syncsEnded,
      balanceTotal: total ?? now.balanceTotal,
      balanceUnlocked: unlocked ?? now.balanceUnlocked,
      balanceLocked: locked ?? now.balanceLocked,
      receiveAddress: address ?? now.receiveAddress,
      history: history ?? now.history,
      send: const SendStatus(SendPhase.idle),
      rebuiltFromKeys: rebuilt ?? now.rebuiltFromKeys,
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
  }) async {
    calls.add('start ${source.name}');
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

  @override
  Future<String?> recoveryPhrase() async =>
      _state.value == null ? null : testPhrase;

  @override
  Future<bool> phraseConfirmed() async => confirmed;

  @override
  Future<void> confirmPhrase() async {
    calls.add('confirm');
    confirmed = true;
  }

  @override
  Future<void> close() async {
    calls.add('close');
    _state.value = null;
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
  sync: const SyncStatus(SyncPhase.notYet),
  syncsEnded: 0,
  balanceTotal: '0.00',
  balanceUnlocked: '0.00',
  balanceLocked: '0.00',
  receiveAddress: ReceiveAddress(1, testAddresses[0]),
  history: const [],
  send: const SendStatus(SendPhase.idle),
  rebuiltFromKeys: false,
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

/// Whether the button labelled [label] can be pressed.
bool enabled(WidgetTester tester, String label) {
  final text = find.text(label.toUpperCase());
  final button = find.ancestor(
    of: text,
    matching: find.bySubtype<ButtonStyleButton>(),
  );
  return tester.widget<ButtonStyleButton>(button.first).onPressed != null;
}
