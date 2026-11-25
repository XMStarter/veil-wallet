import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/type.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';
import 'problem_words.dart';
import 'receive_screen.dart';
import 'send_screen.dart';
import 'settings_screen.dart';
import 'status_words.dart';
import 'unlock_screen.dart';

/// Where an open wallet lands: the balance, what Tor and the
/// synchronisation are doing, the way to receiving and to sending, and the
/// latest payments.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  bool _locking = false;

  /// Amounts are not shown: for a screen someone else can see.
  bool _hidden = false;

  /// Where the synchronisation under way was first seen, to tell how much
  /// of its way it has gone.
  int? _scanFrom;

  /// How many entries of the history the screen shows.
  static const _recent = 20;

  @override
  void initState() {
    super.initState();
    widget.backend.state.addListener(_follow);
    _follow();
  }

  @override
  void dispose() {
    widget.backend.state.removeListener(_follow);
    super.dispose();
  }

  void _follow() {
    final sync = widget.backend.state.value?.sync;
    if (sync == null || sync.phase != SyncPhase.running) {
      _scanFrom = null;
    } else {
      _scanFrom ??= sync.walletHeight;
    }
  }

  /// How much of the synchronisation under way is done, from 0 to 1.
  double _done(SyncStatus sync) {
    final from = _scanFrom, at = sync.walletHeight, to = sync.nodeHeight;
    if (from == null || at == null || to == null || to <= from) return 0;
    return ((at - from) / (to - from)).clamp(0, 1).toDouble();
  }

  /// Saves and closes the wallet, stops Tor, and asks for the password
  /// again. The app stays open.
  Future<void> _lock() async {
    if (_locking) return;
    setState(() => _locking = true);
    Problem? notSaved;
    try {
      notSaved = await widget.backend.close();
    } catch (error) {
      // The wallet is closed as far as the app can tell; what went wrong
      // is said where closing leads, not left as "closing" for ever.
      notSaved = unforeseen(error);
    }
    if (!mounted) return;
    openAlone(
      context,
      UnlockScreen(backend: widget.backend, notSaved: notSaved),
    );
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.homeTitle,
      actions: [
        VeilButton(
          label: text.homeReceive,
          onPressed: _locking
              ? null
              : () => open(context, ReceiveScreen(backend: widget.backend)),
        ),
        VeilButton(
          label: text.homeSend,
          primary: false,
          onPressed: _locking
              ? null
              : () => open(context, SendScreen(backend: widget.backend)),
        ),
        VeilQuietButton(
          label: text.homeSettings,
          onPressed: _locking
              ? null
              : () => open(context, SettingsScreen(backend: widget.backend)),
        ),
        VeilQuietButton(
          label: _locking ? text.lockWorking : text.lockAction,
          onPressed: _locking ? null : _lock,
        ),
      ],
      children: [
        ValueListenableBuilder<SessionState?>(
          valueListenable: widget.backend.state,
          builder: (context, state, _) {
            if (state == null) return const SizedBox.shrink();
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _balance(text, state),
                const SizedBox(height: 20),
                _status(text, state),
                if (state.notSaved != null) ...[
                  const SizedBox(height: 16),
                  Notice(
                    '${text.homeNotSaved} '
                    '${text.problemDetail(state.notSaved!.detail)}',
                    warning: true,
                  ),
                ],
                // A phone set to stop the app outside the screen: said,
                // because a long synchronisation then never finishes by
                // itself.
                ValueListenableBuilder<bool>(
                  valueListenable: widget.backend.device.stoppedInBackground,
                  builder: (context, stopped, _) => stopped
                      ? Padding(
                          padding: const EdgeInsets.only(top: 16),
                          child: Notice(text.homeStoppedInBackground),
                        )
                      : const SizedBox.shrink(),
                ),
                if (state.rebuiltFromKeys) ...[
                  const SizedBox(height: 16),
                  Notice(text.homeRebuilt, warning: true),
                  // Nothing has been synchronised yet: the wallet waits
                  // to be told, or to be given nodes in Settings.
                  if (state.sync.phase == SyncPhase.notYet) ...[
                    const SizedBox(height: 8),
                    Notice(text.homeRebuiltWaiting),
                    VeilQuietButton(
                      label: text.homeRebuiltSync,
                      onPressed: _locking ? null : widget.backend.sync,
                    ),
                  ],
                ],
                // A payment in doubt is said here too, not only on the
                // screen it happened on.
                if (state.send.phase == SendPhase.outcomeUnknown) ...[
                  const SizedBox(height: 16),
                  Notice(text.homeUnknownNotice, warning: true),
                ],
                const SizedBox(height: 24),
                ..._activity(text, state),
                const SizedBox(height: 24),
                Notice(text.startStagenetNotice),
              ],
            );
          },
        ),
      ],
    );
  }

  /// The balance. A tap hides every amount on the screen, another shows
  /// them again.
  Widget _balance(AppLocalizations text, SessionState state) {
    return Semantics(
      button: true,
      label: _hidden ? text.homeBalanceShow : text.homeBalanceHide,
      child: InkWell(
        onTap: () => setState(() => _hidden = !_hidden),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (_hidden) ...[
              Text(
                text.homeBalanceHidden,
                style: VeilText.amount,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 4),
              Text(
                text.homeBalanceShow,
                style: VeilText.secondaryIn(context),
                textAlign: TextAlign.center,
              ),
            ] else ...[
              FittedBox(
                fit: BoxFit.scaleDown,
                child: Text(
                  text.amountXmr(state.balanceTotal),
                  style: VeilText.amount,
                  maxLines: 1,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                text.homeAvailable(state.balanceUnlocked),
                style: VeilText.secondaryIn(context),
                textAlign: TextAlign.center,
              ),
              Text(
                text.homePending(state.balanceLocked),
                style: VeilText.secondaryIn(context),
                textAlign: TextAlign.center,
              ),
              // The amounts are those of the last synchronisation that
              // finished; say so while they may be behind.
              if (state.sync.phase != SyncPhase.done)
                Text(
                  text.homeBalanceStale,
                  style: VeilText.secondaryIn(context),
                  textAlign: TextAlign.center,
                ),
            ],
          ],
        ),
      ),
    );
  }

  /// Tor and the synchronisation, in words; colour carries nothing here.
  Widget _status(AppLocalizations text, SessionState state) {
    final running = state.sync.phase == SyncPhase.running;
    return Container(
      color: VeilPalette.of(context).surface,
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            torWords(text, state.tor),
            style: VeilText.body,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 4),
          Text(
            syncWords(text, state),
            style: VeilText.body,
            textAlign: TextAlign.center,
          ),
          if (running || _locking) ...[
            const SizedBox(height: 10),
            // Without a value while locking: that takes as long as it
            // takes.
            LinearProgressIndicator(value: _locking ? null : _done(state.sync)),
          ],
        ],
      ),
    );
  }

  /// The latest entries of the history, newest first.
  List<Widget> _activity(AppLocalizations text, SessionState state) {
    final latest = state.history.reversed.take(_recent).toList();
    return [
      Text(text.homeActivity.toUpperCase(), style: VeilText.headingIn(context)),
      const SizedBox(height: 8),
      const Divider(),
      if (latest.isEmpty)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 16),
          child: Text(
            text.homeNoActivity,
            style: VeilText.secondaryIn(context),
          ),
        ),
      for (final transfer in latest) ...[
        _ActivityRow(transfer: transfer, hidden: _hidden),
        const Divider(),
      ],
    ];
  }
}

