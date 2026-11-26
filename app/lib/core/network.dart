import 'package:veil_bridge/veil_bridge.dart';

/// The network this app was built for, as the build said it
/// (`--dart-define=VEIL_NETWORK=mainnet`). A build that was told nothing
/// is for stagenet, whose coins are worth nothing: an app holds real
/// money only when it was built to, and never by an oversight.
const builtFor = String.fromEnvironment(
  'VEIL_NETWORK',
  defaultValue: 'stagenet',
);

/// The network of that name. Anything but the two names is an error, so
/// that a build with a misspelt network does not start as something else.
WalletNetwork networkNamed(String name) => switch (name) {
  'mainnet' => WalletNetwork.mainnet,
  'stagenet' => WalletNetwork.stagenet,
  _ => throw ArgumentError.value(
    name,
    'VEIL_NETWORK',
    'the app is built for mainnet or for stagenet',
  ),
};
