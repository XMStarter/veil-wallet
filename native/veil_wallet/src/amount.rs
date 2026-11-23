//! Monero amounts as exact integers.
//!
//! One XMR is 10^12 atomic units. Amounts are never held as floating-point
//! numbers, because a rounding error here is a wrong payment.

use std::fmt;
use std::str::FromStr;

/// Number of decimal places in one XMR.
const DECIMALS: usize = 12;
/// Atomic units in one XMR.
const UNITS_PER_XMR: u64 = 1_000_000_000_000;

/// An amount of Monero in atomic units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Amount(u64);

impl Amount {
    pub const ZERO: Amount = Amount(0);

    pub const fn from_atomic(units: u64) -> Self {
        Amount(units)
    }

    pub const fn as_atomic(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, other: Amount) -> Option<Amount> {
        self.0.checked_add(other.0).map(Amount)
    }

    pub fn checked_sub(self, other: Amount) -> Option<Amount> {
        self.0.checked_sub(other.0).map(Amount)
    }
}

/// Formats as XMR with at least two decimals and no trailing zeros beyond
/// them, for example `0.25`, `1.00`, `0.000000000001`.
impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / UNITS_PER_XMR;
        let fraction = format!("{:012}", self.0 % UNITS_PER_XMR);
        let trimmed = fraction.trim_end_matches('0');
        let shown = if trimmed.len() < 2 {
            &fraction[..2]
        } else {
            trimmed
        };
        write!(f, "{whole}.{shown}")
    }
}

/// Why a string is not an amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseAmountError {
    #[error("the amount is empty")]
    Empty,
    #[error("the amount may contain only digits and one decimal separator")]
    InvalidCharacter,
    #[error("Monero has 12 decimal places, the amount has more")]
    TooManyDecimals,
    #[error("the amount is too large")]
    TooLarge,
}

/// Parses a decimal XMR amount such as `0.25` or `1`. A comma is accepted as
/// the decimal separator. Signs, exponents, spaces inside the number, and
/// digit grouping are rejected.
impl FromStr for Amount {
    type Err = ParseAmountError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ParseAmountError::Empty);
        }

        let mut parts = text.split(['.', ',']);
        let whole = parts.next().unwrap_or("");
        let fraction = parts.next().unwrap_or("");
        if parts.next().is_some() {
            return Err(ParseAmountError::InvalidCharacter);
        }
        if whole.is_empty() && fraction.is_empty() {
            return Err(ParseAmountError::InvalidCharacter);
        }
        let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
        if !all_digits(whole) || !all_digits(fraction) {
            return Err(ParseAmountError::InvalidCharacter);
        }
        if fraction.len() > DECIMALS {
            return Err(ParseAmountError::TooManyDecimals);
        }

        let whole_units = if whole.is_empty() {
            0
        } else {
            whole
                .parse::<u64>()
                .ok()
                .and_then(|w| w.checked_mul(UNITS_PER_XMR))
                .ok_or(ParseAmountError::TooLarge)?
        };
        let fraction_units = if fraction.is_empty() {
            0
        } else {
            let padded = format!("{fraction:0<width$}", width = DECIMALS);
            padded
                .parse::<u64>()
                .map_err(|_| ParseAmountError::TooLarge)?
        };
        whole_units
            .checked_add(fraction_units)
            .map(Amount)
            .ok_or(ParseAmountError::TooLarge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<u64, ParseAmountError> {
        text.parse::<Amount>().map(Amount::as_atomic)
    }

    #[test]
    fn parses_ordinary_amounts() {
        assert_eq!(parse("1"), Ok(1_000_000_000_000));
        assert_eq!(parse("0.25"), Ok(250_000_000_000));
        assert_eq!(parse("0,25"), Ok(250_000_000_000));
        assert_eq!(parse(".5"), Ok(500_000_000_000));
        assert_eq!(parse("5."), Ok(5_000_000_000_000));
        assert_eq!(parse(" 2.5 "), Ok(2_500_000_000_000));
        assert_eq!(parse("0"), Ok(0));
        assert_eq!(parse("0.000000000001"), Ok(1));
        assert_eq!(parse("007.10"), Ok(7_100_000_000_000));
    }

    #[test]
    fn parses_the_largest_amount() {
        assert_eq!(parse("18446744.073709551615"), Ok(u64::MAX));
    }

    #[test]
    fn rejects_amounts_that_do_not_fit() {
        assert_eq!(
            parse("18446744.073709551616"),
            Err(ParseAmountError::TooLarge)
        );
        assert_eq!(parse("18446745"), Err(ParseAmountError::TooLarge));
        assert_eq!(
            parse("99999999999999999999999"),
            Err(ParseAmountError::TooLarge)
        );
    }

    #[test]
    fn rejects_more_than_twelve_decimals() {
        assert_eq!(
            parse("0.0000000000001"),
            Err(ParseAmountError::TooManyDecimals)
        );
        // Not silently rounded, even when the extra digit is zero.
        assert_eq!(
            parse("1.0000000000000"),
            Err(ParseAmountError::TooManyDecimals)
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse(""), Err(ParseAmountError::Empty));
        assert_eq!(parse("   "), Err(ParseAmountError::Empty));
        for text in [
            ".", ",", "-1", "+1", "1e3", "1.2.3", "1,2,3", "1 000", "1_000", "abc", "0x10", "１",
            "1.2,3",
        ] {
            assert_eq!(
                parse(text),
                Err(ParseAmountError::InvalidCharacter),
                "{text:?}"
            );
        }
    }

    #[test]
    fn formats_with_at_least_two_decimals() {
        let show = |units: u64| Amount::from_atomic(units).to_string();
        assert_eq!(show(0), "0.00");
        assert_eq!(show(1_000_000_000_000), "1.00");
        assert_eq!(show(250_000_000_000), "0.25");
        assert_eq!(show(100_000_000_000), "0.10");
        assert_eq!(show(1), "0.000000000001");
        assert_eq!(show(1_304_000_000_000), "1.304");
        assert_eq!(show(u64::MAX), "18446744.073709551615");
    }

    #[test]
    fn formatting_and_parsing_round_trip() {
        for units in [
            0,
            1,
            9,
            10,
            999_999_999_999,
            1_000_000_000_000,
            1_000_000_000_001,
            123_456_789_012_345,
            u64::MAX,
        ] {
            let amount = Amount::from_atomic(units);
            assert_eq!(amount.to_string().parse::<Amount>(), Ok(amount));
        }
    }

    #[test]
    fn checked_arithmetic_does_not_wrap() {
        let max = Amount::from_atomic(u64::MAX);
        let one = Amount::from_atomic(1);
        assert_eq!(max.checked_add(one), None);
        assert_eq!(Amount::ZERO.checked_sub(one), None);
        assert_eq!(one.checked_add(one), Some(Amount::from_atomic(2)));
    }
}
