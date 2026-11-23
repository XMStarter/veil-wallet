import 'dart:math';

/// The words of a phrase as typed: any spacing, any case.
List<String> phraseWords(String text) => text
    .toLowerCase()
    .split(RegExp(r'\s+'))
    .where((word) => word.isNotEmpty)
    .toList();

/// Whether [count] words can be a recovery phrase at all: 16 or 25.
bool isPhraseLength(int count) => count == 16 || count == 25;

/// Which words of a phrase of [length] words to ask for, as positions
/// starting at 1, in ascending order.
List<int> wordsToCheck(int length, {int count = 3, Random? random}) {
  final positions = List.generate(length, (index) => index + 1)
    ..shuffle(random ?? Random.secure());
  return positions.take(count).toList()..sort();
}

/// Whether [typed] is the word the user was asked for.
bool sameWord(String typed, String expected) =>
    typed.trim().toLowerCase() == expected;
