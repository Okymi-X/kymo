//! Attack detectors over recent requests. Pure: the caller passes the requests and the time, and
//! gets back who crossed which threshold. Staff are never passed in, so they are never flagged.
//!
//! Each detector counts something per subject (a signed in player, or an address when nobody is
//! signed in) inside its time window. Reaching `alert_at` raises an alert; reaching `ban_at`, when
//! the organizer set one, is the point where the caller may ban automatically.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Many different unknown or missing addresses: someone is looking for something to attack.
    Scan,
    /// Repeated refusals on staff only routes.
    AdminProbe,
    /// Requests that carry the usual injection and traversal strings.
    Signature,
    /// A user agent that names a scanning tool.
    ScannerTool,
    /// Many failed sign ins from one address.
    BruteForce,
    /// Far more requests than any page needs.
    Flood,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Scan,
        Kind::AdminProbe,
        Kind::Signature,
        Kind::ScannerTool,
        Kind::BruteForce,
        Kind::Flood,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Scan => "scan",
            Kind::AdminProbe => "admin_probe",
            Kind::Signature => "signature",
            Kind::ScannerTool => "scanner_tool",
            Kind::BruteForce => "brute_force",
            Kind::Flood => "flood",
        }
    }

    pub fn parse(raw: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == raw)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Scan => "Path scanning",
            Kind::AdminProbe => "Admin probing",
            Kind::Signature => "Attack strings",
            Kind::ScannerTool => "Scanner tool",
            Kind::BruteForce => "Failed sign ins",
            Kind::Flood => "Request flood",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Kind::Scan => "Distinct unknown or missing addresses requested in the window.",
            Kind::AdminProbe => {
                "Refusals on admin routes: a signed in player refused, or a visitor trying several different admin addresses."
            }
            Kind::Signature => {
                "Requests containing traversal, injection or script strings, or probing for files like .env and .git."
            }
            Kind::ScannerTool => {
                "Requests whose user agent names a scanner such as sqlmap, nikto, gobuster, ffuf or nmap."
            }
            Kind::BruteForce => "Failed sign ins from one address in the window.",
            Kind::Flood => "Total requests from one player or address in the window.",
        }
    }
}

/// How one detector behaves. The organizer can tighten or loosen every field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    /// Counts at or above this raise an alert.
    pub alert_at: u32,
    /// Counts at or above this ban the player automatically. None never bans.
    pub ban_at: Option<u32>,
    /// Only the last this many seconds are counted.
    pub window_secs: i64,
    /// How long an automatic ban lasts. None is permanent.
    pub ban_secs: Option<i64>,
}

pub const MAX_WINDOW_SECS: i64 = 3600;

pub fn default_config(kind: Kind) -> Config {
    let (alert_at, ban_at, window_secs) = match kind {
        Kind::Scan => (25, Some(120), 120),
        Kind::AdminProbe => (5, Some(25), 300),
        Kind::Signature => (2, Some(10), 300),
        Kind::ScannerTool => (1, Some(20), 300),
        Kind::BruteForce => (8, Some(40), 300),
        Kind::Flood => (600, Some(2000), 60),
    };
    Config {
        enabled: true,
        alert_at,
        ban_at,
        window_secs,
        ban_secs: Some(15 * 60),
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=1_000_000).contains(&self.alert_at) {
            return Err("alert threshold is 1 to 1000000");
        }
        if let Some(ban) = self.ban_at
            && (ban < self.alert_at || ban > 10_000_000)
        {
            return Err("the ban threshold must be at least the alert threshold");
        }
        if !(10..=MAX_WINDOW_SECS).contains(&self.window_secs) {
            return Err("the window is 10 seconds to 1 hour");
        }
        if let Some(secs) = self.ban_secs
            && !(60..=31_536_000).contains(&secs)
        {
            return Err("a ban lasts 1 minute to 1 year, or is permanent");
        }
        Ok(())
    }
}

