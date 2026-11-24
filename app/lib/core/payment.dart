/// What was pasted or typed where an address is expected: an address, or a
/// payment link as Monero wallets write it, `monero:<address>`, which may
/// name an amount.
class PaymentTarget {
  const PaymentTarget(this.address, [this.amount]);

  final String address;

  /// The amount the link asks for, as text; null if it names none.
  final String? amount;
}

final _spaces = RegExp(r'\s+');
final _number = RegExp(r'^\d+(\.\d+)?$');

/// Reads [text] as an address or a payment link. Whether the address is
/// one is for the wallet to say; this only takes the text apart.
///
/// An address may come with gaps in it, as this app shows it, in groups of
/// four: they are taken out. An amount in a link counts only if it is a
/// plain number; anything else in the link is left aside.
PaymentTarget readPayment(String text) {
  var rest = text.trim();
  String? amount;
  if (rest.toLowerCase().startsWith('monero:')) {
    rest = rest.substring('monero:'.length);
    final mark = rest.indexOf('?');
    if (mark >= 0) {
      for (final part in rest.substring(mark + 1).split('&')) {
        final equals = part.indexOf('=');
        if (equals < 0 || part.substring(0, equals) != 'tx_amount') continue;
        final value = part.substring(equals + 1);
        if (_number.hasMatch(value)) amount = value;
      }
      rest = rest.substring(0, mark);
    }
  }
  return PaymentTarget(rest.replaceAll(_spaces, ''), amount);
}
