# veil_bridge

The Dart side of Veil's core: it loads the native library built from
[`native/veil_core`](../../native/veil_core), starts a session in it, sends
it commands, and reads its state.

```dart
final core = VeilCore.open(libraryPath);
final session = core.start(config, password: password);
// Returns at once. Draw from session.state() whenever session.revision
// has changed.
session.sync();
session.prepareSend(address, '1.5'); // state.send becomes review, with the fee
session.confirmSend();               // or cancelSend()
final last = session.close();
```

- Commands return at once; their outcome shows in the state.
- `recoveryPhrase()` and `close()` wait for the session's thread. A
  synchronisation under way stops for them, normally within seconds, but
  not at once. An app calls them off its main isolate: `session.address`
  crosses to another isolate, and `VeilSession.fromAddress` takes it
  there.
- Amounts are text, exactly as the core wrote them. This side shows them
  and passes them back, and never calculates with them: the part of the
  balance that cannot be spent yet comes from the core as an amount of
  its own.
- A problem has a kind (`ProblemKind`) for the interface to put into its
  own words, and a detail in English for a log.
- The network is stagenet. There is no other yet.

## Tests

```bash
dart test
```

`test/state_test.dart` reads the state from the files the core's own tests
write (`native/veil_core/tests/fixtures`), so a change on one side that the
other does not follow fails here.

`test/ffi_test.dart` loads the core as a shared library and calls it. It
needs `cargo build -p veil_core` in `native/` first (or `VEIL_CORE_LIB`)
and is skipped without it. It opens no wallet, so Tor is never started and
nothing leaves the machine.

`native/check.sh offline` runs both, with format and analysis, when Flutter
is installed by `./check.sh setup`.
