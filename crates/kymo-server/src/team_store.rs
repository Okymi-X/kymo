use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;

#[derive(Debug, Clone, FromRow)]
pub struct TeamRow {
    pub id: i64,
    pub name: String,
    pub owner_user_id: i64,
    pub invite_code: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Member {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub joined_at: i64,
}

/// A team as its members see it. Only the owner gets the invite code.
#[derive(Debug, Serialize)]
pub struct TeamOut {
    pub id: i64,
    pub name: String,
    pub owner_user_id: i64,
    pub invite_code: Option<String>,
    pub created_at: i64,
    pub members: Vec<Member>,
}

pub fn fresh_code() -> String {
    rand::random::<[u8; 8]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn by_id(pool: &SqlitePool, id: i64) -> Result<TeamRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM player_team WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn by_code(pool: &SqlitePool, code: &str) -> Result<TeamRow, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM player_team WHERE invite_code = ?")
            .bind(code)
            .fetch_one(pool)
            .await?,
    )
}

/// The team the user belongs to, if any.
pub async fn of_user(pool: &SqlitePool, user_id: i64) -> Result<Option<TeamRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT t.* FROM player_team t JOIN player_team_member m ON m.team_id = t.id WHERE m.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn is_member(pool: &SqlitePool, team_id: i64, user_id: i64) -> Result<bool, ApiError> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM player_team_member WHERE team_id = ? AND user_id = ?")
            .bind(team_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

pub async fn size(pool: &SqlitePool, team_id: i64) -> Result<u32, ApiError> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM player_team_member WHERE team_id = ?")
        .bind(team_id)
        .fetch_one(pool)
        .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    owner: i64,
    now: i64,
) -> Result<TeamRow, ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let id = sqlx::query(
        "INSERT INTO player_team (name, owner_user_id, invite_code, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(name)
    .bind(owner)
    .bind(fresh_code())
    .bind(now)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO player_team_member (team_id, user_id, role, joined_at) VALUES (?, ?, 'owner', ?)",
    )
    .bind(id)
    .bind(owner)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    by_id(pool, id).await
}

pub async fn add_member(
    pool: &SqlitePool,
    team_id: i64,
    user_id: i64,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO player_team_member (team_id, user_id, role, joined_at) VALUES (?, ?, 'member', ?)",
    )
    .bind(team_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a member. The last member deletes the team; when the owner leaves a team that
/// still has people, the longest standing member becomes the owner.
pub async fn remove_member(pool: &SqlitePool, team_id: i64, user_id: i64) -> Result<(), ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM player_team_member WHERE team_id = ? AND user_id = ?")
        .bind(team_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let next: Option<i64> = sqlx::query_scalar(
        "SELECT user_id FROM player_team_member WHERE team_id = ? ORDER BY joined_at, user_id LIMIT 1",
    )
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await?;
    match next {
        None => {
            sqlx::query("DELETE FROM player_team WHERE id = ?")
                .bind(team_id)
                .execute(&mut *tx)
                .await?;
        }
        Some(heir) => {
            let owner: i64 =
                sqlx::query_scalar("SELECT owner_user_id FROM player_team WHERE id = ?")
                    .bind(team_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if owner == user_id {
                sqlx::query("UPDATE player_team SET owner_user_id = ? WHERE id = ?")
                    .bind(heir)
                    .bind(team_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE player_team_member SET role = 'owner' WHERE team_id = ? AND user_id = ?")
                    .bind(team_id)
                    .bind(heir)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn rotate_code(pool: &SqlitePool, team_id: i64) -> Result<String, ApiError> {
    let code = fresh_code();
    sqlx::query("UPDATE player_team SET invite_code = ? WHERE id = ?")
        .bind(&code)
        .bind(team_id)
        .execute(pool)
        .await?;
    Ok(code)
}

pub async fn view(pool: &SqlitePool, team: TeamRow, viewer: i64) -> Result<TeamOut, ApiError> {
    let members: Vec<Member> = sqlx::query_as(
        "SELECT m.user_id, u.username, u.display_name, m.role, m.joined_at FROM player_team_member m JOIN user u ON u.id = m.user_id WHERE m.team_id = ? ORDER BY m.joined_at, m.user_id",
    )
    .bind(team.id)
    .fetch_all(pool)
    .await?;
    Ok(TeamOut {
        invite_code: (team.owner_user_id == viewer).then_some(team.invite_code),
        id: team.id,
        name: team.name,
        owner_user_id: team.owner_user_id,
        created_at: team.created_at,
        members,
    })
}
