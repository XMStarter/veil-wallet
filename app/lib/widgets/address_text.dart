import 'package:flutter/material.dart';

import '../theme/type.dart';

/// An address made to be checked: in groups of four characters, with the
/// beginning and the end emphasised.
class AddressText extends StatelessWidget {
  const AddressText(this.address, {super.key});

  final String address;

  /// How many groups at each end stand out.
  static const _edge = 2;

  /// [address] cut into groups of four characters; the last may be
  /// shorter.
  static List<String> groups(String address) => [
    for (var at = 0; at < address.length; at += 4)
      address.substring(at, at + 4 < address.length ? at + 4 : address.length),
  ];

  @override
  Widget build(BuildContext context) {
    final parts = groups(address);
    return Text.rich(
      TextSpan(
        children: [
          for (final (position, part) in parts.indexed)
            TextSpan(
              text: position == parts.length - 1 ? part : '$part ',
              style: position < _edge || position >= parts.length - _edge
                  ? VeilText.addressEdge
                  : VeilText.addressMiddleIn(context),
            ),
        ],
      ),
      textAlign: TextAlign.center,
      // Read out and searched for as what it is, without the gaps.
      semanticsLabel: address,
    );
  }
}
