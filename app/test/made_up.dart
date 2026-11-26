// Made-up input for whatever in the app reads text from outside: the
// Dart side of `native/veil_test_guard/src/fuzz.rs`, which says what this
// is and what it is not.
//
// The inputs come from a seed. With nothing set it is fixed, so every run
// of the tests sees the same inputs; `native/check.sh fuzz` sets another
// (`VEIL_FUZZ_SEED`) and a time to run for (`VEIL_FUZZ_SECONDS`). A
// failure names the seed, the number of the case and the input.

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';

const _defaultCases = 3000;
const _defaultSeed = 0x5eed0f7e11;

/// Characters that parsers trip over, besides the ones in the samples.
const _awkward = [
  '',
  ' ',
  '  ',
  '\t',
  '\n',
  '\r\n',
  '\u0000',
  '\ufeff',
  '\u200b',
  '\u202e',
  '\u00a0',
  '\u2028',
  '.',
  ',',
  ':',
  ';',
  '-',
  '+',
  '_',
  '/',
  '\\',
  '?',
  '&',
  '=',
  '#',
  '%',
  '@',
  "'",
  '"',
  '{',
  '}',
  '[',
  ']',
  '0',
  '1',
  '9',
  '00',
  '-1',
  '1e9',
  '0x10',
  '5.0',
  '18446744073709551615',
  '18446744073709551616',
  '99999999999999999999999999999999',
  'a',
  'A',
  'z',
  'é',
  'ß',
  'я',
  '中',
  '🙂',
  '\u0301',
  '\ud800',
  '\udc00',
  'İ',
  'null',
  'true',
  'onion',
  '.onion',
  'monero:',
  'MONERO:',
  'tx_amount=',
  '?tx_amount=1',
  'obfs4',
  'http://',
  '%00',
  '%20',
  '..',
];

int? _number(String name) =>
    int.tryParse(Platform.environment[name]?.trim() ?? '');

int _hash(String name) {
  var hash = 0x811c9dc5;
  for (final unit in name.codeUnits) {
    hash = ((hash ^ unit) * 0x01000193) & 0xffffffff;
  }
  return hash;
}

/// One made-up input and where it came from.
class _Case {
  _Case(this.name, this.seed, this.number, this.text);

  final String name;
  final int seed;
  final int number;
  final String text;

  String failed(Object error, StackTrace stack) {
    final shown = text.length > 400 ? '${text.substring(0, 400)}...' : text;
    return 'made-up input $number of `$name` failed '
        '(VEIL_FUZZ_SEED=$seed VEIL_FUZZ_CASES=${number + 1} bring it back):\n'
        '  ${jsonEncode(shown)}\n'
        '$error\n$stack';
  }
}

/// The inputs for [name], made as they are asked for: the samples first,
/// then as many made-up ones as the environment says, or until its time
/// is up.
Iterable<_Case> _cases(String name, List<String> samples, int? atMost) sync* {
  final seed = _number('VEIL_FUZZ_SEED') ?? _defaultSeed;
  final seconds = _number('VEIL_FUZZ_SECONDS');
  // A number that was asked for is run in full, whatever the test wants:
  // that is how a failure far into a long run is brought back.
  final cases =
      _number('VEIL_FUZZ_CASES') ?? min(_defaultCases, atMost ?? _defaultCases);
  final until = seconds == null
      ? null
      : DateTime.now().add(Duration(seconds: seconds));
  final random = Random(seed ^ _hash(name));
  for (
    var number = 0;
    until == null ? number < cases : DateTime.now().isBefore(until);
    number++
  ) {
    yield _Case(
      name,
      seed,
      number,
      number < samples.length ? samples[number] : _text(random, samples),
    );
  }
}

/// Hands [check] texts made from [samples], which should be inputs the
/// code under test accepts, and from nothing; the samples themselves come
/// first. [check] fails as a test does. [atMost] keeps a costly check from
/// running thousands of times when no time to run for is set.
void madeUpTexts(
  String name,
  List<String> samples,
  void Function(String text) check, {
  int? atMost,
}) {
  for (final made in _cases(name, samples, atMost)) {
    try {
      check(made.text);
    } catch (error, stack) {
      fail(made.failed(error, stack));
    }
  }
}

/// [madeUpTexts] for a check that has to wait for something.
Future<void> madeUpTextsAsync(
  String name,
  List<String> samples,
  Future<void> Function(String text) check, {
  int? atMost,
}) async {
  for (final made in _cases(name, samples, atMost)) {
    try {
      await check(made.text);
    } catch (error, stack) {
      fail(made.failed(error, stack));
    }
  }
}

String _pick(Random random, List<String> from) =>
    from.isEmpty ? '' : from[random.nextInt(from.length)];

String _text(Random random, List<String> samples) {
  switch (random.nextInt(10)) {
    // A good one, damaged once or a few times.
    case < 6:
      var text = _pick(random, samples);
      for (var times = random.nextInt(3); times >= 0; times--) {
        text = _damage(random, text);
      }
      return text;
    // Two good ones, each cut somewhere, end to end.
    case 6:
      final first = _pick(random, samples);
      final second = _pick(random, samples);
      return first.substring(0, random.nextInt(first.length + 1)) +
          second.substring(random.nextInt(second.length + 1));
    // Awkward pieces and nothing else.
    case < 9:
      return [
        for (var count = random.nextInt(12); count > 0; count--)
          _pick(random, _awkward),
      ].join();
    // Units that are mostly no text at all.
    default:
      return String.fromCharCodes([
        for (var count = random.nextInt(64); count > 0; count--)
          random.nextInt(0x10000),
      ]);
  }
}

String _damage(Random random, String text) {
  final at = random.nextInt(text.length + 1);
  final piece = _pick(random, _awkward);
  final before = text.substring(0, at);
  final after = text.substring(at);
  switch (random.nextInt(8)) {
    // Something put in.
    case 0 || 1:
      return before + piece + after;
    // Something taken out.
    case 2:
      return before + after.substring(min(after.length, 1 + random.nextInt(4)));
    // One unit changed for another.
    case 3:
      return before + piece + after.substring(min(after.length, 1));
    // Its case changed.
    case 4:
      if (after.isEmpty) return text;
      final unit = after.substring(0, 1);
      final changed = unit == unit.toUpperCase()
          ? unit.toLowerCase()
          : unit.toUpperCase();
      return before + changed + after.substring(1);
    // A part said twice.
    case 5:
      final part = after.substring(0, min(after.length, random.nextInt(8)));
      return before + part + after;
    // Cut short.
    case 6:
      return before;
    // A piece repeated until it is long.
    default:
      return before + piece * (1 + random.nextInt(400)) + after;
  }
}
