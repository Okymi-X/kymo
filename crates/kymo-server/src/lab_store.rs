//! Storage of lab hosts. The token is write-only: it is stored, used and never sent back.

use kymo_lab::client::parse_fingerprint;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;

#[derive(Debug, Clone, FromRow)]
pub struct LabHostRow {
    pub id: i64,
    pub name: String,
    pub api_url: String,
    pub token: String,
    pub fingerprint: Option<String>,
    pub public_host: String,
    pub enabled: bool,
    pub created_at: i64,
    pub last_checked_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub vpn_host: Option<String>,
    pub vpn_port: Option<i64>,
    pub vpn_only: bool,
}

/// A lab host as the admin API shows it: everything except the token.
#[derive(Debug, Serialize)]
pub struct LabHostOut {
    pub id: i64,
    pub name: String,
    pub api_url: String,
    pub fingerprint: Option<String>,
    pub public_host: String,
    pub enabled: bool,
    pub created_at: i64,
    pub last_checked_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    /// The host's own VPN gateway as it last reported itself, if it has one.
    pub vpn: Option<LabVpnOut>,
    /// Events set to run their instances here.
    pub events: i64,
    /// Instances running here right now.
    pub live_instances: i64,
}

#[derive(Debug, Serialize)]
pub struct LabVpnOut {
    pub host: String,
    pub port: i64,
    /// The host refuses to publish ports: every instance must go through the gateway.
    pub only: bool,
}

impl LabHostRow {
    pub fn out(self, events: i64, live_instances: i64) -> LabHostOut {
        LabHostOut {
            id: self.id,
            name: self.name,
            api_url: self.api_url,
            fingerprint: self.fingerprint,
            public_host: self.public_host,
            enabled: self.enabled,
            created_at: self.created_at,
            last_checked_at: self.last_checked_at,
            last_ok_at: self.last_ok_at,
            last_error: self.last_error,
            vpn: self.vpn_host.map(|host| LabVpnOut {
                host,
                port: self.vpn_port.unwrap_or(1194),
                only: self.vpn_only,
            }),
            events,
            live_instances,
        }
    }
}

/// What an admin may set. `token` left out on an update keeps the stored one.
#[derive(Debug, Clone, Default)]
pub struct LabFields {
    pub name: Option<String>,
    pub api_url: Option<String>,
    pub token: Option<String>,
    /// `Some("")` clears the pin.
    pub fingerprint: Option<String>,
    pub public_host: Option<String>,
    pub enabled: Option<bool>,
}

pub fn check_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    let ok = (2..=40).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || " ._-".contains(c));
    if ok {
        Ok(name.to_owned())
    } else {
        Err(ApiError::BadRequest(
            "the name is 2 to 40 characters: letters, digits, spaces, dots, dashes and underscores"
                .into(),
        ))
    }
}

pub fn check_url(url: &str) -> Result<String, ApiError> {
    let url = url.trim().trim_end_matches('/').to_owned();
    // The client builder knows what it accepts; reuse it so the form and the connection agree.
    kymo_lab::RemoteRuntime::new(&url, "x", None, "x").map_err(ApiError::BadRequest)?;
    if url.len() > 200 || url.contains(char::is_whitespace) {
        return Err(ApiError::BadRequest("the lab URL is not valid".into()));
    }
    Ok(url)
}

pub fn check_token(token: &str) -> Result<String, ApiError> {
    let token = token.trim();
    if !(16..=200).contains(&token.len()) || token.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "the token is 16 to 200 characters".into(),
        ));
    }
    Ok(token.to_owned())
}

/// A pinned fingerprint, normalised to `AB:CD:...`, or None for no pin.
pub fn check_fingerprint(text: &str) -> Result<Option<String>, ApiError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let bytes = parse_fingerprint(text).map_err(ApiError::BadRequest)?;
    Ok(Some(
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    ))
}

pub fn check_host(host: &str) -> Result<String, ApiError> {
    let host = host.trim();
    let ok = (1..=253).contains(&host.len())
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:-".contains(c))
        && !host.starts_with(['-', '.']);
    if ok {
        Ok(host.to_owned())
    } else {
        Err(ApiError::BadRequest(
            "the public host is a name or address players can reach, such as ctf.example.org or 203.0.113.7".into(),
        ))
    }
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<LabHostRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM lab_host ORDER BY name")
        .fetch_all(pool)
        .await?)
}

