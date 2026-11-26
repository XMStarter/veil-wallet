//! Made-up input for whatever reads text or bytes from outside.
//!
//! A parser is tested with the inputs its author thought of. This hands it
//! the others: thousands of texts made by damaging good ones, by joining
//! two, and from nothing, with the characters that tend to break things
//! (nothing at all, very long runs, signs, separators, other scripts,
//! characters that are no text). The test says what must hold for every
//! one of them: at the least that nothing panics, and usually more, such
//! as that whatever is accepted comes out the same when written and read
//! again.
//!
//! The inputs come from a seed, so a run can be repeated exactly. With
//! nothing set, the seed is fixed and every run of the tests sees the same
//! inputs, which keeps the ordinary checks from failing one day in ten.
//! `../check.sh fuzz` sets a new seed (`VEIL_FUZZ_SEED`) and a time to run
//! for (`VEIL_FUZZ_SECONDS`); a failure prints the seed, the number of the
//! case and the input, and the test run by itself with that seed and
//! `VEIL_FUZZ_CASES` one above that number fails on it again.
//!
//! This is not coverage-guided fuzzing: nothing here watches which parts
//! of the code an input reaches and steers towards the rest. It finds what
//! damaged and random input finds, which for parsers of this size is most
//! of what there is to find, and it runs wherever the tests run.

use std::time::{Duration, Instant};

/// How many inputs a test is given when nothing says otherwise.
const DEFAULT_CASES: u64 = 3_000;

/// The seed every run uses unless `VEIL_FUZZ_SEED` names another.
const DEFAULT_SEED: u64 = 0x005e_ed0f_7e11;

/// Characters that parsers trip over, besides the ones in the samples.
const AWKWARD: &[&str] = &[
    "",
    " ",
    "  ",
    "\t",
    "\n",
    "\r\n",
    "\0",
    "\u{feff}",
    "\u{200b}",
    "\u{202e}",
    ".",
    ",",
    ":",
    ";",
    "-",
    "+",
    "_",
    "/",
    "\\",
    "?",
    "&",
    "=",
    "#",
    "%",
    "@",
    "'",
    "\"",
    "{",
    "}",
    "[",
    "]",
    "0",
    "1",
    "9",
    "00",
    "-1",
    "1e9",
    "0x10",
    "18446744073709551615",
    "18446744073709551616",
    "99999999999999999999999999999999",
    "a",
    "A",
    "z",
    "é",
    "ß",
    "я",
    "中",
    "🙂",
    "\u{301}",
    "null",
    "true",
    "onion",
    ".onion",
    "monero:",
    "obfs4",
    "http://",
    "%00",
    "%s",
    "..",
];

/// A source of numbers that can be started again from its seed
/// (SplitMix64: small, well spread, and no dependency).
pub struct Numbers(u64);

impl Numbers {
    pub fn new(seed: u64) -> Numbers {
        Numbers(seed)
    }

    pub fn number(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `limit`; zero when `limit` is zero.
    pub fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            0
        } else {
            (self.number() % limit as u64) as usize
        }
    }

    /// One of `items`.
    pub fn pick<'a, T: ?Sized>(&mut self, items: &[&'a T]) -> &'a T {
        items[self.below(items.len())]
    }
}

/// The seed, how many cases, and for how long, as the environment says.
struct Plan {
    seed: u64,
    /// How many cases, when the environment names a number.
    cases: Option<u64>,
    until: Option<Instant>,
}

impl Plan {
    fn from_environment() -> Plan {
        let number = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|text| text.trim().parse::<u64>().ok())
        };
        let seconds = number("VEIL_FUZZ_SECONDS");
        Plan {
            seed: number("VEIL_FUZZ_SEED").unwrap_or(DEFAULT_SEED),
            cases: number("VEIL_FUZZ_CASES"),
            until: seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds)),
        }
    }
}

