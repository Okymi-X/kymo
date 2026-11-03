//! Storage for the VPN: player identities, revoked certificates, lab networks, and the few
//! settings the authority needs. Rules live in kymo-vpn.

use std::collections::BTreeSet;

use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::jeopardy_store::Scorer;

pub const SETTING_CA: &str = "vpn_ca";
pub const SETTING_SERVER: &str = "vpn_server";
pub const SETTING_TLS_CRYPT: &str = "vpn_tls_crypt";
pub const SETTING_CRL_NUMBER: &str = "vpn_crl_number";
pub const SETTING_CRL_COUNT: &str = "vpn_crl_count";

pub async fn setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------- clients ----------

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ClientRow {
    pub user_id: i64,
    pub idx: i64,
    pub serial: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

pub async fn client(pool: &SqlitePool, user_id: i64) -> Result<Option<ClientRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM vpn_client WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn clients(pool: &SqlitePool) -> Result<Vec<ClientRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM vpn_client ORDER BY idx")
        .fetch_all(pool)
        .await?)
}

pub async fn used_client_indices(pool: &SqlitePool) -> Result<BTreeSet<u32>, ApiError> {
    let rows: Vec<i64> = sqlx::query_scalar("SELECT idx FROM vpn_client")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|i| u32::try_from(i).ok())
        .collect())
}

/// Replaces (or creates) a player's identity. The previous serial, if any, is recorded as
/// revoked in the same transaction so the two never disagree.
pub async fn upsert_client(
    pool: &SqlitePool,
    user_id: i64,
    idx: u32,
    serial: &str,
    now: i64,
    expires_at: i64,
) -> Result<Option<String>, ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let previous: Option<String> =
        sqlx::query_scalar("SELECT serial FROM vpn_client WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(old) = &previous {
        sqlx::query("INSERT OR IGNORE INTO vpn_revoked (serial, at) VALUES (?, ?)")
            .bind(old)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE vpn_client SET serial = ?, issued_at = ?, expires_at = ? WHERE user_id = ?",
        )
        .bind(serial)
        .bind(now)
        .bind(expires_at)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("INSERT INTO vpn_client (user_id, idx, serial, issued_at, expires_at) VALUES (?, ?, ?, ?, ?)")
            .bind(user_id)
            .bind(i64::from(idx))
            .bind(serial)
            .bind(now)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(previous)
}

/// Revokes a player's current certificate and forgets the identity.
pub async fn remove_client(pool: &SqlitePool, user_id: i64, now: i64) -> Result<bool, ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let serial: Option<String> =
        sqlx::query_scalar("SELECT serial FROM vpn_client WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(serial) = serial else {
        return Ok(false);
    };
    sqlx::query("INSERT OR IGNORE INTO vpn_revoked (serial, at) VALUES (?, ?)")
        .bind(&serial)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM vpn_client WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn revoked(pool: &SqlitePool) -> Result<Vec<(String, i64)>, ApiError> {
    Ok(
        sqlx::query_as("SELECT serial, at FROM vpn_revoked ORDER BY at, serial")
            .fetch_all(pool)
            .await?,
    )
}

// ---------- labs ----------

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct LabRow {
    pub id: i64,
    pub idx: i64,
    pub event_id: i64,
    pub scorer_kind: String,
    pub scorer_id: i64,
    pub network: String,
    pub created_at: i64,
    pub last_used_at: i64,
}

pub async fn lab_of(
    pool: &SqlitePool,
    event_id: i64,
    scorer: Scorer,
) -> Result<Option<LabRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT * FROM vpn_lab WHERE event_id = ? AND scorer_kind = ? AND scorer_id = ?",
    )
    .bind(event_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_optional(pool)
    .await?)
}

pub async fn labs(pool: &SqlitePool) -> Result<Vec<LabRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM vpn_lab ORDER BY idx")
        .fetch_all(pool)
        .await?)
}

pub async fn used_lab_indices(pool: &SqlitePool) -> Result<BTreeSet<u32>, ApiError> {
    let rows: Vec<i64> = sqlx::query_scalar("SELECT idx FROM vpn_lab")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|i| u32::try_from(i).ok())
        .collect())
}

pub async fn insert_lab(
    pool: &SqlitePool,
    idx: u32,
    event_id: i64,
    scorer: Scorer,
    network: &str,
    now: i64,
) -> Result<LabRow, ApiError> {
    sqlx::query("INSERT INTO vpn_lab (idx, event_id, scorer_kind, scorer_id, network, created_at, last_used_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(i64::from(idx))
        .bind(event_id)
        .bind(scorer.kind())
        .bind(scorer.id())
        .bind(network)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
    lab_of(pool, event_id, scorer)
        .await?
        .ok_or(ApiError::Internal)
}

pub async fn touch_lab(pool: &SqlitePool, id: i64, now: i64) -> Result<(), ApiError> {
    sqlx::query("UPDATE vpn_lab SET last_used_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_lab(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM vpn_lab WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Addresses in use by live instances of a lab.
pub async fn lab_ips_in_use(pool: &SqlitePool, lab_id: i64) -> Result<BTreeSet<String>, ApiError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT i.lab_ip FROM instance i JOIN vpn_lab l ON l.event_id = i.event_id AND l.scorer_kind = i.scorer_kind AND l.scorer_id = i.scorer_id WHERE l.id = ? AND i.lab_ip IS NOT NULL AND i.state IN ('starting', 'running')",
    )
    .bind(lab_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

pub async fn live_instances_in_lab(pool: &SqlitePool, lab: &LabRow) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM instance WHERE event_id = ? AND scorer_kind = ? AND scorer_id = ? AND state IN ('starting', 'running')",
    )
    .bind(lab.event_id)
    .bind(&lab.scorer_kind)
    .bind(lab.scorer_id)
    .fetch_one(pool)
    .await?)
}

/// Player accounts that may use a participant's lab: the player, or every member of the team.
pub async fn lab_members(pool: &SqlitePool, kind: &str, id: i64) -> Result<Vec<i64>, ApiError> {
    if kind == "team" {
        Ok(
            sqlx::query_scalar("SELECT user_id FROM player_team_member WHERE team_id = ?")
                .bind(id)
                .fetch_all(pool)
                .await?,
        )
    } else {
        Ok(vec![id])
    }
}

pub async fn is_banned(pool: &SqlitePool, user_id: i64) -> Result<bool, ApiError> {
    let banned: Option<bool> = sqlx::query_scalar("SELECT banned FROM user WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(banned.unwrap_or(true))
}

/// The address given to the most recent instance of a lab, whatever became of it.
pub async fn last_lab_ip(pool: &SqlitePool, lab_id: i64) -> Result<Option<String>, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT i.lab_ip FROM instance i JOIN vpn_lab l ON l.event_id = i.event_id AND l.scorer_kind = i.scorer_kind AND l.scorer_id = i.scorer_id WHERE l.id = ? AND i.lab_ip IS NOT NULL ORDER BY i.id DESC LIMIT 1",
    )
    .bind(lab_id)
    .fetch_optional(pool)
    .await?)
}