/// One payment: which way it went, when, and how much.
class _ActivityRow extends StatelessWidget {
  const _ActivityRow({required this.transfer, required this.hidden});

  final Transfer transfer;
  final bool hidden;

  /// How many confirmations make received funds spendable.
  static const _settled = 10;

  String _when(AppLocalizations text) {
    if (transfer.failed) return text.activityFailed;
    if (transfer.pending || transfer.confirmations == 0) {
      return text.activityUnconfirmed;
    }
    if (transfer.confirmations < _settled) {
      return text.activityConfirming(transfer.confirmations);
    }
    final time = DateTime.fromMillisecondsSinceEpoch(transfer.timestamp * 1000);
    return DateFormat.yMMMd(text.localeName).add_Hm().format(time);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final incoming = transfer.direction == Direction.incoming;
    // A sign and a label as well as a colour, so that the two can be told
    // apart without it.
    final sign = incoming ? '+' : '−';
    final colour = transfer.failed
        ? VeilPalette.of(context).muted
        : incoming
        ? VeilPalette.of(context).threadNavy
        : VeilPalette.of(context).threadCrimson;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  incoming ? text.activityReceived : text.activitySent,
                  style: VeilText.body,
                ),
                Text(_when(text), style: VeilText.secondaryIn(context)),
              ],
            ),
          ),
          const SizedBox(width: 12),
          if (!hidden)
            Text(
              '$sign${transfer.amount}',
              style: VeilText.listAmount.copyWith(color: colour),
            ),
        ],
      ),
    );
  }
}