impl Plan {
    /// Whether case number `case` is still to be run, for a test that
    /// wants `at_most` cases when nothing else is said. A time to run for
    /// ends the run when it is up. A number of cases that was asked for
    /// is run in full, whatever the test wants: that is how a failure
    /// far into a long run is brought back.
    fn goes_on(&self, case: u64, at_most: u64) -> bool {
        match (self.until, self.cases) {
            (Some(until), _) => Instant::now() < until,
            (None, Some(cases)) => case < cases,
            (None, None) => case < DEFAULT_CASES.min(at_most),
        }
    }
}

/// Says which input a test was on when it failed. A test that panics has
/// no other way to tell: the panic's own message knows nothing of the
/// input.
struct Witness<'a> {
    name: &'a str,
    seed: u64,
    case: u64,
    input: String,
}

impl Drop for Witness<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "made-up input {} of `{}` failed \
                 (VEIL_FUZZ_SEED={} VEIL_FUZZ_CASES={} bring it back):\n  {}",
                self.case,
                self.name,
                self.seed,
                self.case + 1,
                self.input
            );
        }
    }
}

/// Hands `check` texts made from `samples`, which should be inputs the
/// code under test accepts, and from nothing. The samples themselves come
/// first, as they are. `check` panics when something that must hold does
/// not. `name` tells the tests apart, so that each gets inputs of its own
/// from the same seed.
pub fn texts(name: &str, samples: &[&str], check: impl Fn(&str)) {
    texts_as(&Plan::from_environment(), name, samples, u64::MAX, check);
}

/// Like [`texts`], for a test whose every case costs something: no more
/// than `cases_at_most` of them unless a time to run for is set.
pub fn some_texts(name: &str, samples: &[&str], cases_at_most: u64, check: impl Fn(&str)) {
    texts_as(
        &Plan::from_environment(),
        name,
        samples,
        cases_at_most,
        check,
    );
}

fn texts_as(plan: &Plan, name: &str, samples: &[&str], cases_at_most: u64, check: impl Fn(&str)) {
    let mut numbers = Numbers::new(plan.seed ^ hash(name));
    let mut case = 0;
    while plan.goes_on(case, cases_at_most) {
        let input = match samples.get(case as usize) {
            Some(sample) => (*sample).to_owned(),
            None => text(&mut numbers, samples),
        };
        let witness = Witness {
            name,
            seed: plan.seed,
            case,
            input: format!("{input:?}"),
        };
        check(&input);
        drop(witness);
        case += 1;
    }
}

/// Like [`texts`], for code that reads bytes that need not be text.
/// `cases_at_most` keeps a test whose every case costs something, a
/// connection for example, from running thousands of them by default; a
/// run with a time to run for goes on until the time is up.
pub fn bytes(name: &str, samples: &[&[u8]], cases_at_most: u64, check: impl Fn(&[u8])) {
    bytes_as(
        &Plan::from_environment(),
        name,
        samples,
        cases_at_most,
        check,
    );
}

fn bytes_as(plan: &Plan, name: &str, samples: &[&[u8]], cases_at_most: u64, check: impl Fn(&[u8])) {
    let mut numbers = Numbers::new(plan.seed ^ hash(name));
    let mut case = 0;
    while plan.goes_on(case, cases_at_most) {
        let input = match samples.get(case as usize) {
            Some(sample) => sample.to_vec(),
            None => damaged(&mut numbers, samples),
        };
        let witness = Witness {
            name,
            seed: plan.seed,
            case,
            input: shown(&input),
        };
        check(&input);
        drop(witness);
        case += 1;
    }
}

/// Bytes as a failure shows them: what is text as text, the rest as
/// escapes, and the end left off if it is long.
fn shown(bytes: &[u8]) -> String {
    const MOST: usize = 600;
    let start = &bytes[..bytes.len().min(MOST)];
    let more = match bytes.len() - start.len() {
        0 => String::new(),
        more => format!(" and {more} more bytes"),
    };
    format!("b\"{}\"{more}", start.escape_ascii())
}

