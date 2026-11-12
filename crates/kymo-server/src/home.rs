//! The home page. By default it is built from texts the organizer can edit; the organizer may
//! instead supply their own HTML and CSS. Custom HTML is cleaned when it is saved (no scripts,
//! forms, frames or event handlers) and again by the browser, and the page's security policy
//! forbids scripts and outside resources on top of that.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::text::is_hidden_char;

const KEY: &str = "home";
const MAX_LINE: usize = 200;
const MAX_PARAGRAPH: usize = 2000;
const MAX_HTML: usize = 100_000;
const MAX_CSS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Default,
    Custom,
}

/// Everything the home page shows. Empty optional texts hide their section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Home {
    pub mode: Mode,
    pub headline: String,
    pub subheadline: String,
    pub signin_label: String,
    pub signup_label: String,
    /// Greeting for a signed in player; their name follows it.
    pub welcome: String,
    pub continue_label: String,
    pub banner: String,
    pub about_title: String,
    pub about_text: String,
    pub show_events: bool,
    pub show_scoreboards: bool,
    pub footer: String,
    pub custom_html: String,
    pub custom_css: String,
}

impl Default for Home {
    fn default() -> Self {
        Self {
            mode: Mode::Default,
            headline: "Compete, learn and climb the scoreboard".into(),
            subheadline:
                "Sign in to join events, start your challenge machines and follow your progress."
                    .into(),
            signin_label: "Sign in".into(),
            signup_label: "Create account".into(),
            welcome: "Welcome back".into(),
            continue_label: "Go to events".into(),
            banner: String::new(),
            about_title: "About".into(),
            about_text: String::new(),
            show_events: true,
            show_scoreboards: true,
            footer: String::new(),
            custom_html: String::new(),
            custom_css: String::new(),
        }
    }
}

fn line(label: &str, value: &mut String, max: usize, required: bool) -> Result<(), ApiError> {
    *value = value.trim().to_owned();
    if value.chars().count() > max || value.chars().any(is_hidden_char) {
        return Err(ApiError::BadRequest(format!(
            "{label} must be at most {max} characters, on one line"
        )));
    }
    if required && value.is_empty() {
        return Err(ApiError::BadRequest(format!("{label} cannot be empty")));
    }
    Ok(())
}

fn paragraph(label: &str, value: &mut String) -> Result<(), ApiError> {
    *value = value.trim().to_owned();
    let hidden = value
        .chars()
        .any(|c| is_hidden_char(c) && c != '\n' && c != '\r' && c != '\t');
    if value.chars().count() > MAX_PARAGRAPH || hidden {
        return Err(ApiError::BadRequest(format!(
            "{label} must be at most {MAX_PARAGRAPH} characters"
        )));
    }
    Ok(())
}

/// Parts of a style sheet that must never be stored: anything that loads something from outside,
/// runs script or covers the whole page. Comments are removed first so they cannot split a word.
pub fn check_css(css: &str) -> Result<(), ApiError> {
    let mut plain = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        plain.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    plain.push_str(rest);
    // A backslash can spell any letter (`u\72l(` is `url(`), so no check below could be trusted.
    if plain.contains('\\') {
        return Err(ApiError::BadRequest(
            "the CSS cannot contain backslash escapes".into(),
        ));
    }
    let squashed: String = plain
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    for bad in [
        "@import",
        "expression(",
        "javascript:",
        "behavior:",
        "-moz-binding",
        "</",
        "position:fixed",
    ] {
        if squashed.contains(bad) {
            return Err(ApiError::BadRequest(format!(
                "the CSS cannot contain {bad}"
            )));
        }
    }
    let mut after = squashed.as_str();
    while let Some(at) = after.find("url(") {
        let target = after[at + 4..].trim_start_matches(['"', '\'']);
        if !target.starts_with("data:image/") {
            return Err(ApiError::BadRequest(
                "the CSS can only use url() for data:image/ pictures".into(),
            ));
        }
        after = &after[at + 4..];
    }
    Ok(())
}

/// The HTML with everything that could run or load from outside removed.
pub fn clean_html(html: &str) -> String {
    let tags: HashSet<&str> = [
        "a",
        "abbr",
        "article",
        "aside",
        "b",
        "blockquote",
        "br",
        "caption",
        "code",
        "col",
        "colgroup",
        "dd",
        "details",
        "div",
        "dl",
        "dt",
        "em",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "i",
        "img",
        "li",
        "main",
        "mark",
        "nav",
        "ol",
        "p",
        "pre",
        "section",
        "small",
        "span",
        "strong",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "td",
        "th",
        "thead",
        "tr",
        "u",
        "ul",
    ]
    .into_iter()
    .collect();
    let generic: HashSet<&str> = ["class", "id", "title", "role", "aria-label"]
        .into_iter()
        .collect();
    ammonia::Builder::empty()
        .tags(tags)
        .generic_attributes(generic)
        .add_tag_attributes("a", ["href"])
        .add_tag_attributes("img", ["src", "alt", "width", "height"])
        .url_schemes(["http", "https", "mailto", "tel"].into_iter().collect())
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(html)
        .to_string()
}