/// One logged request, as the detectors see it.
#[derive(Debug, Clone)]
pub struct Sample {
    pub at: i64,
    pub method: String,
    pub route: String,
    pub path: String,
    pub query: String,
    pub status: u16,
    pub ip: String,
    pub user_id: Option<i64>,
    pub ua: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub kind: Kind,
    pub user_id: Option<i64>,
    pub ip: String,
    pub count: u32,
    pub first_at: i64,
    pub last_at: i64,
    /// A few example requests, newest first.
    pub evidence: Vec<String>,
}

const UNMATCHED: &str = "unmatched";
const LOGIN_ROUTES: [&str; 2] = ["/api/v1/auth/player-login", "/api/v1/auth/login"];
const ADMIN_PREFIXES: [&str; 2] = ["/api/v1/admin/", "/api/v1/admin"];
const MAX_EVIDENCE: usize = 5;

const SIGNATURES: &[&str] = &[
    "../",
    "..\\",
    "\0",
    "' or ",
    "\" or ",
    "' --",
    "union select",
    "<script",
    "javascript:",
    "onerror=",
    "${jndi",
    "{{7*7}}",
    "/etc/passwd",
    "/proc/self",
    "c:\\windows",
    "/.env",
    "/.git",
    "wp-login",
    "wp-admin",
    "phpmyadmin",
    "xp_cmdshell",
    "sleep(",
    "benchmark(",
    "; drop ",
    "base64_decode(",
    "eval(",
];

const SCANNER_AGENTS: &[&str] = &[
    "sqlmap",
    "nikto",
    "gobuster",
    "ffuf",
    "nmap",
    "wfuzz",
    "dirbuster",
    "dirb/",
    "nuclei",
    "masscan",
    "zgrab",
    "acunetix",
    "hydra",
    "feroxbuster",
    "whatweb",
    "nessus",
    "openvas",
];

/// `%41` to `A`, `+` to a space. Anything malformed is left as it is.
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if bytes.get(i + 2).is_some() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The first attack string found in the address and query, if any. Decoded twice, so a doubly
/// encoded `%252e%252e%252f` is caught as well as a plain one.
pub fn signature_in(path: &str, query: &str) -> Option<&'static str> {
    let once = percent_decode(&format!("{path}?{query}")).to_lowercase();
    let twice = percent_decode(&once);
    SIGNATURES
        .iter()
        .find(|s| once.contains(*s) || twice.contains(*s))
        .copied()
}

pub fn scanner_in(ua: &str) -> Option<&'static str> {
    let ua = ua.to_lowercase();
    SCANNER_AGENTS.iter().find(|a| ua.contains(*a)).copied()
}

fn describe(s: &Sample) -> String {
    let mut line = format!("{} {}", s.method, s.path);
    if !s.query.is_empty() {
        line.push('?');
        line.push_str(&s.query);
    }
    line.push_str(&format!(" -> {}", s.status));
    line.chars().take(180).collect()
}

fn is_login(s: &Sample) -> bool {
    s.method == "POST" && LOGIN_ROUTES.contains(&s.route.as_str())
}

fn is_admin_path(s: &Sample) -> bool {
    ADMIN_PREFIXES.iter().any(|p| s.path.starts_with(p))
}

/// Who a request counts against: the signed in player, otherwise the address.
fn subject_of(s: &Sample) -> (Option<i64>, String) {
    match s.user_id {
        Some(id) => (Some(id), String::new()),
        None => (None, s.ip.clone()),
    }
}