/// A number for a name, so that tests do not share their inputs.
fn hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// One made-up text.
fn text(numbers: &mut Numbers, samples: &[&str]) -> String {
    let sample = |numbers: &mut Numbers| {
        if samples.is_empty() {
            String::new()
        } else {
            numbers.pick(samples).to_owned()
        }
    };
    match numbers.below(10) {
        // A good one, damaged once or a few times: most of what is found
        // is found one step from what is accepted.
        0..=5 => {
            let mut text = sample(numbers);
            for _ in 0..=numbers.below(3) {
                text = damage(numbers, &text);
            }
            text
        }
        // Two good ones, each cut somewhere, end to end.
        6 => {
            let (first, second) = (sample(numbers), sample(numbers));
            let (first, second): (Vec<char>, Vec<char>) =
                (first.chars().collect(), second.chars().collect());
            let cut_first = numbers.below(first.len() + 1);
            let cut_second = numbers.below(second.len() + 1);
            first[..cut_first]
                .iter()
                .chain(&second[cut_second..])
                .collect()
        }
        // Awkward pieces and nothing else.
        7 | 8 => (0..numbers.below(12))
            .map(|_| numbers.pick(AWKWARD))
            .collect(),
        // Bytes that are mostly no text at all.
        _ => {
            let bytes: Vec<u8> = (0..numbers.below(64))
                .map(|_| numbers.number() as u8)
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }
}

/// `text` with one thing done to it.
fn damage(numbers: &mut Numbers, text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let at = numbers.below(chars.len() + 1);
    let piece: Vec<char> = numbers.pick(AWKWARD).chars().collect();
    match numbers.below(8) {
        // Something put in.
        0 | 1 => {
            chars.splice(at..at, piece);
        }
        // Something taken out.
        2 => {
            let end = (at + 1 + numbers.below(4)).min(chars.len());
            chars.drain(at.min(end)..end);
        }
        // One character changed for another.
        3 if !chars.is_empty() => {
            let at = at.min(chars.len() - 1);
            chars.splice(at..=at, piece);
        }
        // Its case changed.
        4 if !chars.is_empty() => {
            let at = at.min(chars.len() - 1);
            let c = chars[at];
            chars[at] = if c.is_uppercase() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c.to_uppercase().next().unwrap_or(c)
            };
        }
        // A part said twice.
        5 => {
            let end = (at + numbers.below(8)).min(chars.len());
            let part: Vec<char> = chars[at.min(end)..end].to_vec();
            chars.splice(at..at, part);
        }
        // Cut short.
        6 => chars.truncate(at),
        // A piece repeated until it is long.
        _ => {
            let long = piece
                .iter()
                .cycle()
                .take(piece.len() * (1 + numbers.below(400)));
            chars.splice(at..at, long.copied().collect::<Vec<char>>());
        }
    }
    chars.into_iter().collect()
}

