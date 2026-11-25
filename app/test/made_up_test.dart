// What the app reads from outside, given thousands of made-up inputs: a
// code read with the camera, a phrase typed or pasted, the lines of
// bridges and nodes, and the settings file.

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/payment.dart';
import 'package:veil/core/phrase.dart';
import 'package:veil/core/settings.dart';

import 'made_up.dart';

const _address =
    '73a4nWuvkYoYoksGurDjKZQcZkmaxLaKbbeiKzHnMmqKivrCzq5Q2JtJG1UZNZFqLPbQ3MiXCk2Q5bdwdUNSr7X9QrPubkn';

final _whitespace = RegExp(r'\s');
final _plainNumber = RegExp(r'^[0-9]+(\.[0-9]+)?$');
final _lettersOnly = RegExp(r'^[\p{L}\p{M}]+$', unicode: true);

void main() {
  test('a made-up code or paste is taken apart and nothing is added', () {
    final samples = [
      _address,
      'monero:$_address',
      'monero:$_address?tx_amount=0.25',
      'MONERO:$_address?recipient_name=A&tx_amount=1&tx_description=tea',
      'monero:$_address?tx_amount=1&tx_amount=2',
      'monero:$_address?tx_amount=1e9',
      'monero:$_address?tx_amount=-1',
      '  ${_address.substring(0, 4)} ${_address.substring(4)}\n',
      'bitcoin:bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh?amount=1',
      'https://example.com/?tx_amount=5',
    ];
    madeUpTexts('payment', samples, (text) {
      final target = readPayment(text);
      expect(target.address.contains(_whitespace), isFalse);
      final link = text.trim().toLowerCase().startsWith('monero:');
      if (!link) {
        // Not a link: the text is the address, and no amount comes with
        // it, whatever it says.
        expect(target.amount, isNull);
        // Taken off the ends first, as the reader does it: the ends lose
        // one character more than the middle does, U+0085.
        expect(target.address, text.trim().replaceAll(RegExp(r'\s+'), ''));
        return;
      }
      // Every character of the address was in the link, in that order,
      // before any question mark.
      final body = text.trim().substring('monero:'.length);
      final mark = body.indexOf('?');
      final named = mark < 0 ? body : body.substring(0, mark);
      expect(target.address, named.replaceAll(RegExp(r'\s+'), ''));
      final amount = target.amount;
      if (amount != null) {
        // A plain number that the link itself names as its amount.
        expect(_plainNumber.hasMatch(amount), isTrue, reason: amount);
        expect(mark, greaterThanOrEqualTo(0));
        expect(
          body.substring(mark + 1).split('&'),
          contains('tx_amount=$amount'),
        );
      }
    });
  });

  test('a made-up phrase is its words in letters, or no words', () {
    final sixteen = [
      for (var i = 0; i < 16; i++) 'word${'abcdefghijklmnop'[i]}',
    ];
    final samples = [
      sixteen.join(' '),
      sixteen.join('  ').toUpperCase(),
      [for (var i = 0; i < 16; i++) '${i + 1}. ${sixteen[i]},'].join('\n'),
      'Ábaco niño café cafe\u0301',
      'alpha.  Beta, gamma',
      'alpha\u00a0beta\u200b gamma',
    ];
    madeUpTexts('phrase', samples, (text) {
      final words = phraseWords(text);
      for (final word in words) {
        expect(_lettersOnly.hasMatch(word), isTrue, reason: jsonEncode(word));
      }
      // Never more words than there were pieces between spaces.
      final pieces = text.split(RegExp(r'\s+')).where((piece) => piece != '');
      expect(words.length, lessThanOrEqualTo(pieces.length));
      // What is handed on reads as the same words again.
      expect(phraseWords(words.join(' ')), words);
    });
  });

  test('made-up lines of bridges and nodes are lines that say something', () {
    final samples = [
      'obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F cert=YXJl iat-mode=0',
      '# a note\n\n192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956\r\n',
      'a.onion:18081, b.onion:18089\nc.onion:38081',
    ];
    madeUpTexts('lines', samples, (text) {
      final bridges = bridgeLines(text);
      for (final line in bridges) {
        expect(line, isNotEmpty);
        expect(line, line.trim());
        expect(line.startsWith('#'), isFalse);
        expect(line.contains('\n'), isFalse);
      }
      // Nothing that was a line of its own is lost or joined to another.
      expect(
        bridges.length,
        text.split('\n').where((line) {
          final said = line.trim();
          return said.isNotEmpty && !said.startsWith('#');
        }).length,
      );
      // Read again as it is kept, it is the same list.
      expect(bridgeLines(bridges.join('\n')), bridges);

      final nodes = nodeLines(text);
      for (final line in nodes) {
        expect(line, isNotEmpty);
        expect(line, line.trim());
        expect(line.contains('\n') || line.contains(','), isFalse);
      }
      expect(nodeLines(nodes.join('\n')), nodes);
    });
  });

  test('a made-up settings file costs settings and nothing else', () async {
    final written = jsonEncode(
      const AppSettings(
        language: 'ru',
        dark: true,
        autoLockMinutes: 15,
        bridges: ['192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956'],
      ).toJson(),
    );
    final samples = [
      written,
      jsonEncode(const AppSettings().toJson()),
      '{"auto_lock_minutes": 5.0}',
      '{"auto_lock_minutes": 0}',
      '{"auto_lock_minutes": 100000}',
      '{"auto_lock_minutes": "15"}',
      '{"bridges": "obfs4"}',
      '{"bridges": [1, null, "a", ["b"]]}',
      '{"language": "de", "dark": "yes"}',
      '[]',
      'null',
      '',
    ];
    final dir = await Directory.systemTemp.createTemp('veil-made-up');
    addTearDown(() => dir.delete(recursive: true));
    final file = File('${dir.path}/settings.json');

    void sane(AppSettings settings) {
      expect(settings.language, anyOf(isNull, 'en', 'ru'));
      // There is no "never", and nothing the user could not have chosen.
      expect(AppSettings.autoLockChoices, contains(settings.autoLockMinutes));
      // What is written from it is read as the same settings.
      final again = AppSettings.fromJson(
        jsonDecode(jsonEncode(settings.toJson())),
      );
      expect(again.toJson(), settings.toJson());
    }

    madeUpTexts('settings', samples, (text) {
      Object? json;
      try {
        json = jsonDecode(text);
      } on FormatException {
        return;
      }
      sane(AppSettings.fromJson(json));
    });

    // And through the file itself, text or not.
    var number = 0;
    await madeUpTextsAsync('settings file', samples, (text) async {
      number++;
      final bytes = number.isEven
          ? utf8.encode(text)
          : [for (final unit in text.codeUnits) unit & 0xff];
      await file.writeAsBytes(bytes, flush: true);
      sane(await AppSettings.load(file.path));
    }, atMost: 300);
  });

  test('settings are replaced whole, never left half written', () async {
    final dir = await Directory.systemTemp.createTemp('veil-settings');
    addTearDown(() => dir.delete(recursive: true));
    final file = '${dir.path}/app/settings.json';
    const first = AppSettings(bridges: ['one bridge'], autoLockMinutes: 15);
    await first.save(file);
    await first.copyWith(dark: true).save(file);
    final kept = await AppSettings.load(file);
    expect(kept.bridges, ['one bridge']);
    expect(kept.dark, isTrue);
    // Nothing else lies next to it afterwards.
    expect(
      [
        for (final entry in Directory('${dir.path}/app').listSync())
          entry.uri.pathSegments.last,
      ],
      ['settings.json'],
    );
  });
}
