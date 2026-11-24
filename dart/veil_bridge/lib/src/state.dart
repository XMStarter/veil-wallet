/// The state of a session, as the core hands it over in JSON.
///
/// The format is pinned by files the core's own tests write
/// (`native/veil_core/tests/fixtures`); this package's tests read the same
/// files. A name this side does not know is an error, not something to
/// skip: it means the two sides have come apart.
library;

/// What went wrong, as a kind for the interface to put into its own words
/// and a detail in English for a log.
class Problem {
  const Problem(this.kind, this.detail);

  /// One of [ProblemKind.all].
  final String kind;
  final String detail;

  static Problem? maybe(Object? json) {
    if (json == null) return null;
    final map = json as Map<String, dynamic>;
    final kind = map['kind'] as String;
    if (!ProblemKind.all.contains(kind)) {
      throw FormatException('unknown kind of problem: $kind');
    }
    return Problem(kind, map['detail'] as String);
  }

  @override
  String toString() => '$kind: $detail';
}

/// The kinds of [Problem] the core reports.
abstract final class ProblemKind {
  static const wrongPassword = 'wrong_password';
  static const walletExists = 'wallet_exists';
  static const walletInUse = 'wallet_in_use';
  static const invalidPhrase = 'invalid_phrase';
  static const torNotReady = 'tor_not_ready';
  static const torFailed = 'tor_failed';
  static const torLost = 'tor_lost';
  static const noNode = 'no_node';
  static const invalidAddress = 'invalid_address';
  static const invalidAmount = 'invalid_amount';
  static const invalidNode = 'invalid_node';
  static const invalidBridge = 'invalid_bridge';
  static const transportMissing = 'transport_missing';
  static const zeroAmount = 'zero_amount';
  static const notEnoughFunds = 'not_enough_funds';
  static const severalTransactions = 'several_transactions';
  static const tooManyUnusedAddresses = 'too_many_unused_addresses';
  static const sendOutcomeUnknown = 'send_outcome_unknown';
  static const notNow = 'not_now';
  static const badRequest = 'bad_request';
  static const other = 'other';

  static const all = {
    wrongPassword,
    walletExists,
    walletInUse,
    invalidPhrase,
    torNotReady,
    torFailed,
    torLost,
    noNode,
    invalidAddress,
    invalidAmount,
    invalidNode,
    invalidBridge,
    transportMissing,
    zeroAmount,
    notEnoughFunds,
    severalTransactions,
    tooManyUnusedAddresses,
    sendOutcomeUnknown,
    notNow,
    badRequest,
    other,
  };
}

T _named<T extends Enum>(List<T> values, Object? name, String what) {
  for (final value in values) {
    if (_snake(value.name) == name) return value;
  }
  throw FormatException('unknown $what: $name');
}

/// `outcomeUnknown` to `outcome_unknown`, the way the core writes names.
String _snake(String camel) => camel.replaceAllMapped(
  RegExp('[A-Z]'),
  (match) => '_${match[0]!.toLowerCase()}',
);

/// Where a session is in its life.
enum Phase { opening, ready, closed, failed }

/// What Tor is doing. [lost]: it was connected and cannot reach its
/// network now; it goes on trying, and the core says [ready] again once a
/// connection gets through.
enum TorState { bootstrapping, ready, failed, lost }

class TorStatus {
  const TorStatus(this.state, this.percent, this.message);

  final TorState state;

  /// Bootstrap progress, 0 to 100.
  final int percent;

  /// Why Tor is stuck, failed or lost, when known.
  final String? message;
}

enum SyncPhase { notYet, running, done, failed }

/// The last synchronisation.
class SyncStatus {
  const SyncStatus(
    this.phase, {
    this.walletHeight,
    this.nodeHeight,
    this.problem,
  });

  final SyncPhase phase;

  /// How far the wallet has scanned, once the first blocks have arrived.
  final int? walletHeight;
  final int? nodeHeight;

  /// Why it failed, when [phase] is [SyncPhase.failed].
  final Problem? problem;

  int? get blocksLeft {
    final wallet = walletHeight, node = nodeHeight;
    if (wallet == null || node == null) return null;
    return node > wallet ? node - wallet : 0;
  }
}

class ReceiveAddress {
  const ReceiveAddress(this.index, this.address);

  /// Position of the address in the wallet, starting at 1.
  final int index;
  final String address;
}

enum Direction { incoming, outgoing }

/// One entry of the wallet's history.
class Transfer {
  const Transfer({
    required this.txid,
    required this.direction,
    required this.amount,
    required this.fee,
    required this.blockHeight,
    required this.confirmations,
    required this.timestamp,
    required this.pending,
    required this.failed,
    required this.addressIndices,
  });

  final String txid;
  final Direction direction;
  final String amount;
  final String fee;

  /// Zero while the transaction is not in a block.
  final int blockHeight;
  final int confirmations;

  /// Unix time in seconds.
  final int timestamp;
  final bool pending;
  final bool failed;

  /// Receiving addresses involved, by index. Empty for outgoing transfers.
  final List<int> addressIndices;
}

enum SendPhase {
  idle,
  preparing,
  review,
  sending,
  sent,
  outcomeUnknown,
  failed,
}

/// A send, from the form to its outcome.
class SendStatus {
  const SendStatus(
    this.phase, {
    this.to,
    this.amount,
    this.fee,
    this.total,
    this.sourceAddresses,
    this.txids,
    this.settled,
    this.problem,
  });

  final SendPhase phase;

  /// The transaction under review: where it goes, how much, and the fee.
  final String? to;
  final String? amount;
  final String? fee;

  /// The amount and the fee together: what leaves the wallet.
  final String? total;