/// Runs one detector over the requests and returns every subject at or above its alert threshold.
/// Requests older than the window are ignored.
pub fn evaluate(kind: Kind, cfg: &Config, samples: &[Sample], now: i64) -> Vec<Hit> {
    if !cfg.enabled {
        return Vec::new();
    }
    let since = now - cfg.window_secs;
    struct Acc<'a> {
        keys: BTreeSet<String>,
        count: u32,
        first: i64,
        last: i64,
        ip: String,
        evidence: Vec<(i64, &'a Sample)>,
    }
    let mut by_subject: BTreeMap<(Option<i64>, String), Acc<'_>> = BTreeMap::new();
    for s in samples.iter().filter(|s| s.at > since && s.at <= now) {
        // What this request contributes: a key (for distinct counting) or just one more.
        let key: Option<String> = match kind {
            Kind::Scan => (s.route == UNMATCHED || s.status == 404).then(|| s.path.clone()),
            Kind::AdminProbe => {
                if is_admin_path(s) && matches!(s.status, 401 | 403) {
                    // A signed in player refused counts every time; a visitor counts each
                    // different address once, so a stale tab repeating one call does not.
                    Some(if s.user_id.is_some() {
                        format!("{}#{}", s.path, s.at)
                    } else {
                        s.path.clone()
                    })
                } else {
                    None
                }
            }
            Kind::Signature => {
                signature_in(&s.path, &s.query).map(|_| format!("{}#{}", s.at, s.path))
            }
            Kind::ScannerTool => scanner_in(&s.ua).map(|_| format!("{}#{}", s.at, s.path)),
            Kind::BruteForce => {
                (is_login(s) && s.status == 401).then(|| format!("{}#{}", s.at, s.ip))
            }
            Kind::Flood => Some(format!("{}#{}#{}", s.at, s.path, s.ip)),
        };
        let Some(key) = key else { continue };
        let (user, ip) = subject_of(s);
        // Brute force is about the address, even if a stale token rides along.
        let (user, ip) = if kind == Kind::BruteForce {
            (None, s.ip.clone())
        } else {
            (user, ip)
        };
        let acc = by_subject.entry((user, ip)).or_insert_with(|| Acc {
            keys: BTreeSet::new(),
            count: 0,
            first: s.at,
            last: s.at,
            ip: s.ip.clone(),
            evidence: Vec::new(),
        });
        // Distinct kinds count once per key; everything else counts every request.
        let distinct =
            matches!(kind, Kind::Scan) || (kind == Kind::AdminProbe && s.user_id.is_none());
        if !distinct || acc.keys.insert(key) {
            acc.count += 1;
        }
        acc.first = acc.first.min(s.at);
        if s.at >= acc.last {
            acc.last = s.at;
            acc.ip = s.ip.clone();
        }
        acc.evidence.push((s.at, s));
    }
    let mut hits: Vec<Hit> = by_subject
        .into_iter()
        .filter(|(_, acc)| acc.count >= cfg.alert_at)
        .map(|((user_id, ip), mut acc)| {
            acc.evidence.sort_by_key(|e| std::cmp::Reverse(e.0));
            let mut seen = BTreeSet::new();
            let evidence = acc
                .evidence
                .iter()
                .map(|(_, s)| describe(s))
                .filter(|line| seen.insert(line.clone()))
                .take(MAX_EVIDENCE)
                .collect();
            Hit {
                kind,
                user_id,
                ip: if user_id.is_some() { acc.ip } else { ip },
                count: acc.count,
                first_at: acc.first,
                last_at: acc.last,
                evidence,
            }
        })
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.count));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 10_000;

    fn req(at: i64, route: &str, path: &str, status: u16, user: Option<i64>) -> Sample {
        Sample {
            at,
            method: "GET".into(),
            route: route.into(),
            path: path.into(),
            query: String::new(),
            status,
            ip: "10.0.0.5".into(),
            user_id: user,
            ua: "Mozilla/5.0".into(),
        }
    }

    fn cfg(alert_at: u32) -> Config {
        Config {
            alert_at,
            window_secs: 120,
            ..default_config(Kind::Scan)
        }
    }

    #[test]
    fn scanning_counts_distinct_missing_addresses_per_player() {
        let mut rows = Vec::new();
        for i in 0..30 {
            rows.push(req(
                NOW - 10,
                UNMATCHED,
                &format!("/api/v1/x{i}"),
                404,
                Some(7),
            ));
        }
        // The same address over and over is one address.
        for _ in 0..50 {
            rows.push(req(NOW - 5, UNMATCHED, "/api/v1/same", 404, Some(8)));
        }
        // Ordinary pages are not scanning.
        for _ in 0..50 {
            rows.push(req(
                NOW - 5,
                "/api/v1/events",
                "/api/v1/events",
                200,
                Some(9),
            ));
        }
        let hits = evaluate(Kind::Scan, &cfg(25), &rows, NOW);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].user_id, hits[0].count), (Some(7), 30));
        assert_eq!(hits[0].evidence.len(), 5);
        // Below the threshold or outside the window: nothing.
        assert!(evaluate(Kind::Scan, &cfg(31), &rows, NOW).is_empty());
        assert!(evaluate(Kind::Scan, &cfg(25), &rows, NOW + 500).is_empty());
    }

    #[test]
    fn a_visitor_is_counted_by_address_and_a_player_by_account() {
        let mut rows = Vec::new();
        for i in 0..5 {
            rows.push(req(NOW - 1, UNMATCHED, &format!("/a{i}"), 401, None));
            rows.push(req(NOW - 1, UNMATCHED, &format!("/b{i}"), 404, Some(3)));
        }
        let hits = evaluate(Kind::Scan, &cfg(5), &rows, NOW);
        assert_eq!(hits.len(), 2);
        assert!(
            hits.iter()
                .any(|h| h.user_id.is_none() && h.ip == "10.0.0.5")
        );
        assert!(hits.iter().any(|h| h.user_id == Some(3)));
    }

    #[test]
    fn admin_probing_ignores_a_stale_tab_but_not_a_refused_player() {
        let config = Config {
            alert_at: 3,
            window_secs: 300,
            ..default_config(Kind::AdminProbe)
        };
        let mut rows = Vec::new();
        // An expired session polling one admin address: 20 times, one distinct address.
        for i in 0..20 {
            rows.push(req(
                NOW - i,
                "/api/v1/admin/events",
                "/api/v1/admin/events",
                401,
                None,
            ));
        }
        // A signed in player refused on admin routes three times.
        for i in 0..3 {
            rows.push(req(
                NOW - i,
                "/api/v1/admin/users",
                "/api/v1/admin/users",
                403,
                Some(4),
            ));
        }
        // A visitor trying three different admin addresses.
        for p in ["users", "labs", "tokens"] {
            let mut r = req(NOW - 2, UNMATCHED, &format!("/api/v1/admin/{p}"), 401, None);
            r.ip = "10.0.0.9".into();
            rows.push(r);
        }
        let hits = evaluate(Kind::AdminProbe, &config, &rows, NOW);
        let who: Vec<_> = hits
            .iter()
            .map(|h| (h.user_id, h.ip.as_str(), h.count))
            .collect();
        assert!(who.contains(&(Some(4), "10.0.0.5", 3)), "{who:?}");
        assert!(who.contains(&(None, "10.0.0.9", 3)), "{who:?}");
        assert!(
            !who.iter().any(|w| w.1 == "10.0.0.5" && w.0.is_none()),
            "{who:?}"
        );
    }

    #[test]
    fn attack_strings_are_found_plain_encoded_and_double_encoded() {
        assert_eq!(signature_in("/api/v1/../../etc/passwd", ""), Some("../"));
        assert_eq!(
            signature_in("/api/v1/events", "q=%27%20OR%201=1"),
            Some("' or "),
            "case does not matter"
        );
        assert_eq!(
            signature_in("/api/v1/events", "q=%27%20or%20x"),
            Some("' or ")
        );
        assert_eq!(signature_in("/a", "x=%252e%252e%252f"), Some("../"));
        assert_eq!(
            signature_in("/a", "x=<script>alert(1)</script>"),
            Some("<script")
        );
        assert_eq!(signature_in("/.env", ""), Some("/.env"));
        assert_eq!(
            signature_in("/api/v1/events/spring-cup", "view=scoreboard"),
            None
        );
        assert_eq!(
            signature_in("/api/v1/events", "q=100%"),
            None,
            "a stray percent is harmless"
        );
        assert_eq!(
            scanner_in("sqlmap/1.7.2#stable (https://sqlmap.org)"),
            Some("sqlmap")
        );
        assert_eq!(
            scanner_in("Mozilla/5.0 (X11; Linux x86_64) Firefox/130"),
            None
        );
        assert_eq!(
            scanner_in("curl/8.5.0"),
            None,
            "ordinary scripting is not a scanner"
        );
    }

    #[test]
    fn signature_and_scanner_detectors_count_every_request() {
        let mut rows = Vec::new();
        for i in 0..3 {
            let mut r = req(
                NOW - i,
                "/api/v1/events/{slug}",
                "/api/v1/events/x",
                404,
                Some(2),
            );
            r.query = "a=../../etc/passwd".into();
            r.ua = "gobuster/3.6".into();
            rows.push(r);
        }
        let sig = Config {
            alert_at: 2,
            window_secs: 300,
            ..default_config(Kind::Signature)
        };
        assert_eq!(evaluate(Kind::Signature, &sig, &rows, NOW)[0].count, 3);
        let tool = Config {
            alert_at: 1,
            ..default_config(Kind::ScannerTool)
        };
        assert_eq!(evaluate(Kind::ScannerTool, &tool, &rows, NOW)[0].count, 3);
    }

    #[test]
    fn brute_force_is_about_the_address_and_counts_only_failed_sign_ins() {
        let mut rows = Vec::new();
        for i in 0..9 {
            let mut r = req(
                NOW - i,
                "/api/v1/auth/player-login",
                "/api/v1/auth/player-login",
                401,
                Some(1),
            );
            r.method = "POST".into();
            rows.push(r);
        }
        let mut ok = req(
            NOW,
            "/api/v1/auth/player-login",
            "/api/v1/auth/player-login",
            200,
            None,
        );
        ok.method = "POST".into();
        rows.push(ok);
        let config = Config {
            alert_at: 8,
            window_secs: 300,
            ..default_config(Kind::BruteForce)
        };
        let hits = evaluate(Kind::BruteForce, &config, &rows, NOW);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].user_id, hits[0].count), (None, 9));
    }

    #[test]
    fn a_flood_is_total_requests_and_a_disabled_detector_is_silent() {
        let rows: Vec<Sample> = (0..700)
            .map(|i| {
                req(
                    NOW - i % 50,
                    "/api/v1/events",
                    "/api/v1/events",
                    200,
                    Some(6),
                )
            })
            .collect();
        let hits = evaluate(Kind::Flood, &default_config(Kind::Flood), &rows, NOW);
        assert_eq!((hits[0].user_id, hits[0].count), (Some(6), 700));
        let off = Config {
            enabled: false,
            ..default_config(Kind::Flood)
        };
        assert!(evaluate(Kind::Flood, &off, &rows, NOW).is_empty());
    }

    #[test]
    fn settings_are_checked() {
        assert!(default_config(Kind::Scan).validate().is_ok());
        for kind in Kind::ALL {
            assert!(default_config(kind).validate().is_ok(), "{kind:?}");
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        let mut c = default_config(Kind::Scan);
        c.alert_at = 0;
        assert!(c.validate().is_err());
        let mut c = default_config(Kind::Scan);
        c.ban_at = Some(c.alert_at - 1);
        assert!(c.validate().is_err());
        let mut c = default_config(Kind::Scan);
        c.window_secs = 5;
        assert!(c.validate().is_err());
        let mut c = default_config(Kind::Scan);
        c.ban_secs = Some(10);
        assert!(c.validate().is_err());
        c.ban_secs = None;
        assert!(c.validate().is_ok(), "permanent is allowed");
        c.ban_at = None;
        assert!(c.validate().is_ok(), "never banning is allowed");
    }
}
