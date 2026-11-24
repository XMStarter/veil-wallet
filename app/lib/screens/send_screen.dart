import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../core/backend.dart';
import '../core/payment.dart';
import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/type.dart';
import '../widgets/address_text.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_field.dart';
import '../widgets/veil_screen.dart';
import 'problem_words.dart';

/// A payment, from the form to its outcome.
///
/// The send itself lives in the core, and this screen shows whatever the
/// core says about it: the form while there is none, the transaction to
/// review once it is built, and then how it went. Nothing leaves the
/// device before the user has seen the address, the amount, the fee and
/// the total, and pressed the button under them.
class SendScreen extends StatefulWidget {
  const SendScreen({super.key, required this.backend});

  final WalletBackend backend;

  @override
  State<SendScreen> createState() => _SendScreenState();
}

class _SendScreenState extends State<SendScreen> {
  final _address = TextEditingController();
  final _amount = TextEditingController();

  /// All that can be spent, less the fee, instead of a typed amount.
  bool _everything = false;

  /// A request the core could not read at all, such as an amount that is
  /// not a number. Everything else it objects to arrives in the state.
  Problem? _refused;

  /// The clipboard had nothing to paste.
  bool _nothingToPaste = false;

  @override
  void initState() {
    super.initState();
    // A send that failed earlier and was left on show is not this form's
    // to explain.
    if (_phase == SendPhase.failed) widget.backend.cancelSend();
  }

  @override
  void dispose() {
    _address.dispose();
    _amount.dispose();
    super.dispose();
  }

  SendPhase get _phase =>
      widget.backend.state.value?.send.phase ?? SendPhase.idle;

  bool get _filled =>
      _address.text.trim().isNotEmpty &&
      (_everything || _amount.text.trim().isNotEmpty);

  Future<void> _paste() async {
    String text;
    try {
      final data = await Clipboard.getData(Clipboard.kTextPlain);
      text = data?.text?.trim() ?? '';
    } on PlatformException {
      text = '';
    } on MissingPluginException {
      text = '';
    }
    if (!mounted) return;
    if (text.isEmpty) {
      setState(() => _nothingToPaste = true);
      return;
    }
    final target = readPayment(text);
    setState(() {
      _nothingToPaste = false;
      _address.text = target.address;
      final amount = target.amount;
      if (amount != null && !_everything) _amount.text = amount;
    });
  }

  void _prepare() {
    if (!_filled) return;
    // A stale refusal goes first, so that the one shown is this attempt's.
    if (_phase == SendPhase.failed) widget.backend.cancelSend();
    final refused = widget.backend.prepareSend(
      readPayment(_address.text).address,
      _everything ? everything : _amount.text.trim(),
    );
    setState(() {
      _refused = refused;
      _nothingToPaste = false;
    });
  }

  void _leave() {
    if (_phase == SendPhase.failed) widget.backend.cancelSend();
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return ValueListenableBuilder<SessionState?>(
      valueListenable: widget.backend.state,
      builder: (context, state, _) {
        final send = state?.send ?? const SendStatus(SendPhase.idle);
        return switch (send.phase) {
          SendPhase.idle ||
          SendPhase.preparing ||
          SendPhase.failed => _form(text, state, send),
          SendPhase.review || SendPhase.sending => _review(text, send),
          SendPhase.sent => _sent(text),
          SendPhase.outcomeUnknown => _unknown(text, send),
        };
      },
    );
  }

