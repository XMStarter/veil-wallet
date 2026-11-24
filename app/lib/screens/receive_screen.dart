import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import '../theme/colors.dart';
import '../theme/type.dart';
import '../widgets/address_text.dart';
import '../widgets/notice.dart';
import '../widgets/qr_view.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'problem_words.dart';

/// The address to be paid at: as a QR code and as text made to be checked.
/// It is the wallet's current one; once it has been paid, the core hands
/// over the next and the screen shows that.
class ReceiveScreen extends StatefulWidget {
  const ReceiveScreen({super.key, required this.backend});

  final WalletBackend backend;

  /// What a QR code for [address] holds: a payment link, as other Monero
  /// wallets write and read it.
  static String link(String address) => 'monero:$address';

  @override
  State<ReceiveScreen> createState() => _ReceiveScreenState();
}

class _ReceiveScreenState extends State<ReceiveScreen> {
  /// The address that was put into the clipboard, while it is the one
  /// shown.
  String? _copied;

  /// Whether the user has asked for another address on this screen. Only
  /// then is a refusal theirs to read.
  bool _asked = false;

  /// The system did not take the address.
  bool _copyFailed = false;

  Future<void> _copy(String address) async {
    var taken = true;
    try {
      await Clipboard.setData(ClipboardData(text: address));
    } on PlatformException {
      taken = false;
    } on MissingPluginException {
      taken = false;
    }
    if (!mounted) return;
    setState(() {
      _copied = taken ? address : null;
      _copyFailed = !taken;
    });
  }

  void _another() {
    setState(() {
      _asked = true;
      _copyFailed = false;
    });
    widget.backend.newAddress();
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return ValueListenableBuilder<SessionState?>(
      valueListenable: widget.backend.state,
      builder: (context, state, _) {
        final receive = state?.receiveAddress;
        final problem = state?.lastProblem;
        final refused =
            _asked &&
            problem != null &&
            problem.kind == ProblemKind.tooManyUnusedAddresses;
        return VeilScreen(
          title: text.receiveTitle,
          actions: [
            VeilButton(
              label: text.receiveCopy,
              onPressed: receive == null ? null : () => _copy(receive.address),
            ),
            VeilButton(
              label: text.receiveNew,
              primary: false,
              onPressed: receive == null ? null : _another,
            ),
            VeilQuietButton(
              label: text.back,
              onPressed: () => Navigator.of(context).pop(),
            ),
          ],
          children: [
            if (receive != null) ...[
              Center(child: QrView(ReceiveScreen.link(receive.address))),
              const SizedBox(height: 16),
              Text(
                text.receiveAddressNumber(receive.index).toUpperCase(),
                style: VeilText.heading,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              Container(
                color: VeilColors.surface,
                padding: const EdgeInsets.all(12),
                child: AddressText(receive.address),
              ),
              if (_copied == receive.address) ...[
                const SizedBox(height: 8),
                Text(
                  text.receiveCopied,
                  style: VeilText.secondary,
                  textAlign: TextAlign.center,
                ),
              ],
              if (_copyFailed) ProblemText(text.receiveCopyFailed),
              if (refused) ProblemText(problemWords(text, problem)),
              const SizedBox(height: 16),
              Notice(text.receiveNotice),
            ],
          ],
        );
      },
    );
  }
}
