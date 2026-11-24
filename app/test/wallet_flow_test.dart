// The way from the start screen to an open wallet, on a wallet that only
// pretends: no core, no files, no Tor.

import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:veil/core/backend.dart';
import 'package:veil/core/phrase.dart';
import 'package:veil/screens/phrase_check_screen.dart';
import 'package:veil/widgets/notice.dart';

import 'support.dart';

final words = phraseWords(testPhrase);

/// The positions the check screen on show asks for.
List<int> askedPositions(WidgetTester tester) => [
  for (var number = 1; number <= words.length; number++)
    if (find.text('Word $number').evaluate().isNotEmpty) number,
];

void main() {
  setUpAll(loadFonts);

  testWidgets('a new wallet: password, phrase, check, and it is open', (
    tester,
  ) async {
    final backend = FakeBackend();
    await pumpApp(tester, 'en', backend: backend);
    await press(tester, 'Create wallet');
    expect(find.text('NEW WALLET'), findsOneWidget);

    // No wallet without a password typed twice the same, and long enough.
    expect(enabled(tester, 'Create wallet'), isFalse);
    await type(tester, 'Password', 'short');
    expect(find.text('At least 8 characters.'), findsOneWidget);
    await type(tester, 'Password', testPassword);
    await type(tester, 'Repeat the password', 'correct horse batter');
    expect(find.text('The two passwords differ.'), findsOneWidget);
    expect(enabled(tester, 'Create wallet'), isFalse);
    expect(backend.calls, isEmpty);
    await type(tester, 'Repeat the password', testPassword);
    expect(find.text('The two passwords differ.'), findsNothing);
    await press(tester, 'Create wallet');
    expect(backend.startedWith, WalletSource.create);
    expect(backend.password, testPassword);

    // The phrase: every word, numbered, under a warning.
    expect(find.text('RECOVERY PHRASE'), findsOneWidget);
    for (var at = 0; at < words.length; at++) {
      expect(find.text('${at + 1}'), findsOneWidget);
    }
    expect(find.text(words.first), findsOneWidget);
    expect(find.text(words.last), findsOneWidget);
    expect(tester.widget<Notice>(find.byType(Notice)).warning, isTrue);
    // The wallet exists now: there is no way back to "create".
    expect(find.text('BACK'), findsNothing);

    // The check asks for three of the words and takes nothing else.
    await press(tester, 'I wrote them down');
    expect(find.text('CHECK'), findsOneWidget);
    final asked = askedPositions(tester);
    expect(asked, hasLength(3));
    expect(enabled(tester, 'Continue'), isFalse);
    await type(tester, 'Word ${asked[0]}', words[asked[0] - 1]);
    await type(tester, 'Word ${asked[1]}', 'zzzzzzzzzz');
    expect(find.textContaining('That is not the word'), findsOneWidget);
    expect(enabled(tester, 'Continue'), isFalse);
    // Back shows the phrase again, and the check starts over.
    await press(tester, 'Back');
    expect(find.text('RECOVERY PHRASE'), findsOneWidget);
    await press(tester, 'I wrote them down');
    final again = askedPositions(tester);
    for (final number in again) {
      // Typed as people type: a capital, a space after.
      final word = words[number - 1];
      await type(
        tester,
        'Word $number',
        '${word[0].toUpperCase()}${word.substring(1)} ',
      );
    }
    expect(find.textContaining('That is not the word'), findsNothing);
    expect(backend.confirmed, isFalse);
    await press(tester, 'Continue');

    expect(backend.confirmed, isTrue);
    expect(find.text('WALLET'), findsOneWidget);
    expect(find.text('Tor is connecting: 15%'), findsOneWidget);
    // Tor's state follows the core's, in words.
    backend.tor(TorState.ready);
    await tester.pumpAndSettle();
    expect(find.text('Tor is connected'), findsOneWidget);
  });

  testWidgets('the words asked for are three different ones, in order', (
    tester,
  ) async {
    for (var seed = 0; seed < 50; seed++) {
      final asked = wordsToCheck(16, random: Random(seed));
      expect(asked.toSet(), hasLength(3));
      expect(asked, orderedEquals([...asked]..sort()));
      expect(asked.every((number) => number >= 1 && number <= 16), isTrue);
    }
    expect(PhraseCheckScreen, isNotNull);
  });

  testWidgets(
    'a wallet that cannot be created says why and lets the user try again',
    (tester) async {
      final backend = FakeBackend()
        ..failNext = const Problem(
          ProblemKind.walletExists,
          'a wallet already exists',
        );
      await pumpApp(tester, 'en', backend: backend);
      await press(tester, 'Create wallet');
      await type(tester, 'Password', testPassword);
      await type(tester, 'Repeat the password', testPassword);
      await press(tester, 'Create wallet');
      expect(
        find.textContaining('There is already a wallet on this device'),
        findsOneWidget,
      );
      expect(find.text('NEW WALLET'), findsOneWidget);
      expect(enabled(tester, 'Create wallet'), isTrue);
    },
  );

  testWidgets('a wallet restored from sixteen words is open at once', (
    tester,
  ) async {
    final backend = FakeBackend();
    await pumpApp(tester, 'en', backend: backend);
    await press(tester, 'Restore from recovery phrase');
    expect(find.text('RESTORE'), findsOneWidget);
    expect(find.textContaining('No words yet'), findsOneWidget);

    await type(tester, 'Recovery phrase', 'unaware yard donate');
    expect(find.textContaining('3 words'), findsOneWidget);
    await type(tester, 'Password', testPassword);
    await type(tester, 'Repeat the password', testPassword);
    // Three words are not a phrase.
    expect(enabled(tester, 'Restore wallet'), isFalse);

    // Typed carelessly: capitals, line breaks, double spaces.
    await type(
      tester,
      'Recovery phrase',
      '  ${testPhrase.toUpperCase().replaceAll(' ', '  \n')} ',
    );
    expect(find.textContaining('16 words'), findsOneWidget);
    // Sixteen words carry their own date: no block number is asked for.
    expect(find.text('Block to scan from (optional)'), findsNothing);
    await press(tester, 'Restore wallet');

    expect(backend.startedWith, WalletSource.restore);
    expect(backend.restoredPhrase, testPhrase);
    expect(backend.restoredHeight, isNull);
    // Whoever restored from the phrase has it: no check.
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.confirmed, isTrue);
  });

  testWidgets('twenty-five words may come with a block to scan from', (
    tester,
  ) async {
    final backend = FakeBackend();
    final legacy = List.filled(25, 'abbey').join(' ');
    await pumpApp(tester, 'en', backend: backend);
    await press(tester, 'Restore from recovery phrase');
    await type(tester, 'Recovery phrase', legacy);
    expect(find.textContaining('25 words'), findsOneWidget);
    expect(find.text('Block to scan from (optional)'), findsOneWidget);
    // Leaving the block out is allowed, and the cost is said in so many
    // words, as a warning.
    Notice costNotice() => tester.widget<Notice>(
      find.ancestor(
        of: find.textContaining('can take more than an hour'),
        matching: find.byType(Notice),
      ),
    );
    expect(find.textContaining('nothing is missed'), findsOneWidget);
    expect(costNotice().warning, isTrue);
    await type(tester, 'Password', testPassword);
    await type(tester, 'Repeat the password', testPassword);
    await type(tester, 'Block to scan from (optional)', 'spring');
    expect(enabled(tester, 'Restore wallet'), isFalse);
    await type(tester, 'Block to scan from (optional)', '1234567');
    // With a block given it is an explanation and no longer a warning.
    expect(costNotice().warning, isFalse);
    await press(tester, 'Restore wallet');
    expect(backend.restoredHeight, 1234567);
  });

  testWidgets(
    'twenty-five words without a block are restored from the first one',
    (tester) async {
      final backend = FakeBackend();
      await pumpApp(tester, 'ru', backend: backend);
      await press(tester, 'Восстановить по фразе');
      await type(
        tester,
        'Фраза восстановления',
        List.filled(25, 'abbey').join(' '),
      );
      expect(find.textContaining('может занять больше часа'), findsOneWidget);
      await type(tester, 'Пароль', testPassword);
      await type(tester, 'Пароль ещё раз', testPassword);
      await press(tester, 'Восстановить кошелёк');
      expect(backend.startedWith, WalletSource.restore);
      expect(backend.restoredHeight, isNull);
    },
  );

  testWidgets('a phrase the wallet refuses is said to be the reason', (
    tester,
  ) async {
    final backend = FakeBackend()
      ..failNext = const Problem(
        ProblemKind.invalidPhrase,
        'checksum mismatch',
      );
    await pumpApp(tester, 'en', backend: backend);
    await press(tester, 'Restore from recovery phrase');
    await type(tester, 'Recovery phrase', testPhrase);
    await type(tester, 'Password', testPassword);
    await type(tester, 'Repeat the password', testPassword);
    await press(tester, 'Restore wallet');
    expect(
      find.textContaining('This is not a recovery phrase'),
      findsOneWidget,
    );
    expect(find.text('RESTORE'), findsOneWidget);
    // What was typed is still there to be corrected.
    expect(find.textContaining('16 words'), findsOneWidget);
  });

  testWidgets('an existing wallet opens with its password and no other', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    expect(enabled(tester, 'Open wallet'), isFalse);
    await type(tester, 'Password', 'not the password');
    await press(tester, 'Open wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(find.text('OPEN WALLET'), findsOneWidget);

    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('Wrong password.'), findsNothing);
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.startedWith, WalletSource.open);
  });

  testWidgets('after a wrong password the next one is typed straight away', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    backend.takes = const Duration(milliseconds: 300);
    // The field has the keyboard from the start, and keeps it through a
    // refusal, with the refused password marked to be typed over.
    await typeOn(tester, 'not the password');
    await pressAndWait(tester, 'Open wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    expect(allMarked(tester), isTrue);
    await typeOn(tester, 'nor this one');
    await pressAndWait(tester, 'Open wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    await typeOn(tester, testPassword);
    await pressAndWait(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('a failure nobody foresaw is shown, and the screen goes on', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    backend.breakNext = ArgumentError('something that cannot happen');
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    // Not "Opening…" for ever: what happened is said, and the wallet can
    // be opened by trying again.
    expect(find.textContaining('something that cannot happen'), findsOneWidget);
    expect(enabled(tester, 'Open wallet'), isTrue);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('a wallet whose phrase was never checked shows it again first', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: false);
    await pumpApp(tester, 'en', backend: backend);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('RECOVERY PHRASE'), findsOneWidget);
    expect(find.text(words.first), findsOneWidget);
  });

  testWidgets('a wallet rebuilt from its keys says so, and waits to be told', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    backend.tor(TorState.ready, rebuilt: true);
    await tester.pumpAndSettle();
    expect(find.textContaining('opened from its keys'), findsOneWidget);
    // A node of the user's own went with the file. The wallet says so,
    // and does not go to the public nodes before it is told to.
    expect(find.textContaining('set it again in Settings'), findsOneWidget);
    expect(backend.calls, isNot(contains('sync')));
    await press(tester, 'Synchronise with the public nodes');
    expect(backend.calls.last, 'sync');
    // Once it synchronises there is nothing left to ask.
    expect(find.text('SYNCHRONISE WITH THE PUBLIC NODES'), findsNothing);
    expect(find.textContaining('opened from its keys'), findsOneWidget);
  });

  testWidgets('the warnings on Home fit the window in Russian too', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'ru');
    backend.report(
      rebuilt: true,
      notSaved: const Problem(ProblemKind.other, 'failed to save file'),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('открыт по ключам'), findsOneWidget);
    expect(find.textContaining('не удалось сохранить на диск'), findsOneWidget);
    expect(find.text('СИНХРОНИЗИРОВАТЬ ЧЕРЕЗ ПУБЛИЧНЫЕ УЗЛЫ'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a phrase that was never checked is not skipped', (tester) async {
    // The wallet's phrase was never checked, and now it cannot be read.
    // The wallet is not opened as if nothing were owed: it is closed, the
    // screen says so, and the next attempt shows the phrase.
    final backend = FakeBackend(hasWallet: true, confirmed: false);
    await pumpApp(tester, 'en', backend: backend);
    backend.noPhrase = true;
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsNothing);
    expect(find.textContaining('no recovery phrase'), findsOneWidget);
    expect(backend.state.value, isNull);
    expect(backend.calls.last, 'close');

    backend.noPhrase = false;
    await press(tester, 'Open wallet');
    expect(find.text('RECOVERY PHRASE'), findsOneWidget);
  });

  testWidgets('a wallet that could not be saved says so, open and closed', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    expect(find.textContaining('could not be saved'), findsNothing);
    backend.report(
      notSaved: const Problem(ProblemKind.other, 'no space left on device'),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('could not be saved to disk'), findsOneWidget);
    expect(find.textContaining('no space left on device'), findsOneWidget);

    // Locked like that, the screen it leads to says what was lost.
    backend.notSavedOnClose = const Problem(ProblemKind.other, 'disk full');
    await press(tester, 'Lock wallet');
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(
      find.textContaining('could not be saved when it was closed'),
      findsOneWidget,
    );
    expect(find.textContaining('disk full'), findsOneWidget);
  });

  testWidgets('locking that fails does not leave the screen closing for ever', (
    tester,
  ) async {
    final backend = await openWallet(tester, 'en');
    backend.breakClose = StateError('the session was closed already');
    await press(tester, 'Lock wallet');
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(
      find.textContaining('the session was closed already'),
      findsOneWidget,
    );
    // And the wallet opens again from there.
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
  });

  testWidgets('every screen fits the window, in Russian too', (tester) async {
    final backend = FakeBackend();
    await pumpApp(tester, 'ru', backend: backend);
    await press(tester, 'Создать кошелёк');
    await type(tester, 'Пароль', testPassword);
    await type(tester, 'Пароль ещё раз', testPassword);
    await press(tester, 'Создать кошелёк');
    expect(find.text('ФРАЗА ВОССТАНОВЛЕНИЯ'), findsOneWidget);
    await press(tester, 'Слова записаны');
    expect(find.text('ПРОВЕРКА'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a wallet can be locked, and opens again with its password', (
    tester,
  ) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.state.value, isNotNull);

    // Locking saves and closes the wallet; the app stays, at the password.
    await press(tester, 'Lock wallet');
    expect(backend.calls.last, 'close');
    expect(backend.state.value, isNull);
    expect(find.text('OPEN WALLET'), findsOneWidget);
    expect(find.text('WALLET'), findsNothing);
    // The field is empty: the password typed before is not kept.
    expect(enabled(tester, 'Open wallet'), isFalse);
    // There is no way back to the open wallet but the password.
    expect(find.text('BACK'), findsNothing);

    await type(tester, 'Password', 'not the password');
    await press(tester, 'Open wallet');
    expect(find.text('Wrong password.'), findsOneWidget);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    expect(find.text('WALLET'), findsOneWidget);
    expect(backend.state.value, isNotNull);
  });

  testWidgets('locking works in Russian too', (tester) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'ru', backend: backend);
    await type(tester, 'Пароль', testPassword);
    await press(tester, 'Открыть кошелёк');
    await press(tester, 'Заблокировать кошелёк');
    expect(find.text('ОТКРЫТЬ КОШЕЛЁК'), findsOneWidget);
  });

  testWidgets('closing the window closes the wallet first', (tester) async {
    final backend = FakeBackend(hasWallet: true, confirmed: true);
    await pumpApp(tester, 'en', backend: backend);
    await type(tester, 'Password', testPassword);
    await press(tester, 'Open wallet');
    final response = await tester.binding.handleRequestAppExit();
    expect(response.name, 'exit');
    expect(backend.calls.last, 'close');
  });
}