  /// Where to, and how much.
  Widget _form(AppLocalizations text, SessionState? state, SendStatus send) {
    final preparing = send.phase == SendPhase.preparing;
    final problem = _refused ?? send.problem;
    final kind = problem?.kind;
    final aboutAddress = kind == ProblemKind.invalidAddress;
    final aboutAmount =
        kind == ProblemKind.invalidAmount ||
        kind == ProblemKind.zeroAmount ||
        kind == ProblemKind.notEnoughFunds ||
        kind == ProblemKind.severalTransactions;
    return VeilScreen(
      title: text.sendTitle,
      actions: [
        VeilButton(
          label: text.continueLabel,
          onPressed: _filled && !preparing ? _prepare : null,
        ),
        if (preparing)
          VeilQuietButton(
            label: text.sendCancel,
            onPressed: widget.backend.cancelSend,
          )
        else
          VeilQuietButton(label: text.back, onPressed: _leave),
      ],
      children: [
        VeilField(
          label: text.sendAddressLabel,
          controller: _address,
          monospace: true,
          lines: 3,
          enabled: !preparing,
          autofocus: true,
          onChanged: (_) => setState(() => _refused = null),
        ),
        if (problem != null && aboutAddress)
          ProblemText(problemWords(text, problem)),
        if (_nothingToPaste) ProblemText(text.sendPasteEmpty),
        VeilQuietButton(
          label: text.sendPaste,
          onPressed: preparing ? null : _paste,
        ),
        const SizedBox(height: 8),
        if (_everything)
          Notice(text.sendEverythingOn)
        else
          VeilField(
            label: text.sendAmountLabel,
            controller: _amount,
            numeric: true,
            enabled: !preparing,
            onChanged: (_) => setState(() => _refused = null),
            onSubmitted: (_) => _prepare(),
          ),
        if (problem != null && aboutAmount)
          ProblemText(problemWords(text, problem)),
        if (state != null) ...[
          const SizedBox(height: 8),
          Text(
            text.homeAvailable(state.balanceUnlocked),
            style: VeilText.secondaryIn(context),
          ),
        ],
        VeilQuietButton(
          label: _everything ? text.sendExact : text.sendEverything,
          onPressed: preparing
              ? null
              : () => setState(() {
                  _everything = !_everything;
                  _refused = null;
                }),
        ),
        if (problem != null && !aboutAddress && !aboutAmount)
          ProblemText(problemWords(text, problem)),
        if (preparing) ...[
          const SizedBox(height: 16),
          const LinearProgressIndicator(),
          const SizedBox(height: 8),
          Text(
            state?.sync.phase == SyncPhase.running
                ? text.sendWaitingForSync
                : text.sendPreparing,
            style: VeilText.secondaryIn(context),
            textAlign: TextAlign.center,
          ),
        ],
      ],
    );
  }

  /// The transaction as it would go out, for a last look.
  Widget _review(AppLocalizations text, SendStatus send) {
    final sending = send.phase == SendPhase.sending;
    final sources = send.sourceAddresses?.length ?? 0;
    return VeilScreen(
      title: text.reviewTitle,
      actions: [
        VeilButton(
          label: text.reviewSend,
          onPressed: sending ? null : widget.backend.confirmSend,
        ),
        VeilQuietButton(
          label: text.sendCancel,
          onPressed: sending ? null : widget.backend.cancelSend,
        ),
      ],
      children: [
        Text(text.reviewTo.toUpperCase(), style: VeilText.headingIn(context)),
        const SizedBox(height: 8),
        Container(
          color: VeilPalette.of(context).surface,
          padding: const EdgeInsets.all(12),
          child: AddressText(send.to ?? ''),
        ),
        const SizedBox(height: 16),
        _Line(text.reviewAmount, text.amountXmr(send.amount ?? '')),
        const Divider(),
        _Line(text.reviewFee, text.amountXmr(send.fee ?? '')),
        const Divider(),
        _Line(text.reviewTotal, text.amountXmr(send.total ?? ''), strong: true),
        const SizedBox(height: 16),
        if (sources > 1) ...[
          Notice(text.reviewCombined(sources), warning: true),
          const SizedBox(height: 12),
        ],
        if (sending) ...[
          const LinearProgressIndicator(),
          const SizedBox(height: 8),
          Text(
            text.reviewSending,
            style: VeilText.secondaryIn(context),
            textAlign: TextAlign.center,
          ),
        ] else
          Notice(text.reviewNothingSent),
      ],
    );
  }

  Widget _sent(AppLocalizations text) {
    return VeilScreen(
      title: text.sentTitle,
      actions: [
        VeilButton(
          label: text.sentDone,
          onPressed: () {
            widget.backend.clearSend();
            Navigator.of(context).pop();
          },
        ),
      ],
      children: [
        Text(text.sentBody, style: VeilText.body, textAlign: TextAlign.center),
      ],
    );
  }

  /// The connection was lost while sending. Never shown as a failure: a
  /// second send could pay twice.
  Widget _unknown(AppLocalizations text, SendStatus send) {
    final settled = send.settled ?? false;
    return VeilScreen(
      title: text.unknownTitle,
      actions: [
        if (settled)
          VeilButton(
            label: text.unknownUnderstood,
            onPressed: widget.backend.clearSend,
          ),
        VeilQuietButton(
          label: text.unknownBack,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        Notice(settled ? text.unknownSettled : text.unknownBody, warning: true),
      ],
    );
  }
}

/// A name and an amount on one line.
class _Line extends StatelessWidget {
  const _Line(this.label, this.value, {this.strong = false});

  final String label;
  final String value;

  /// The line the eye should land on.
  final bool strong;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.baseline,
        textBaseline: TextBaseline.alphabetic,
        children: [
          Text(label, style: VeilText.body),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              value,
              style: strong ? VeilText.listAmount : VeilText.body,
              textAlign: TextAlign.end,
            ),
          ),
        ],
      ),
    );
  }
}
