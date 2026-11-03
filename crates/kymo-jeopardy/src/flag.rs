//! Flag checking. Static and per team flags are compared in constant time.

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::FlagError;

/// Longest accepted regex pattern, in characters.
pub const MAX_PATTERN_LEN: usize = 256;
/// Compiled regex size cap in bytes, bounding memory for hostile patterns.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// How a challenge flag is verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlagSpec {
    /// One flag for everybody.
    Static { value: String },
    /// Submission must match the whole pattern (implicitly anchored).
    Regex { pattern: String },
    /// Flag already resolved for the submitting participant or team.
    PerTeam { expected: String },
}

impl FlagSpec {
    /// Checks the spec is usable. Call at authoring time.
    pub fn validate(&self) -> Result<(), FlagError> {
        match self {
            FlagSpec::Regex { pattern } => compile(pattern, true).map(|_| ()),
            _ => Ok(()),
        }
    }
}

/// Accepted submission shape, for example `KYMO{` and `}`. Empty parts do not
/// constrain anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagFormat {
    pub prefix: String,
    pub suffix: String,
}

impl FlagFormat {
    pub fn new(prefix: impl Into<String>, suffix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            suffix: suffix.into(),
        }
    }

    /// `submitted` is expected to be trimmed already.
    fn accepts(&self, submitted: &str, case_sensitive: bool) -> bool {
        let (s, p, x) = if case_sensitive {
            (
                submitted.to_owned(),
                self.prefix.clone(),
                self.suffix.clone(),
            )
        } else {
            (
                submitted.to_lowercase(),
                self.prefix.to_lowercase(),
                self.suffix.to_lowercase(),
            )
        };
        s.len() >= p.len() + x.len() && s.starts_with(&p) && s.ends_with(&x)
    }
}

/// Outcome of a flag check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Correct,
    Incorrect,
    /// Not in the accepted format. Not an attempt.
    BadFormat,
}

impl Verdict {
    /// Whether this submission consumes rate limit and attempt cap budget.
    pub fn counts_as_attempt(self) -> bool {
        !matches!(self, Verdict::BadFormat)
    }
}

/// Constant time string equality. Both inputs are hashed first so neither
/// content nor length influences timing.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let da = Sha256::digest(a.as_bytes());
    let db = Sha256::digest(b.as_bytes());
    let mut diff = 0u8;
    for (x, y) in da.iter().zip(db.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn compile(pattern: &str, case_sensitive: bool) -> Result<regex::Regex, FlagError> {
    if pattern.chars().count() > MAX_PATTERN_LEN {
        return Err(FlagError::PatternTooLong {
            max: MAX_PATTERN_LEN,
        });
    }
    let anchored = format!("\\A(?:{pattern})\\z");
    RegexBuilder::new(&anchored)
        .case_insensitive(!case_sensitive)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| FlagError::InvalidPattern(e.to_string()))
}

/// Checks `submitted` against `spec`. Surrounding whitespace is trimmed
/// first. The format is checked before the flag. A regex that cannot be
/// compiled never matches (fail closed); use [`FlagSpec::validate`] when
/// authoring.
pub fn check(
    spec: &FlagSpec,
    format: &FlagFormat,
    case_sensitive: bool,
    submitted: &str,
) -> Verdict {
    let submitted = submitted.trim();
    if !format.accepts(submitted, case_sensitive) {
        return Verdict::BadFormat;
    }
    let ok = match spec {
        FlagSpec::Static { value } => eq_flag(value.trim(), submitted, case_sensitive),
        FlagSpec::PerTeam { expected } => eq_flag(expected.trim(), submitted, case_sensitive),
        FlagSpec::Regex { pattern } => compile(pattern, case_sensitive)
            .map(|re| re.is_match(submitted))
            .unwrap_or(false),
    };
    if ok {
        Verdict::Correct
    } else {
        Verdict::Incorrect
    }
}

