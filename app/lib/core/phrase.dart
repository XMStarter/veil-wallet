import 'dart:math';

/// The words of a phrase as typed or pasted: any spacing, any case, and
/// without what is not part of a word. A phone's keyboard puts a full stop
/// after two spaces, and a phrase copied from a note may come numbered or
/// with commas; a word of a phrase is letters and nothing else.
List<String> phraseWords(String text) => text
    .toLowerCase()
    .split(RegExp(r'\s+'))
    .map((word) => word.replaceAll(_notOfAWord, ''))
    .where((word) => word.isNotEmpty)
    .toList();

/// Anything that is neither a letter nor a mark that belongs to one, such
/// as an accent written as a character of its own.
final _notOfAWord = RegExp(r'[^\p{L}\p{M}]', unicode: true);

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
