//! Dynamic flags and multi-flag points. A dynamic flag is the admin's prefix, a random hex part
//! and the admin's closing text. Randomness is passed in as bytes, so this module stays pure.

use crate::error::FlagError;
use crate::flag::FlagFormat;

/// Shortest and longest random part, in hex characters.
pub const MIN_RANDOM_LEN: usize = 4;
pub const MAX_RANDOM_LEN: usize = 32;
/// Length used when the admin does not choose one (32 bits).
pub const DEFAULT_RANDOM_LEN: usize = 8;
/// Most flags one challenge may have.
pub const MAX_FLAGS: usize = 8;
/// Largest weight of one flag.
pub const MAX_WEIGHT: i64 = 1000;
/// Longest flag Kymo accepts, in characters (the same cap as a submission).
pub const MAX_FLAG_LEN: usize = 500;

/// How many random bytes `generate` needs for a random part of `len` hex characters.
pub fn bytes_needed(len: usize) -> usize {
    len.div_ceil(2)
}

/// `len` lowercase hex characters made from `bytes`. Extra bytes are ignored and missing ones
/// read as zero, so a short slice never panics.
pub fn random_hex(bytes: &[u8], len: usize) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    (0..len)
        .map(|i| {
            let byte = bytes.get(i / 2).copied().unwrap_or(0);
            let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            char::from(DIGITS[usize::from(nibble)])
        })
        .collect()
}

/// The flag for one run: prefix, random part, closing text.
pub fn generate(prefix: &str, closing: &str, random_len: usize, bytes: &[u8]) -> String {
    format!("{prefix}{}{closing}", random_hex(bytes, random_len))
}

/// Checks that a prefix, closing text and random length produce a flag players can submit:
/// within the length cap and inside the challenge's accepted prefix and suffix.
pub fn validate_shape(
    prefix: &str,
    closing: &str,
    random_len: usize,
    format: &FlagFormat,
    case_sensitive: bool,
) -> Result<(), FlagError> {
    if prefix.trim().is_empty() {
        return Err(FlagError::EmptyPrefix);
    }
    if !(MIN_RANDOM_LEN..=MAX_RANDOM_LEN).contains(&random_len) {
        return Err(FlagError::RandomLength {
            min: MIN_RANDOM_LEN,
            max: MAX_RANDOM_LEN,
        });
    }
    if prefix.chars().count() + random_len + closing.chars().count() > MAX_FLAG_LEN {
        return Err(FlagError::TooLong { max: MAX_FLAG_LEN });
    }
    let fold = |s: &str| {
        if case_sensitive {
            s.to_owned()
        } else {
            s.to_lowercase()
        }
    };
    if !fold(prefix).starts_with(&fold(&format.prefix)) {
        return Err(FlagError::PrefixOutsideFormat);
    }
    if !fold(closing).ends_with(&fold(&format.suffix)) {
        return Err(FlagError::ClosingOutsideFormat);
    }
    Ok(())
}

/// Splits `total` points over flags by weight. The parts always add up to `total`: each flag
/// gets its rounded down share, and the remainder goes to the flag with the largest weight
/// (the later one on a tie). A weight below 1 counts as 1.
pub fn split_points(total: i64, weights: &[i64]) -> Vec<i64> {
    if weights.is_empty() {
        return Vec::new();
    }
    let total = total.max(0);
    let weights: Vec<i64> = weights.iter().map(|w| (*w).max(1)).collect();
    let sum: i64 = weights.iter().sum();
    let mut parts: Vec<i64> = weights.iter().map(|w| total * w / sum).collect();
    let given: i64 = parts.iter().sum();
    let leftover = total - given;
    let heaviest = weights
        .iter()
        .enumerate()
        .max_by_key(|(i, w)| (**w, *i))
        .map_or(0, |(i, _)| i);
    parts[heaviest] += leftover;
    parts
}