/// One made-up run of bytes: a sample damaged, or bytes from nothing.
fn damaged(numbers: &mut Numbers, samples: &[&[u8]]) -> Vec<u8> {
    let mut bytes: Vec<u8> = if samples.is_empty() || numbers.below(6) == 0 {
        (0..numbers.below(96))
            .map(|_| numbers.number() as u8)
            .collect()
    } else {
        numbers.pick(samples).to_vec()
    };
    for _ in 0..numbers.below(4) {
        let at = numbers.below(bytes.len() + 1);
        match numbers.below(6) {
            0 => bytes.insert(at, numbers.number() as u8),
            1 if !bytes.is_empty() => {
                bytes.remove(at.min(bytes.len() - 1));
            }
            2 if !bytes.is_empty() => {
                let at = at.min(bytes.len() - 1);
                bytes[at] ^= 1 << numbers.below(8);
            }
            3 if !bytes.is_empty() => {
                let at = at.min(bytes.len() - 1);
                bytes[at] = *numbers.pick(&[&0u8, &1, &0x7f, &0x80, &0xff]);
            }
            4 => bytes.truncate(at),
            _ => {
                let more: Vec<u8> = (0..numbers.below(300))
                    .map(|_| numbers.number() as u8)
                    .collect();
                bytes.splice(at..at, more);
            }
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The plan of a run with nothing set, whatever is set around the
    /// tests: `check.sh fuzz` runs these too.
    const PLAN: Plan = Plan {
        seed: DEFAULT_SEED,
        cases: None,
        until: None,
    };

    #[test]
    fn a_number_of_cases_asked_for_is_run_whatever_the_test_wants() {
        let asked = Plan {
            cases: Some(450),
            ..PLAN
        };
        let seen = RefCell::new(0);
        bytes_as(&asked, "asked", &[b"x"], 200, |_| *seen.borrow_mut() += 1);
        assert_eq!(seen.into_inner(), 450);
        let seen = RefCell::new(0);
        texts_as(&asked, "asked", &["x"], 200, |_| *seen.borrow_mut() += 1);
        assert_eq!(seen.into_inner(), 450);
    }

    #[test]
    fn the_same_seed_gives_the_same_inputs() {
        let run = |name: &str| {
            let seen = RefCell::new(Vec::new());
            texts_as(&PLAN, name, &["good", "also good"], u64::MAX, |input| {
                seen.borrow_mut().push(input.to_owned())
            });
            seen.into_inner()
        };
        let first = run("one test");
        assert_eq!(first, run("one test"));
        // Another test gets other inputs from the same seed.
        assert_ne!(first, run("another test"));
        assert_eq!(first.len() as u64, DEFAULT_CASES);
    }

    #[test]
    fn the_inputs_are_of_every_kind() {
        let seen = RefCell::new(Vec::new());
        texts_as(&PLAN, "kinds", &["0.25", "1"], u64::MAX, |input| {
            seen.borrow_mut().push(input.to_owned())
        });
        let seen = seen.into_inner();
        let some = |what: &dyn Fn(&String) -> bool| seen.iter().any(what);
        assert!(some(&|input| input.is_empty()));
        assert!(
            some(&|input| input == "0.25" || input == "1"),
            "a sample left whole"
        );
        assert!(some(&|input| input.len() > 1_000), "something long");
        assert!(some(&|input| input.contains('\0')));
        assert!(some(&|input| !input.is_ascii()));
        assert!(
            some(&|input| input.contains('\u{fffd}')),
            "bytes that were no text"
        );
        // Most are near a sample: that is where parsers go wrong.
        let near = seen
            .iter()
            .filter(|input| input.contains('.') || input.contains('1'))
            .count();
        assert!(near > seen.len() / 3, "{near} of {}", seen.len());
    }

    #[test]
    fn bytes_are_made_too_and_no_more_than_asked_for() {
        let seen = RefCell::new(Vec::new());
        bytes_as(&PLAN, "bytes", &[b"\x05\x01\x00"], 200, |input| {
            seen.borrow_mut().push(input.to_vec())
        });
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 200);
        assert!(seen.iter().any(|input| input.is_empty()));
        assert!(seen.iter().any(|input| input.as_slice() == b"\x05\x01\x00"));
        assert!(seen.iter().any(|input| input.len() > 100));
    }

    #[test]
    fn numbers_stay_below_their_limit() {
        let mut numbers = Numbers::new(7);
        assert_eq!(numbers.below(0), 0);
        assert!((0..1_000).all(|_| numbers.below(3) < 3));
        let mut again = Numbers::new(7);
        assert_eq!(again.below(0), 0);
        assert_eq!(Numbers::new(1).number(), Numbers::new(1).number());
        assert_ne!(Numbers::new(1).number(), Numbers::new(2).number());
    }
}
