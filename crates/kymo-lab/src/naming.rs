//! Instance names. The platform builds one for every instance before it asks a lab host to
//! start it, and the lab host checks it again: neither side trusts the other's input.

/// Longest name accepted; Docker allows more, but names also end up in logs and API paths.
pub const MAX_NAME: usize = 63;

/// Lowercase letters, digits and dashes, 3 to 63 characters, starting and ending with a letter
/// or digit.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (3..=MAX_NAME).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

/// Keeps what is safe in a name from free text (a username, say): lowercase letters and
/// digits, other runs collapse to one dash, trimmed, cut to `max` characters. Never empty.
pub fn sanitize(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        } else {
            pending_dash = true;
        }
        if out.len() >= max {
            break;
        }
    }
    if out.is_empty() { "x".into() } else { out }
}

/// A random lowercase hex suffix, so two instances of one owner and challenge never collide.
pub fn random_suffix() -> String {
    format!("{:06x}", rand::random::<u32>() & 0x00ff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe_or_refused() {
        for ok in ["kymo-c3-u7-a1b2c3", "abc", "a-b"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "ab",
            "-abc",
            "abc-",
            "Kymo-1",
            "a b c",
            "a_b_c",
            "a/../b",
            "é-abc",
            &"a".repeat(64),
            "kymo-1;rm",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn sanitize_keeps_only_safe_characters() {
        assert_eq!(sanitize("Alice O'Neil!", 20), "alice-o-neil");
        assert_eq!(sanitize("../../etc", 20), "etc");
        assert_eq!(sanitize("$$$", 20), "x");
        assert_eq!(sanitize("abcdefghij", 4), "abcd");
        assert!(valid_name(&format!("kymo-{}", sanitize("Ünï cödé", 10))));
    }

    #[test]
    fn suffixes_are_hex_and_vary() {
        let a = random_suffix();
        assert_eq!(a.len(), 6);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        let many: std::collections::HashSet<String> = (0..200).map(|_| random_suffix()).collect();
        assert!(many.len() > 150, "200 suffixes should rarely repeat");
    }
}