  /// The wallet's own addresses, by index, whose coins the transaction
  /// under review spends. More than one means coins from different
  /// addresses are spent together.
  final List<int>? sourceAddresses;

  /// The transactions that were sent, when [phase] is [SendPhase.sent], or
  /// that may have been, when it is [SendPhase.outcomeUnknown].
  final List<String>? txids;

  /// With [SendPhase.outcomeUnknown]: false while the core is still
  /// looking for the transaction and takes no other send; true once a
  /// synchronisation well after the attempt has not shown it. The core
  /// turns the phase into [SendPhase.sent] by itself if it finds it.
  final bool? settled;
  final Problem? problem;
}

/// What an interface draws.
class SessionState {
  const SessionState({
    required this.revision,
    required this.phase,
    required this.phaseProblem,
    required this.tor,
    required this.node,
    required this.ownNodes,
    required this.sync,
    required this.syncsEnded,
    required this.balanceTotal,
    required this.balanceUnlocked,
    required this.balanceLocked,
    required this.receiveAddress,
    required this.history,
    required this.send,
    required this.rebuiltFromKeys,
    required this.notSaved,
    required this.lastProblem,
  });

  /// Goes up with every change.
  final int revision;
  final Phase phase;

  /// Why the wallet did not open or close, when [phase] is [Phase.failed].
  final Problem? phaseProblem;
  final TorStatus tor;

  /// The node synchronised with last, as `host.onion:port`.
  final String? node;

  /// The user's own nodes, as `host.onion:port`, in the order to try
  /// them. Empty while the nodes the session was started with are used.
  final List<String> ownNodes;
  final SyncStatus sync;

  /// How many synchronisations have ended since the session started.
  final int syncsEnded;

  /// Funds as of the last synchronisation that finished.
  final String balanceTotal;

  /// The part of it that can be spent now.
  final String balanceUnlocked;

  /// The rest: received or returned as change, and not yet confirmed
  /// often enough to be spent.
  final String balanceLocked;
  final ReceiveAddress? receiveAddress;

  /// Transfers as of the last synchronisation, oldest first.
  final List<Transfer> history;
  final SendStatus send;

  /// The wallet's data file could not be read and the wallet was opened
  /// from its keys. Its funds show again after a synchronisation.
  final bool rebuiltFromKeys;

  /// Why the wallet could not be written to disk the last time that was
  /// tried; null once a save has worked again. While it is set, what the
  /// wallet has learned since its last good save is in memory only.
  final Problem? notSaved;

  /// Why the last command did nothing, when the reason has no place of its
  /// own in the state.
  final Problem? lastProblem;

  factory SessionState.fromJson(Map<String, dynamic> json) {
    final phase = json['phase'] as Map<String, dynamic>;
    final tor = json['tor'] as Map<String, dynamic>;
    final sync = json['sync'] as Map<String, dynamic>;
    final balance = json['balance'] as Map<String, dynamic>;
    final address = json['receive_address'] as Map<String, dynamic>?;
    final send = json['send'] as Map<String, dynamic>;
    return SessionState(
      revision: json['revision'] as int,
      phase: _named(Phase.values, phase['name'], 'phase'),
      phaseProblem: Problem.maybe(phase['problem']),
      tor: TorStatus(
        _named(TorState.values, tor['state'], 'state of Tor'),
        tor['percent'] as int,
        tor['message'] as String?,
      ),
      node: json['node'] as String?,
      ownNodes: (json['own_nodes'] as List<dynamic>).cast<String>(),
      sync: SyncStatus(
        _named(SyncPhase.values, sync['name'], 'state of synchronisation'),
        walletHeight: sync['wallet_height'] as int?,
        nodeHeight: sync['node_height'] as int?,
        problem: Problem.maybe(sync['problem']),
      ),
      syncsEnded: json['syncs_ended'] as int,
      balanceTotal: balance['total'] as String,
      balanceUnlocked: balance['unlocked'] as String,
      balanceLocked: balance['locked'] as String,
      receiveAddress: address == null
          ? null
          : ReceiveAddress(
              address['index'] as int,
              address['address'] as String,
            ),
      history: [
        for (final entry in json['history'] as List<dynamic>)
          _transfer(entry as Map<String, dynamic>),
      ],
      send: SendStatus(
        _named(SendPhase.values, send['name'], 'state of a send'),
        to: send['to'] as String?,
        amount: send['amount'] as String?,
        fee: send['fee'] as String?,
        total: send['total'] as String?,
        sourceAddresses: (send['source_addresses'] as List<dynamic>?)
            ?.cast<int>(),
        txids: (send['txids'] as List<dynamic>?)?.cast<String>(),
        settled: send['settled'] as bool?,
        problem: Problem.maybe(send['problem']),
      ),
      rebuiltFromKeys: json['rebuilt_from_keys'] as bool,
      notSaved: Problem.maybe(json['not_saved']),
      lastProblem: Problem.maybe(json['last_problem']),
    );
  }
}

Transfer _transfer(Map<String, dynamic> json) => Transfer(
  txid: json['txid'] as String,
  direction: switch (json['direction']) {
    'in' => Direction.incoming,
    'out' => Direction.outgoing,
    final other => throw FormatException('unknown direction: $other'),
  },
  amount: json['amount'] as String,
  fee: json['fee'] as String,
  blockHeight: json['block_height'] as int,
  confirmations: json['confirmations'] as int,
  timestamp: json['timestamp'] as int,
  pending: json['pending'] as bool,
  failed: json['failed'] as bool,
  addressIndices: (json['address_indices'] as List<dynamic>).cast<int>(),
);