/// Takes the unpaid hint cost out of one flag's points. Returns the points awarded and the cost
/// that is still unpaid. An award is never below zero.
pub fn charge_hint_cost(base: i64, owed: i64) -> (i64, i64) {
    let owed = owed.max(0);
    let taken = base.max(0).min(owed);
    (base.max(0) - taken, owed - taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> FlagFormat {
        FlagFormat::new("NDH{", "}")
    }

    #[test]
    fn hex_uses_the_bytes_in_order_and_the_requested_length() {
        assert_eq!(random_hex(&[0x9f, 0x3a, 0x1c, 0x7d], 8), "9f3a1c7d");
        assert_eq!(random_hex(&[0x9f, 0x3a], 3), "9f3");
        assert_eq!(random_hex(&[], 4), "0000");
        assert_eq!(bytes_needed(8), 4);
        assert_eq!(bytes_needed(5), 3);
    }

    #[test]
    fn a_flag_is_prefix_random_closing() {
        let flag = generate("NDH{b2r_user_", "}", 8, &[0x9f, 0x3a, 0x1c, 0x7d]);
        assert_eq!(flag, "NDH{b2r_user_9f3a1c7d}");
    }

    #[test]
    fn a_good_shape_passes_and_the_checks_name_the_problem() {
        assert_eq!(validate_shape("NDH{a_", "}", 8, &fmt(), true), Ok(()));
        assert_eq!(
            validate_shape("  ", "}", 8, &fmt(), true),
            Err(FlagError::EmptyPrefix)
        );
        assert_eq!(
            validate_shape("NDH{a_", "}", 3, &fmt(), true),
            Err(FlagError::RandomLength { min: 4, max: 32 })
        );
        assert_eq!(
            validate_shape("NDH{a_", "}", 33, &fmt(), true),
            Err(FlagError::RandomLength { min: 4, max: 32 })
        );
        assert_eq!(
            validate_shape("KYMO{a_", "}", 8, &fmt(), true),
            Err(FlagError::PrefixOutsideFormat)
        );
        assert_eq!(
            validate_shape("NDH{a_", ")", 8, &fmt(), true),
            Err(FlagError::ClosingOutsideFormat)
        );
        let long = "a".repeat(MAX_FLAG_LEN);
        assert_eq!(
            validate_shape(&long, "", 8, &FlagFormat::default(), true),
            Err(FlagError::TooLong { max: 500 })
        );
    }

    #[test]
    fn case_rule_follows_the_challenge() {
        assert_eq!(
            validate_shape("ndh{a_", "}", 8, &fmt(), true),
            Err(FlagError::PrefixOutsideFormat)
        );
        assert_eq!(validate_shape("ndh{a_", "}", 8, &fmt(), false), Ok(()));
    }

    #[test]
    fn an_empty_format_accepts_any_prefix_and_closing() {
        assert_eq!(
            validate_shape("x", "", 4, &FlagFormat::default(), true),
            Ok(())
        );
    }

    #[test]
    fn the_split_always_adds_up_to_the_total() {
        assert_eq!(split_points(100, &[1, 1]), vec![50, 50]);
        assert_eq!(split_points(100, &[1, 1, 1]), vec![33, 33, 34]);
        assert_eq!(split_points(100, &[1, 3]), vec![25, 75]);
        assert_eq!(split_points(10, &[3, 3]), vec![5, 5]);
        for total in [0, 1, 7, 100, 999, 1_000_000] {
            for weights in [vec![1], vec![1, 2], vec![5, 3, 2], vec![1; 8]] {
                let parts = split_points(total, &weights);
                assert_eq!(parts.iter().sum::<i64>(), total, "{total} {weights:?}");
                assert!(parts.iter().all(|p| *p >= 0));
            }
        }
    }

    #[test]
    fn the_remainder_goes_to_the_heaviest_then_the_later_flag() {
        assert_eq!(split_points(10, &[1, 2]), vec![3, 7]);
        assert_eq!(split_points(10, &[1, 1, 1]), vec![3, 3, 4]);
        assert_eq!(split_points(10, &[2, 1, 2]), vec![4, 2, 4]);
    }

    #[test]
    fn odd_weights_and_totals_do_not_break_the_split() {
        assert_eq!(split_points(100, &[0, 0]), vec![50, 50]);
        assert_eq!(split_points(-5, &[1, 1]), vec![0, 0]);
        assert!(split_points(10, &[]).is_empty());
    }

    #[test]
    fn hint_cost_is_taken_in_order_and_never_below_zero() {
        assert_eq!(charge_hint_cost(50, 0), (50, 0));
        assert_eq!(charge_hint_cost(50, 20), (30, 0));
        assert_eq!(charge_hint_cost(50, 50), (0, 0));
        assert_eq!(charge_hint_cost(50, 80), (0, 30));
        assert_eq!(charge_hint_cost(30, 30), (0, 0));
        assert_eq!(charge_hint_cost(50, -5), (50, 0));
    }
}
