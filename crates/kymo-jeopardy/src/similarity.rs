//! How alike two programs are, for spotting a solution that was copied. Pure.
//!
//! A program is reduced to a stream of tokens in which comments and spacing are gone and every
//! name, number and string is one placeholder, so renaming variables, rewording comments or
//! reformatting does not hide a copy. What is left is the shape of the code: its keywords,
//! operators and punctuation in order. Overlapping runs of five tokens are hashed, and the
//! similarity is the share of those runs two programs have in common.

use std::collections::HashSet;

/// Tokens in a run that is compared.
const SHINGLE: usize = 5;
/// Fewest tokens a program needs to be compared at all: a few lines of input and output look alike
/// in every honest solution.
pub const MIN_TOKENS: usize = 40;

/// Words kept as themselves, because they are what the shape of a program is made of. They are
/// the common ones of C, C++, Java, Python, JavaScript, Go and Rust; any other word is a name.
const KEYWORDS: &[&str] = &[
    "if",
    "else",
    "elif",
    "for",
    "while",
    "do",
    "return",
    "break",
    "continue",
    "switch",
    "case",
    "default",
    "def",
    "fn",
    "func",
    "function",
    "class",
    "struct",
    "int",
    "long",
    "char",
    "float",
    "double",
    "void",
    "bool",
    "let",
    "var",
    "const",
    "mut",
    "in",
    "not",
    "and",
    "or",
    "import",
    "include",
    "using",
    "namespace",
    "public",
    "private",
    "static",
    "new",
    "try",
    "catch",
    "except",
    "with",
    "match",
    "lambda",
    "yield",
    "async",
    "await",
    "true",
    "false",
    "null",
    "None",
    "True",
    "False",
    "print",
    "input",
    "range",
    "len",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    hashes: HashSet<u64>,
    /// Tokens the program was reduced to.
    pub tokens: usize,
}

fn tokens(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' || (c == '/' && next == Some('/')) {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '"' || c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                i += usize::from(chars[i] == '\\') + 1;
            }
            i += 1;
            out.push("S".to_owned());
        } else if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            out.push("N".to_owned());
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            out.push(if KEYWORDS.contains(&word.as_str()) {
                word
            } else {
                "I".to_owned()
            });
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

fn hash(run: &[String]) -> u64 {
    // FNV-1a: small, stable between runs and machines, and not a secret.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for token in run {
        for byte in token.bytes().chain(std::iter::once(0x1f)) {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

pub fn fingerprint(source: &str) -> Fingerprint {
    let t = tokens(source);
    let hashes = if t.len() < SHINGLE {
        HashSet::new()
    } else {
        t.windows(SHINGLE).map(hash).collect()
    };
    Fingerprint {
        hashes,
        tokens: t.len(),
    }
}

/// Whether the program is long enough for a comparison to mean anything.
pub fn comparable(f: &Fingerprint) -> bool {
    f.tokens >= MIN_TOKENS
}

/// The share of runs the two programs have in common, from 0 to 1. Zero when either is too short.
pub fn similarity(a: &Fingerprint, b: &Fingerprint) -> f64 {
    if !comparable(a) || !comparable(b) {
        return 0.0;
    }
    let both = a.hashes.intersection(&b.hashes).count();
    let either = a.hashes.len() + b.hashes.len() - both;
    if either == 0 {
        return 0.0;
    }
    both as f64 / either as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLUTION: &str = "
def solve(numbers):
    total = 0
    for value in numbers:
        if value % 2 == 0:
            total += value * 3
        else:
            total -= value
    return total

n = int(input())
data = [int(x) for x in input().split()]
print(solve(data[:n]))
";

    #[test]
    fn a_renamed_reformatted_and_recommented_copy_is_still_a_copy() {
        let copy = "
# my own solution
def compute(items):   # sum things
    acc = 0
    for item in items:
        if item % 2 == 0:
            acc += item * 3
        else:
            acc -= item
    return acc
count = int(input())
values = [int(v) for v in input().split()]
print(compute(values[:count]))
";
        let (a, b) = (fingerprint(SOLUTION), fingerprint(copy));
        assert!(comparable(&a));
        assert!(similarity(&a, &b) > 0.9, "{}", similarity(&a, &b));
        assert!((similarity(&a, &a) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_different_way_of_solving_it_is_not_flagged() {
        let other = "
import sys
data = sys.stdin.read().split()
n = int(data[0])
evens = [int(v) for v in data[1:n + 1] if int(v) % 2 == 0]
odds = [int(v) for v in data[1:n + 1] if int(v) % 2 != 0]
print(sum(evens) * 3 - sum(odds))
sys.stdout.write('')
";
        assert!(similarity(&fingerprint(SOLUTION), &fingerprint(other)) < 0.5);
    }

    #[test]
    fn short_programs_are_never_compared() {
        let tiny = "a, b = map(int, input().split())\nprint(a + b)";
        assert!(!comparable(&fingerprint(tiny)));
        assert_eq!(similarity(&fingerprint(tiny), &fingerprint(tiny)), 0.0);
    }

    #[test]
    fn comments_strings_and_numbers_do_not_matter_but_operators_do() {
        let a = fingerprint(
            "x = foo(1, \"a // not a comment\") + 2 /* c */ // tail\n"
                .repeat(12)
                .as_str(),
        );
        let b = fingerprint("y = bar(99, 'other') + 7\n".repeat(12).as_str());
        assert!(similarity(&a, &b) > 0.95);
        let c = fingerprint("y = bar(99, 'other') - 7\n".repeat(12).as_str());
        assert!(similarity(&b, &c) < 0.5);
    }

    #[test]
    fn hostile_input_does_not_panic() {
        for s in [
            "",
            "\"",
            "'\\",
            "/*",
            "/",
            "#",
            "\u{0}\u{ffff}",
            &"(".repeat(10_000),
        ] {
            let _ = similarity(&fingerprint(s), &fingerprint(s));
        }
    }
}
