// The native library, loaded and called for real.
//
// Needs the core built as a shared library (`cargo build -p veil_core` in
// native/, or VEIL_CORE_LIB) and is skipped without it. Nothing here opens
// a wallet, so Tor is never started and nothing leaves the machine.

import 'dart:io';

import 'package:test/test.dart';
import 'package:veil_bridge/veil_bridge.dart';

String? coreLibrary() {
  final named = Platform.environment['VEIL_CORE_LIB'];
  if (named != null) return named;
  final file = Platform.isWindows ? 'veil_core.dll' : 'libveil_core.so';
  final built = File('../../native/target/debug/$file');
  return built.existsSync() ? built.path : null;
}

void main() {
  final library = coreLibrary();
  final skip = library == null
      ? 'the core is not built as a shared library'
      : null;
  late VeilCore core;
  late Directory dir;

  setUp(() {
    core = VeilCore.open(library!);
    dir = Directory.systemTemp.createTempSync('veil-bridge-');
  });
  tearDown(() => dir.deleteSync(recursive: true));

  SessionConfig config({
    List<String> nodes = const [],
    String? walletLibrary,
  }) => SessionConfig(
    walletLibrary: walletLibrary ?? 'no-such-library',
    walletPath: '${dir.path}/wallet',
    torStateDir: '${dir.path}/tor-state',
    torCacheDir: '${dir.path}/tor-cache',
    nodes: nodes,
    source: WalletSource.open,
  );

  test('a request the core cannot read is refused with the reason', () {
    expect(
      () =>
          core.start(config(nodes: ['node.example.com:38089']), password: 'p'),
      throwsA(
        isA<VeilException>().having(
          (e) => e.problem.kind,
          'kind',
          ProblemKind.invalidNode,
        ),
      ),
    );
    expect(
      () => core.start(
        SessionConfig(
          walletLibrary: 'a',
          walletPath: 'b',
          torStateDir: 'c',
          torCacheDir: 'd',
          nodes: const [],
          source: WalletSource.restore,
        ),
        password: 'p',
        phrase: 'three words only',
      ),
      throwsA(isA<VeilException>()),
    );
  }, skip: skip);

  test('bridges are checked without starting anything', () {
    const plain = '192.0.2.55:443 316E643333645F6D79216558614D3931657A5F5F';
    expect(core.checkBridges(const []), isNull);
    expect(core.checkBridges(const [plain, '', '# a comment']), isNull);
    expect(
      core.checkBridges(const [plain, 'nonsense'])?.kind,
      ProblemKind.invalidBridge,
    );
    expect(
      core.checkBridges(const ['conjure $plain'])?.kind,
      ProblemKind.transportMissing,
    );
    // A start with bridges that are not bridges is refused the same way.
    final withBridges = SessionConfig(
      walletLibrary: 'a',
      walletPath: 'b',
      torStateDir: 'c',
      torCacheDir: 'd',
      bridges: const ['nonsense'],
      source: WalletSource.open,
    );
    expect(
      () => core.start(withBridges, password: 'p'),
      throwsA(
        isA<VeilException>().having(
          (e) => e.problem.kind,
          'kind',
          ProblemKind.invalidBridge,
        ),
      ),
    );
  }, skip: skip);

  test('a session that cannot open its wallet says so and closes', () async {
    // The wallet library named here does not exist, so the session fails
    // before it opens a wallet or starts Tor.
    final session = core.start(config(), password: 'p');
    expect(session.address, isNot(0));
    var state = session.state();
    for (var tries = 0; state.phase == Phase.opening && tries < 200; tries++) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
      state = session.state();
    }
    expect(state.phase, Phase.failed);
    expect(state.phaseProblem, isNotNull);
    expect(session.revision, state.revision);
    expect(state.balanceTotal, '0.00');
    expect(Directory('${dir.path}/tor-state').existsSync(), isFalse);

    // Commands to a session that never opened are taken and do nothing.
    session.sync();
    session.newAddress();
    session.cancelSend();
    session.clearSend();
    // An amount that is not a number is refused on the spot.
    expect(
      () => session.prepareSend('7abc', 'much'),
      throwsA(
        isA<VeilException>().having(
          (e) => e.problem.kind,
          'kind',
          ProblemKind.invalidAmount,
        ),
      ),
    );
    expect(session.recoveryPhrase(), isNull);
    // Nor can a session without a wallet say anything about its password.
    expect(session.isPassword('p'), isNull);
    expect(session.changePassword('p', 'q')?.kind, ProblemKind.notNow);
    expect(
      () => session.setNodes(['node.example.com:38089']),
      throwsA(
        isA<VeilException>().having(
          (e) => e.problem.kind,
          'kind',
          ProblemKind.invalidNode,
        ),
      ),
    );

    // The same session, reached by its address as another isolate would.
    final again = VeilSession.fromAddress(core, session.address);
    expect(again.state().phase, Phase.failed);

    expect(session.close().phase, Phase.failed);
    expect(session.revision, 0);
    expect(session.state, throwsStateError);
  }, skip: skip);
}
