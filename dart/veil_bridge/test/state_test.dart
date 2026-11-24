// The state as the core writes it. The files read here are written by the
// core's own tests; see native/veil_core/src/tests/ffi.rs.

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';
import 'package:veil_bridge/veil_bridge.dart';

SessionState fixture(String name) {
  final file = File('../../native/veil_core/tests/fixtures/$name');
  return SessionState.fromJson(
    jsonDecode(file.readAsStringSync()) as Map<String, dynamic>,
  );
}

void main() {
  test('a state with something in every field is read whole', () {
    final state = fixture('state-busy.json');
    expect(state.revision, 42);
    expect(state.phase, Phase.ready);
    expect(state.phaseProblem, isNull);
    expect(state.tor.state, TorState.ready);
    expect(state.tor.percent, 100);
    expect(state.tor.message, isNull);
    expect(state.node, endsWith('.onion:38089'));
    expect(state.sync.phase, SyncPhase.running);
    expect(state.sync.walletHeight, 2250000);
    expect(state.sync.nodeHeight, 2256644);
    expect(state.sync.blocksLeft, 6644);
    expect(state.syncsEnded, 3);
    expect(state.ownNodes, hasLength(2));
    expect(state.ownNodes.first, state.node);
    // Amounts stay the text the core wrote, to the last digit.
    expect(state.balanceTotal, '12.345678901234');
    expect(state.balanceUnlocked, '10.50');
    expect(state.balanceLocked, '1.845678901234');
    expect(state.receiveAddress!.index, 3);
    expect(state.receiveAddress!.address, hasLength(95));
    expect(state.rebuiltFromKeys, isFalse);
    expect(state.notSaved, isNull);
    expect(state.lastProblem!.kind, ProblemKind.notNow);

    expect(state.history, hasLength(2));
    final received = state.history[0];
    expect(received.direction, Direction.incoming);
    expect(received.amount, '12.50');
    expect(received.fee, '0.00');
    expect(received.confirmations, 644);
    expect(received.pending, isFalse);
    expect(received.addressIndices, [1, 2]);
    final sent = state.history[1];
    expect(sent.direction, Direction.outgoing);
    expect(sent.amount, '0.15');
    expect(sent.fee, '0.00003066');
    expect(sent.blockHeight, 0);
    expect(sent.pending, isTrue);
    expect(sent.failed, isFalse);
    expect(sent.addressIndices, isEmpty);

    expect(state.send.phase, SendPhase.review);
    expect(state.send.to, state.receiveAddress!.address);
    expect(state.send.amount, '1.50');
    expect(state.send.fee, '0.00003066');
    expect(state.send.total, '1.50003066');
    expect(state.send.sourceAddresses, [1, 3]);
  });

  test('a state where things went wrong says what and why', () {
    final state = fixture('state-troubled.json');
    expect(state.phase, Phase.failed);
    expect(state.phaseProblem!.kind, ProblemKind.wrongPassword);
    expect(state.tor.state, TorState.bootstrapping);
    expect(state.tor.percent, 45);
    expect(state.tor.message, isNotEmpty);
    expect(state.node, isNull);
    expect(state.sync.phase, SyncPhase.failed);
    expect(state.sync.problem!.kind, ProblemKind.torNotReady);
    expect(state.sync.blocksLeft, isNull);
    expect(state.receiveAddress, isNull);
    expect(state.history, isEmpty);
    expect(state.send.phase, SendPhase.outcomeUnknown);
    expect(state.send.txids, ['c' * 64]);
    expect(state.send.settled, isFalse);
    expect(state.rebuiltFromKeys, isTrue);
    expect(state.notSaved!.kind, ProblemKind.other);
    expect(state.notSaved!.detail, contains('wallet.new'));
    expect(state.lastProblem, isNull);
  });

  test('a name this side does not know is an error', () {
    Map<String, dynamic> busy() => jsonDecode(
      File('../../native/veil_core/tests/fixtures/state-busy.json')
          .readAsStringSync(),
    ) as Map<String, dynamic>;
    final phase = busy()..['phase'] = {'name': 'sleeping'};
    expect(() => SessionState.fromJson(phase), throwsFormatException);
    final problem = busy()
      ..['last_problem'] = {'kind': 'bad_luck', 'detail': ''};
    expect(() => SessionState.fromJson(problem), throwsFormatException);
  });

  test('a configuration is written the way the core reads it', () {
    const config = SessionConfig(
      walletLibrary: '/lib/wallet.so',
      walletPath: '/data/wallet',
      torStateDir: '/data/tor/state',
      torCacheDir: '/data/tor/cache',
      nodes: ['abc.onion:38089'],
      bridges: ['obfs4 192.0.2.55:443 AA cert=x iat-mode=0'],
      source: WalletSource.restore,
      restoreHeight: 1234567,
      syncEvery: Duration(minutes: 2),
    );
    expect(config.toJson(), {
      'network': 'stagenet',
      'source': 'restore',
      'wallet_library': '/lib/wallet.so',
      'wallet_path': '/data/wallet',
      'tor': {'state_dir': '/data/tor/state', 'cache_dir': '/data/tor/cache'},
      'nodes': ['abc.onion:38089'],
      'bridges': ['obfs4 192.0.2.55:443 AA cert=x iat-mode=0'],
      'restore_height': 1234567,
      'sync_every_seconds': 120,
    });
    const plain = SessionConfig(
      walletLibrary: 'a',
      walletPath: 'b',
      torStateDir: 'c',
      torCacheDir: 'd',
      nodes: [],
      source: WalletSource.create,
    );
    expect(plain.toJson().keys, isNot(contains('restore_height')));
    expect(plain.toJson().keys, isNot(contains('sync_every_seconds')));
  });
}
