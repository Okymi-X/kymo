use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_ENDPOINT_LEN: usize = 200;
const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Who a rule counts requests for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Ip,
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Ip => "ip",
            Scope::User => "user",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ip" => Some(Scope::Ip),
            "user" => Some(Scope::User),
            _ => None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuleError {
    #[error(
        "the endpoint is `*`, `/path`, `/prefix/*`, or a method then a path such as `POST /api/v1/x`"
    )]
    Endpoint,
    #[error("max_requests is 1 to 100000")]
    MaxRequests,
    #[error("window_secs is 1 to 86400")]
    Window,
    #[error("strikes_to_ban is 1 to 1000")]
    Strikes,
    #[error("ban_secs is 0 (permanent) or up to 31536000")]
    BanSecs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    /// `*`, `/path`, `/prefix/*`, optionally led by an HTTP method. Paths are the route
    /// patterns the server matched (`/api/v1/events/{slug}/register`), not raw URLs.
    pub endpoint: String,
    pub max_requests: u32,
    pub window_secs: i64,
    pub scope: Scope,
    /// Denied requests within ten minutes that turn into a ban; none means never ban.
    pub strikes_to_ban: Option<u32>,
    /// Length of an automatic ban in seconds; 0 is permanent.
    pub ban_secs: i64,
}

impl Rule {
    pub fn validate(&self) -> Result<(), RuleError> {
        let (method, path) = split(&self.endpoint);
        let path_ok = path == "*"
            || (path.starts_with('/')
                && !path.contains(char::is_whitespace)
                && path.chars().all(|c| !c.is_control()));
        let method_ok = method.is_none_or(|m| METHODS.contains(&m));
        if self.endpoint.len() > MAX_ENDPOINT_LEN
            || self.endpoint.chars().any(char::is_control)
            || !path_ok
            || !method_ok
        {
            return Err(RuleError::Endpoint);
        }
        if !(1..=100_000).contains(&self.max_requests) {
            return Err(RuleError::MaxRequests);
        }
        if !(1..=86_400).contains(&self.window_secs) {
            return Err(RuleError::Window);
        }
        if self
            .strikes_to_ban
            .is_some_and(|s| !(1..=1000).contains(&s))
        {
            return Err(RuleError::Strikes);
        }
        if !(0..=31_536_000).contains(&self.ban_secs) {
            return Err(RuleError::BanSecs);
        }
        Ok(())
    }

    /// Whether this rule covers a request. `route` is the matched route pattern.
    pub fn matches(&self, method: &str, route: &str) -> bool {
        let (rule_method, path) = split(&self.endpoint);
        if rule_method.is_some_and(|m| !m.eq_ignore_ascii_case(method)) {
            return false;
        }
        if path == "*" {
            return true;
        }
        match path.strip_suffix("/*") {
            Some(prefix) => route == prefix || route.starts_with(&format!("{prefix}/")),
            None => route == path,
        }
    }
}

fn split(endpoint: &str) -> (Option<&str>, &str) {
    match endpoint.split_once(' ') {
        Some((method, path)) => (Some(method), path.trim()),
        None => (None, endpoint.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(endpoint: &str) -> Rule {
        Rule {
            id: 1,
            endpoint: endpoint.into(),
            max_requests: 10,
            window_secs: 60,
            scope: Scope::Ip,
            strikes_to_ban: Some(3),
            ban_secs: 600,
        }
    }

    #[test]
    fn endpoint_forms_match_as_documented() {
        assert!(rule("*").matches("GET", "/anything"));
        assert!(rule("/a/b").matches("POST", "/a/b"));
        assert!(!rule("/a/b").matches("POST", "/a/b/c"));
        assert!(rule("POST /a/b").matches("post", "/a/b"));
        assert!(!rule("POST /a/b").matches("GET", "/a/b"));
        assert!(rule("/a/*").matches("GET", "/a/b/c"));
        assert!(rule("/a/*").matches("GET", "/a"));
        assert!(!rule("/a/*").matches("GET", "/ab"));
        assert!(rule("GET *").matches("GET", "/x"));
        assert!(!rule("GET *").matches("PUT", "/x"));
    }

    #[test]
    fn validation_covers_every_field() {
        assert_eq!(rule("POST /a").validate(), Ok(()));
        for bad in ["", "a/b", "FETCH /a", "/a b", "POST", "/a\n"] {
            assert_eq!(rule(bad).validate(), Err(RuleError::Endpoint), "{bad:?}");
        }
        let mut r = rule("/a");
        r.max_requests = 0;
        assert_eq!(r.validate(), Err(RuleError::MaxRequests));
        r = rule("/a");
        r.window_secs = 86_401;
        assert_eq!(r.validate(), Err(RuleError::Window));
        r = rule("/a");
        r.strikes_to_ban = Some(0);
        assert_eq!(r.validate(), Err(RuleError::Strikes));
        r = rule("/a");
        r.ban_secs = -1;
        assert_eq!(r.validate(), Err(RuleError::BanSecs));
        r.ban_secs = 0;
        assert_eq!(r.validate(), Ok(()), "0 is a permanent ban");
    }

    #[test]
    fn scope_names_round_trip() {
        for s in [Scope::Ip, Scope::User] {
            assert_eq!(Scope::parse(s.as_str()), Some(s));
        }
        assert_eq!(Scope::parse("team"), None);
    }
}