pub async fn enabled(pool: &SqlitePool) -> Result<Vec<LabHostRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM lab_host WHERE enabled = 1 ORDER BY id")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<LabHostRow, ApiError> {
    sqlx::query_as("SELECT * FROM lab_host WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn counts(pool: &SqlitePool, id: i64) -> Result<(i64, i64), ApiError> {
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event WHERE lab_host_id = ? OR id IN (SELECT event_id FROM challenge WHERE lab_host_id = ?)",
    )
    .bind(id)
    .bind(id)
    .fetch_one(pool)
    .await?;
    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM instance WHERE lab_host_id = ? AND state IN ('starting', 'running')",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok((events, live))
}

pub async fn insert(
    pool: &SqlitePool,
    name: &str,
    url: &str,
    token: &str,
    fingerprint: Option<&str>,
    host: &str,
    now: i64,
) -> Result<i64, ApiError> {
    let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM lab_host WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    if taken.is_some() {
        return Err(ApiError::Conflict(
            "a lab host with that name already exists".into(),
        ));
    }
    let done = sqlx::query("INSERT INTO lab_host (name, api_url, token, fingerprint, public_host, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(name)
        .bind(url)
        .bind(token)
        .bind(fingerprint)
        .bind(host)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(done.last_insert_rowid())
}

pub async fn update(pool: &SqlitePool, id: i64, fields: &LabFields) -> Result<(), ApiError> {
    let current = get(pool, id).await?;
    let name = match &fields.name {
        Some(n) => check_name(n)?,
        None => current.name.clone(),
    };
    if name != current.name {
        let taken: Option<i64> =
            sqlx::query_scalar("SELECT id FROM lab_host WHERE name = ? AND id <> ?")
                .bind(&name)
                .bind(id)
                .fetch_optional(pool)
                .await?;
        if taken.is_some() {
            return Err(ApiError::Conflict(
                "a lab host with that name already exists".into(),
            ));
        }
    }
    let url = match &fields.api_url {
        Some(u) => check_url(u)?,
        None => current.api_url.clone(),
    };
    let token = match &fields.token {
        Some(t) if !t.trim().is_empty() => check_token(t)?,
        _ => current.token.clone(),
    };
    let fingerprint = match &fields.fingerprint {
        Some(f) => check_fingerprint(f)?,
        None => current.fingerprint.clone(),
    };
    if fingerprint.is_some() && !url.starts_with("https://") {
        return Err(ApiError::BadRequest(
            "a pinned fingerprint needs an https:// URL".into(),
        ));
    }
    let host = match &fields.public_host {
        Some(h) => check_host(h)?,
        None => current.public_host.clone(),
    };
    let enabled = fields.enabled.unwrap_or(current.enabled);
    sqlx::query("UPDATE lab_host SET name = ?, api_url = ?, token = ?, fingerprint = ?, public_host = ?, enabled = ? WHERE id = ?")
        .bind(name)
        .bind(url)
        .bind(token)
        .bind(fingerprint)
        .bind(host)
        .bind(enabled)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    get(pool, id).await?;
    let (events, live) = counts(pool, id).await?;
    if events > 0 {
        return Err(ApiError::Conflict(format!(
            "{events} event(s) still run their instances here: move them to another lab host first"
        )));
    }
    if live > 0 {
        return Err(ApiError::Conflict(
            "instances are still running here: stop them first".into(),
        ));
    }
    // Its VPN state goes with it: labs, player profiles and revocations that live on this host.
    // Left behind, they would point at a host that no longer exists.
    let mut tx = pool.begin().await?;
    for table in ["vpn_lab", "vpn_client", "vpn_revoked"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE lab_host_id = ?"))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM lab_host WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// What the last health check found.
pub async fn record_check(
    pool: &SqlitePool,
    id: i64,
    now: i64,
    error: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE lab_host SET last_checked_at = ?, last_ok_at = CASE WHEN ? IS NULL THEN ? ELSE last_ok_at END, last_error = ? WHERE id = ?")
        .bind(now)
        .bind(error)
        .bind(now)
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remembers what the lab host said about its gateway (or that it has none).
pub async fn record_vpn(
    pool: &SqlitePool,
    id: i64,
    vpn: Option<&kymo_lab::VpnInfo>,
    only: bool,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE lab_host SET vpn_host = ?, vpn_port = ?, vpn_only = ? WHERE id = ?")
        .bind(vpn.map(|v| v.host.as_str()))
        .bind(vpn.map(|v| i64::from(v.port)))
        .bind(vpn.is_some() && only)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
