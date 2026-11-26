//! Recovery phrases.
//!
//! This module only carries a phrase around and tells the two supported
//! formats apart by word count. Checking the words and deriving keys is done
//! by the wallet library.

use std::fmt;

use zeroize::Zeroizing;

use crate::Error;

/// The format of a recovery phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedKind {
    /// 16 words. Carries the wallet's creation date.
    Polyseed,
    /// The classic 25-word Monero phrase. Needs a restore height.
    Legacy,
}

/// A recovery phrase. The text is wiped from memory when this is dropped and
/// is never shown by `Debug`.
#[derive(Clone)]
pub struct SeedPhrase {
    words: Zeroizing<String>,
    kind: SeedKind,
}

impl SeedPhrase {
    /// Accepts a phrase typed or pasted by the user. Extra whitespace and
    /// letter case are normalised. The words themselves are not checked here.
    pub fn parse(text: &str) -> Result<SeedPhrase, Error> {
        let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        let kind = match words.len() {
            16 => SeedKind::Polyseed,
            25 => SeedKind::Legacy,
            count => return Err(Error::SeedWordCount { words: count }),
        };
        let joined = Zeroizing::new(words.join(" "));
        // The intermediate copies hold the phrase too.
        for mut word in words {
            zeroize::Zeroize::zeroize(&mut word);
        }
        Ok(SeedPhrase {
            words: joined,
            kind,
        })
    }

    pub fn kind(&self) -> SeedKind {
        self.kind
    }

    /// The phrase as single-spaced lowercase words. Handle with care.
    pub fn as_str(&self) -> &str {
        &self.words
    }

    /// The words in order, for display.
    pub fn words(&self) -> impl Iterator<Item = &str> {
        self.words.split(' ')
    }
}

impl fmt::Debug for SeedPhrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SeedPhrase({:?}, <hidden>)", self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(count: usize) -> String {
        (0..count)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn tells_formats_apart_by_word_count() {
        assert_eq!(
            SeedPhrase::parse(&phrase(16)).unwrap().kind(),
            SeedKind::Polyseed
        );
        assert_eq!(
            SeedPhrase::parse(&phrase(25)).unwrap().kind(),
            SeedKind::Legacy
        );
    }

    #[test]
    fn rejects_other_word_counts() {
        for count in [0, 1, 12, 13, 15, 17, 24, 26] {
            match SeedPhrase::parse(&phrase(count)) {
                Err(Error::SeedWordCount { words }) => assert_eq!(words, count),
                other => panic!("{count} words: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn normalises_whitespace_and_case() {
        let messy = format!("  {}\n", phrase(16).to_uppercase().replace(' ', " \t "));
        let seed = SeedPhrase::parse(&messy).unwrap();
        assert_eq!(seed.as_str(), phrase(16));
        assert_eq!(seed.words().count(), 16);
    }

    #[test]
    fn debug_output_hides_the_words() {
        let seed = SeedPhrase::parse(&phrase(16)).unwrap();
        let shown = format!("{seed:?}");
        assert!(!shown.contains("word0"), "{shown}");
        assert!(shown.contains("hidden"));
    }

    #[test]
    fn made_up_phrases_are_counted_and_never_shown() {
        let (sixteen, twenty_five) = (phrase(16), phrase(25));
        let untidy = format!("  {}\t\n", phrase(16).to_uppercase().replace(' ', "  "));
        let samples = [sixteen.as_str(), twenty_five.as_str(), untidy.as_str()];
        veil_test_guard::fuzz::texts("phrase", &samples, |text| {
            let count = text.split_whitespace().count();
            match SeedPhrase::parse(text) {
                Ok(phrase) => {
                    let kind = match count {
                        16 => SeedKind::Polyseed,
                        25 => SeedKind::Legacy,
                        other => panic!("{other} words were taken for a phrase"),
                    };
                    assert_eq!(phrase.kind(), kind);
                    // The same words, in order, in small letters, with
                    // one space between them and none around them.
                    let words: Vec<String> =
                        text.split_whitespace().map(str::to_lowercase).collect();
                    assert_eq!(phrase.as_str(), words.join(" "));
                    assert!(phrase.words().eq(words.iter().map(String::as_str)));
                    // Nothing of it in what a log would get.
                    assert_eq!(
                        format!("{phrase:?}"),
                        format!("SeedPhrase({kind:?}, <hidden>)")
                    );
                }
                Err(Error::SeedWordCount { words }) => {
                    assert_eq!(words, count);
                    assert!(count != 16 && count != 25);
                }
                Err(other) => panic!("an unexpected refusal: {other}"),
            }
        });
    }
}
