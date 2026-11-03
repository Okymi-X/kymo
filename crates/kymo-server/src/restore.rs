//! Offline restore of a database snapshot. Run while the server is stopped:
//! `kymo-server restore <backup.db>`. The backup is checked before anything is touched, and
//! the current database is kept next to the target as `<name>.pre-restore-<unix time>`.

use std::path::{Path, PathBuf};

use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};

/// The newest migration this binary knows about.
fn newest_known_migration() -> i64 {
    sqlx::migrate!()
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

/// Path of the database file named by a `sqlite:` URL.
pub fn path_of_url(url: &str) -> Result<PathBuf, String> {
    let rest = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .ok_or("the database URL must start with sqlite:")?;
    let path = rest.split('?').next().unwrap_or(rest);
    if path.is_empty() || path.contains(":memory:") {
        return Err("there is no database file to restore into".into());
    }
    Ok(PathBuf::from(path))
}

/// Opens the backup read only and checks that it is a healthy Kymo database that this binary
/// can migrate forward.
pub async fn verify(backup: &Path) -> Result<(), String> {
    if !backup.is_file() {
        return Err(format!("{} is not a file", backup.display()));
    }
    let options = SqliteConnectOptions::new()
        .filename(backup)
        .read_only(true)
        .create_if_missing(false);
    let mut conn = SqliteConnection::connect_with(&options)
        .await
        .map_err(|e| format!("cannot open the backup: {e}"))?;
    let check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("the backup is damaged: {e}"))?;
    if check != "ok" {
        return Err(format!("the backup failed its integrity check: {check}"));
    }
    let newest: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut conn)
        .await
        .map_err(|_| "this is not a Kymo database".to_owned())?;
    let newest = newest.ok_or("this is not a Kymo database")?;
    if newest > newest_known_migration() {
        return Err(format!(
            "the backup is from a newer Kymo (schema {newest}), upgrade this binary first"
        ));
    }
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('tournament', 'token')",
    )
    .fetch_one(&mut conn)
    .await
    .map_err(|e| e.to_string())?;
    // Tables from the first migrations, so a backup from any older version still qualifies.
    if tables != 2 {
        return Err("this is not a Kymo database".into());
    }
    Ok(())
}

/// Replaces `target` with `backup`. Returns the path where the previous database was kept, if
/// there was one. Refuses when the target looks in use (a non empty write ahead log) unless
/// `force` is set.
pub async fn restore(
    target: &Path,
    backup: &Path,
    force: bool,
    now: i64,
) -> Result<Option<PathBuf>, String> {
    verify(backup).await?;
    let wal = sibling(target, "-wal");
    if !force && std::fs::metadata(&wal).is_ok_and(|m| m.len() > 0) {
        return Err(
            "the database has unwritten changes, stop the server first (or use --force)".into(),
        );
    }
    let kept = if target.exists() {
        let kept = sibling(target, &format!(".pre-restore-{now}"));
        std::fs::copy(target, &kept)
            .map_err(|e| format!("cannot keep the current database: {e}"))?;
        Some(kept)
    } else {
        None
    };
    let staging = sibling(target, ".restoring");
    std::fs::copy(backup, &staging).map_err(|e| format!("cannot copy the backup: {e}"))?;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(target, suffix));
    }
    std::fs::rename(&staging, target)
        .map_err(|e| format!("cannot put the backup in place: {e}"))?;
    Ok(kept)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
