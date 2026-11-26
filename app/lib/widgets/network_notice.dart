import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../l10n/app_localizations.dart';
import 'notice.dart';

/// What the screens say about the network a wallet is on.
///
/// An app built for stagenet says so wherever a balance or an address is
/// near: its coins are worth nothing, and nobody is to take them for
/// money. An app built for the main network says what it means that the
/// money is real where a wallet is made or brought back, which is
/// [whereAWalletBegins], and under About Veil; it does not repeat it
/// under every balance.
class NetworkNotice extends StatelessWidget {
  const NetworkNotice(
    this.network, {
    super.key,
    this.whereAWalletBegins = false,
  });

  final WalletNetwork network;
  final bool whereAWalletBegins;

  /// Whether there is anything to say here, for the space around it.
  bool get saysSomething =>
      network == WalletNetwork.stagenet || whereAWalletBegins;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    if (network == WalletNetwork.stagenet) {
      return Notice(text.startStagenetNotice);
    }
    if (!whereAWalletBegins) return const SizedBox.shrink();
    return Notice(text.startMainnetNotice, warning: true);
  }
}