impl Home {
    pub fn validate(&mut self) -> Result<(), ApiError> {
        line("headline", &mut self.headline, MAX_LINE, true)?;
        line("sign in button", &mut self.signin_label, 40, true)?;
        line("sign up button", &mut self.signup_label, 40, true)?;
        line("welcome text", &mut self.welcome, 80, true)?;
        line("events button", &mut self.continue_label, 40, true)?;
        line("about title", &mut self.about_title, 80, false)?;
        line("footer", &mut self.footer, MAX_LINE, false)?;
        paragraph("subheadline", &mut self.subheadline)?;
        paragraph("banner", &mut self.banner)?;
        paragraph("about text", &mut self.about_text)?;
        if self.custom_html.len() > MAX_HTML || self.custom_css.len() > MAX_CSS {
            return Err(ApiError::BadRequest(format!(
                "custom HTML is at most {} KB and CSS at most {} KB",
                MAX_HTML / 1000,
                MAX_CSS / 1000
            )));
        }
        check_css(&self.custom_css)?;
        self.custom_html = clean_html(&self.custom_html);
        if self.mode == Mode::Custom && self.custom_html.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "custom mode needs some HTML; switch to the default page or add it".into(),
            ));
        }
        Ok(())
    }
}

pub async fn load(pool: &SqlitePool) -> Result<Home, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub async fn save(pool: &SqlitePool, home: &Home) -> Result<(), ApiError> {
    let json = serde_json::to_string(home).map_err(|_| ApiError::Internal)?;
    sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(KEY)
        .bind(json)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn reset(pool: &SqlitePool) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM setting WHERE key = ?")
        .bind(KEY)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cleaner_keeps_layout_and_drops_everything_that_runs() {
        let dirty = r#"<section class="hero" id="top" onclick="x()"><h1>Hi</h1>
<script>alert(1)</script><iframe src="https://e.example"></iframe>
<form action="/x"><input name="a"></form><style>body{}</style>
<a href="javascript:alert(1)">bad</a><a href="/signin" onmouseover="y()">Sign in</a>
<img src="https://e.example/p.png" alt="p" onerror="z()"><p style="color:red">t</p></section>"#;
        let clean = clean_html(dirty);
        for gone in [
            "script",
            "iframe",
            "<form",
            "<input",
            "<style",
            "onclick",
            "onerror",
            "onmouseover",
            "javascript:",
            "style=",
        ] {
            assert!(!clean.contains(gone), "{gone} survived: {clean}");
        }
        assert!(
            clean.contains(r#"<section class="hero" id="top">"#),
            "{clean}"
        );
        assert!(clean.contains(r#"href="/signin""#), "{clean}");
        assert!(
            clean.contains("rel=\"noopener noreferrer nofollow\""),
            "{clean}"
        );
        assert!(clean.contains("<h1>Hi</h1>"), "{clean}");
    }

    #[test]
    fn css_may_style_but_not_load_or_cover_the_page() {
        assert!(check_css(".hero { background: #123; color: var(--color-accent); }").is_ok());
        assert!(check_css(".a { background: url(data:image/png;base64,AAAA) }").is_ok());
        for bad in [
            "@import url(https://e.example/x.css);",
            ".a { background: url(https://e.example/p.png) }",
            ".a { background: url( 'http://e.example/p.png' ) }",
            ".a { width: expression(alert(1)) }",
            ".a { position: fixed; inset: 0 }",
            ".a { position:/* x */fixed }",
            "@im/**/port 'x';",
            "</style><script>",
            ".a { background: u\\72l(https://e.example/p.png) }",
        ] {
            assert!(check_css(bad).is_err(), "accepted: {bad}");
        }
    }

    #[test]
    fn texts_are_checked_and_custom_mode_needs_content() {
        let mut ok = Home::default();
        assert!(ok.validate().is_ok());
        let mut long = Home {
            headline: "x".repeat(MAX_LINE + 1),
            ..Home::default()
        };
        assert!(long.validate().is_err());
        let mut empty = Home {
            signin_label: "  ".into(),
            ..Home::default()
        };
        assert!(empty.validate().is_err());
        let mut hidden = Home {
            footer: "a\u{202E}b".into(),
            ..Home::default()
        };
        assert!(hidden.validate().is_err());
        let mut nothing = Home {
            mode: Mode::Custom,
            ..Home::default()
        };
        assert!(nothing.validate().is_err());
        let mut custom = Home {
            mode: Mode::Custom,
            custom_html: "<h1>Hi</h1><script>x</script>".into(),
            ..Home::default()
        };
        assert!(custom.validate().is_ok());
        assert_eq!(custom.custom_html, "<h1>Hi</h1>");
        // A paragraph may have line breaks, a one line field may not.
        let mut para = Home {
            about_text: "one\ntwo".into(),
            ..Home::default()
        };
        assert!(para.validate().is_ok());
        let mut one_line = Home {
            footer: "one\ntwo".into(),
            ..Home::default()
        };
        assert!(one_line.validate().is_err());
    }
}
