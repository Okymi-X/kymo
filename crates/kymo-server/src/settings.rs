use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;

const KEY: &str = "appearance";
const MAX_SITE_NAME: usize = 40;
const MAX_RADIUS: u8 = 20;
const MAX_TAGLINE: usize = 120;
const MAX_LOGO: usize = 80_000;
const LOGO_PREFIXES: [&str; 3] = [
    "data:image/png;base64,",
    "data:image/jpeg;base64,",
    "data:image/webp;base64,",
];
const DEFAULT_TAGLINE: &str = "Knockout brackets for CTF events, updated as results come in.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontChoice {
    Sans,
    Serif,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scheme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tint {
    Neutral,
    Warm,
    Cool,
}

fn default_tagline() -> String {
    DEFAULT_TAGLINE.into()
}

/// How the instance looks. Stored as one JSON row so new options need no migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub site_name: String,
    pub accent_light: String,
    pub accent_dark: String,
    pub radius: u8,
    pub font: FontChoice,
    #[serde(default = "default_scheme")]
    pub scheme: Scheme,
    #[serde(default = "default_density")]
    pub density: Density,
    #[serde(default = "default_tagline")]
    pub tagline: String,
    #[serde(default = "default_tint")]
    pub tint: Tint,
    #[serde(default = "default_avatars")]
    pub avatars: bool,
    /// Header logo as a small raster data URI. SVG is refused so no script can ride along.
    #[serde(default)]
    pub logo: Option<String>,
}

fn default_tint() -> Tint {
    Tint::Neutral
}

fn default_avatars() -> bool {
    true
}

fn default_scheme() -> Scheme {
    Scheme::System
}

fn default_density() -> Density {
    Density::Comfortable
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            site_name: "Kymo".into(),
            accent_light: "#0a6fe0".into(),
            accent_dark: "#4da3ff".into(),
            radius: 8,
            font: FontChoice::Sans,
            scheme: Scheme::System,
            density: Density::Comfortable,
            tagline: default_tagline(),
            tint: Tint::Neutral,
            avatars: true,
            logo: None,
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Appearance {
    pub fn validate(&mut self) -> Result<(), ApiError> {
        self.site_name = self.site_name.trim().to_owned();
        let name_len = self.site_name.chars().count();
        if name_len == 0 || name_len > MAX_SITE_NAME {
            return Err(ApiError::BadRequest(format!(
                "site name must be 1 to {MAX_SITE_NAME} characters"
            )));
        }
        self.tagline = self.tagline.trim().to_owned();
        if self.tagline.chars().count() > MAX_TAGLINE
            || self.tagline.chars().any(crate::text::is_hidden_char)
        {
            return Err(ApiError::BadRequest(format!(
                "tagline must be at most {MAX_TAGLINE} characters without control characters"
            )));
        }
        if self.site_name.chars().any(crate::text::is_hidden_char) {
            return Err(ApiError::BadRequest(
                "site name cannot contain control characters".into(),
            ));
        }
        if !is_hex_color(&self.accent_light) || !is_hex_color(&self.accent_dark) {
            return Err(ApiError::BadRequest("colors must look like #0a6fe0".into()));
        }
        if self.radius > MAX_RADIUS {
            return Err(ApiError::BadRequest(format!(
                "radius must be at most {MAX_RADIUS}"
            )));
        }
        if let Some(logo) = self.logo.as_deref() {
            let body = LOGO_PREFIXES.iter().find_map(|p| logo.strip_prefix(p));
            let valid = logo.len() <= MAX_LOGO
                && body.is_some_and(|b| {
                    !b.is_empty()
                        && b.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
                });
            if !valid {
                return Err(ApiError::BadRequest(
                    "logo must be a PNG, JPEG or WebP image under 60 KB".into(),
                ));
            }
        }
        Ok(())
    }
}

pub async fn load(pool: &SqlitePool) -> Result<Appearance, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub async fn save(pool: &SqlitePool, appearance: &Appearance) -> Result<(), ApiError> {
    let json = serde_json::to_string(appearance).map_err(|_| ApiError::Internal)?;
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

const FONT_STACKS: [(FontChoice, &str); 3] = [
    (
        FontChoice::Sans,
        "'Inter', ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
    ),
    (
        FontChoice::Serif,
        "'Iowan Old Style', 'Palatino Linotype', Palatino, Georgia, serif",
    ),
    (
        FontChoice::Mono,
        "'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, monospace",
    ),
];

impl Appearance {
    /// CSS that applies this look. Mirrors `themeCss` in the web app, so the server can ship it
    /// inside index.html and the first paint already has the saved look. Only validated values
    /// (hex colors, bounded numbers, fixed font list) reach the text.
    pub fn css(&self) -> String {
        let font = FONT_STACKS
            .iter()
            .find(|(f, _)| *f == self.font)
            .map_or(FONT_STACKS[0].1, |(_, s)| s);
        let size = if self.density == Density::Compact {
            "14.5px"
        } else {
            "16px"
        };
        let tint = match self.tint {
            Tint::Neutral => "",
            Tint::Warm => {
                ":root{--color-bg:#faf8f5;--color-surface:#fffefc;--color-line:#e8e4dd;}:root[data-scheme=\"dark\"]{--color-bg:#0d0c0b;--color-surface:#161412;--color-line:#2d2a26;}@media (prefers-color-scheme:dark){:root:not([data-scheme=\"light\"]){--color-bg:#0d0c0b;--color-surface:#161412;--color-line:#2d2a26;}}"
            }
            Tint::Cool => {
                ":root{--color-bg:#f6f8fb;--color-surface:#fcfdff;--color-line:#dfe5ee;}:root[data-scheme=\"dark\"]{--color-bg:#090c10;--color-surface:#11151b;--color-line:#232a34;}@media (prefers-color-scheme:dark){:root:not([data-scheme=\"light\"]){--color-bg:#090c10;--color-surface:#11151b;--color-line:#232a34;}}"
            }
        };
        let avatars = if self.avatars {
            ""
        } else {
            "[data-avatar]{display:none !important;}"
        };
        format!(
            ":root{{--color-accent:{};--radius-control:{}px;--radius-card:{}px;--font-sans:{};}}:root[data-scheme=\"dark\"]{{--color-accent:{};}}@media (prefers-color-scheme:dark){{:root:not([data-scheme=\"light\"]){{--color-accent:{};}}}}html{{font-size:{size};}}{tint}{avatars}",
            self.accent_light,
            self.radius,
            u16::from(self.radius) + 4,
            font,
            self.accent_dark,
            self.accent_dark,
        )
    }

    pub fn scheme_attr(&self) -> &'static str {
        match self.scheme {
            Scheme::System => "system",
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        }
    }
}
