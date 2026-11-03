use std::net::IpAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanKind {
    Ip,
    User,
}

impl BanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BanKind::Ip => "ip",
            BanKind::User => "user",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ip" => Some(BanKind::Ip),
            "user" => Some(BanKind::User),
            _ => None,
        }
    }
}

/// A ban with no expiry is permanent.
pub fn is_active(expires_at: Option<i64>, now: i64) -> bool {
    expires_at.is_none_or(|t| now < t)
}

/// Expiry time for a ban of `duration_secs`, or none when permanent.
pub fn expiry(now: i64, duration_secs: Option<i64>) -> Option<i64> {
    duration_secs.map(|d| now.saturating_add(d))
}

/// The canonical text of a ban subject: an IP address in its shortest form, or a user id.
/// Returns none when the text is not valid for the kind.
pub fn parse_subject(kind: BanKind, text: &str) -> Option<String> {
    let text = text.trim();
    match kind {
        BanKind::Ip => text.parse::<IpAddr>().ok().map(|ip| ip.to_string()),
        BanKind::User => text
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bans_expire_exactly_at_their_deadline() {
        assert!(is_active(None, i64::MAX));
        assert!(is_active(Some(100), 99));
        assert!(!is_active(Some(100), 100));
        assert_eq!(expiry(10, Some(5)), Some(15));
        assert_eq!(expiry(10, None), None);
        assert_eq!(expiry(i64::MAX, Some(5)), Some(i64::MAX));
    }

    #[test]
    fn subjects_are_validated_and_canonical() {
        assert_eq!(
            parse_subject(BanKind::Ip, " 10.0.0.1 ").as_deref(),
            Some("10.0.0.1")
        );
        assert_eq!(
            parse_subject(BanKind::Ip, "2001:0db8:0:0:0:0:0:1").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(parse_subject(BanKind::Ip, "10.0.0"), None);
        assert_eq!(parse_subject(BanKind::Ip, "evil; drop"), None);
        assert_eq!(parse_subject(BanKind::User, "042").as_deref(), Some("42"));
        assert_eq!(parse_subject(BanKind::User, "0"), None);
        assert_eq!(parse_subject(BanKind::User, "-3"), None);
        assert_eq!(parse_subject(BanKind::User, "x"), None);
    }

    #[test]
    fn kind_names_round_trip() {
        for k in [BanKind::Ip, BanKind::User] {
            assert_eq!(BanKind::parse(k.as_str()), Some(k));
        }
    }
}