fn eq_flag(expected: &str, submitted: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        ct_eq(expected, submitted)
    } else {
        ct_eq(&expected.to_lowercase(), &submitted.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> FlagFormat {
        FlagFormat::new("KYMO{", "}")
    }
    fn st(v: &str) -> FlagSpec {
        FlagSpec::Static { value: v.into() }
    }

    #[test]
    fn static_correct_and_incorrect() {
        assert_eq!(
            check(&st("KYMO{a}"), &fmt(), true, "KYMO{a}"),
            Verdict::Correct
        );
        assert_eq!(
            check(&st("KYMO{a}"), &fmt(), true, "KYMO{b}"),
            Verdict::Incorrect
        );
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            check(&st("KYMO{a}"), &fmt(), true, "  KYMO{a}\n"),
            Verdict::Correct
        );
    }

    #[test]
    fn case_sensitivity() {
        assert_eq!(
            check(&st("KYMO{Ab}"), &fmt(), true, "kymo{ab}"),
            Verdict::BadFormat
        );
        assert_eq!(
            check(&st("KYMO{Ab}"), &fmt(), true, "KYMO{ab}"),
            Verdict::Incorrect
        );
        assert_eq!(
            check(&st("KYMO{Ab}"), &fmt(), false, "kymo{ab}"),
            Verdict::Correct
        );
    }

    #[test]
    fn bad_format_is_not_an_attempt() {
        let v = check(&st("KYMO{a}"), &fmt(), true, "hello");
        assert_eq!(v, Verdict::BadFormat);
        assert!(!v.counts_as_attempt());
        assert!(Verdict::Incorrect.counts_as_attempt());
        assert!(Verdict::Correct.counts_as_attempt());
    }

    #[test]
    fn format_needs_room_for_both_parts() {
        let f = FlagFormat::new("ab", "bc");
        assert_eq!(check(&st("abc"), &f, true, "abc"), Verdict::BadFormat);
    }

    #[test]
    fn empty_format_accepts_anything() {
        let f = FlagFormat::default();
        assert_eq!(check(&st("x"), &f, true, "x"), Verdict::Correct);
        assert_eq!(check(&st("x"), &f, true, "y"), Verdict::Incorrect);
    }

    #[test]
    fn regex_is_fully_anchored() {
        let r = FlagSpec::Regex {
            pattern: r"KYMO\{[a-f0-9]{4}\}".into(),
        };
        assert_eq!(check(&r, &fmt(), true, "KYMO{ab12}"), Verdict::Correct);
        assert_eq!(check(&r, &fmt(), true, "KYMO{ab123}"), Verdict::Incorrect);
        assert_eq!(check(&r, &fmt(), true, "KYMO{ab1g}"), Verdict::Incorrect);
    }

    #[test]
    fn regex_alternation_stays_anchored() {
        let r = FlagSpec::Regex {
            pattern: "KYMO\\{a\\}|KYMO\\{b\\}".into(),
        };
        assert_eq!(check(&r, &fmt(), true, "KYMO{a}"), Verdict::Correct);
        assert_eq!(check(&r, &fmt(), true, "KYMO{a}}"), Verdict::Incorrect);
    }

    #[test]
    fn regex_case_insensitive() {
        let r = FlagSpec::Regex {
            pattern: r"KYMO\{abc\}".into(),
        };
        assert_eq!(check(&r, &fmt(), false, "kymo{ABC}"), Verdict::Correct);
    }

    #[test]
    fn long_pattern_rejected() {
        let r = FlagSpec::Regex {
            pattern: "a".repeat(257),
        };
        assert_eq!(r.validate(), Err(FlagError::PatternTooLong { max: 256 }));
        assert_eq!(
            check(&r, &FlagFormat::default(), true, "a"),
            Verdict::Incorrect
        );
        let ok = FlagSpec::Regex {
            pattern: "a".repeat(256),
        };
        assert_eq!(ok.validate(), Ok(()));
    }

    #[test]
    fn invalid_pattern_fails_closed() {
        let r = FlagSpec::Regex {
            pattern: "(".into(),
        };
        assert!(matches!(r.validate(), Err(FlagError::InvalidPattern(_))));
        assert_eq!(
            check(&r, &FlagFormat::default(), true, "("),
            Verdict::Incorrect
        );
    }

    #[test]
    fn per_team() {
        let p = FlagSpec::PerTeam {
            expected: "KYMO{team7}".into(),
        };
        assert_eq!(check(&p, &fmt(), true, "KYMO{team7}"), Verdict::Correct);
        assert_eq!(check(&p, &fmt(), true, "KYMO{team8}"), Verdict::Incorrect);
    }

    #[test]
    fn ct_eq_behaves() {
        assert!(ct_eq("", ""));
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
        assert!(!ct_eq("abc", ""));
        assert!(!ct_eq("", "a"));
    }
}
