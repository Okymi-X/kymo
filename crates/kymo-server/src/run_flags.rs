//! The flags of one instance run: generated when the run starts, kept with it, and handed to
//! the lab host. A flag from an earlier run stays valid for submission, so the rows outlive the
//! run.

use kymo_instances::{FlagDelivery, RunFlag};
use kymo_jeopardy::{FlagSpec, dynamic};
use sqlx::SqlitePool;

use crate::challenge_store::{self, ChallengeRow};
use crate::error::ApiError;

/// A flag for one run, with the challenge flag it belongs to when it is a row of
/// `challenge_flag`. A static Responder flag has no row.
pub struct PlannedFlag {
    pub flag_id: Option<i64>,
    pub flag: RunFlag,
}

fn random_bytes(random_len: usize) -> Vec<u8> {
    (0..dynamic::bytes_needed(random_len))
        .map(|_| rand::random::<u8>())
        .collect()
}

/// The flags a new run of `challenge` needs. Empty for an ordinary challenge.
pub async fn plan(
    pool: &SqlitePool,
    challenge: &ChallengeRow,
) -> Result<Vec<PlannedFlag>, ApiError> {
    let deliver = if challenge.responder {
        FlagDelivery::Hold
    } else {
        FlagDelivery::Inject
    };
    if challenge.flag_mode == "dynamic" {
        let rows = challenge_store::flag_rows(pool, challenge.id).await?;
        if rows.is_empty() {
            return Err(ApiError::Internal);
        }
        return Ok(rows
            .into_iter()
            .map(|row| {
                let random_len =
                    usize::try_from(row.random_len).unwrap_or(dynamic::DEFAULT_RANDOM_LEN);
                let value = dynamic::generate(
                    &row.value,
                    &row.closing,
                    random_len,
                    &random_bytes(random_len),
                );
                PlannedFlag {
                    flag_id: Some(row.id),
                    flag: RunFlag {
                        name: row.name,
                        value,
                        deliver,
                    },
                }
            })
            .collect());
    }
    if challenge.responder {
        // A static Responder: the Verifier releases the configured flag.
        let FlagSpec::Static { value } = &challenge.flag else {
            return Err(ApiError::Internal);
        };
        return Ok(vec![PlannedFlag {
            flag_id: None,
            flag: RunFlag {
                name: "flag".into(),
                value: value.clone(),
                deliver,
            },
        }]);
    }
    Ok(Vec::new())
}

/// Keeps the generated flags with the run, so they can be checked at submission.
pub async fn keep(pool: &SqlitePool, run_id: i64, planned: &[PlannedFlag]) -> Result<(), ApiError> {
    for item in planned {
        let Some(flag_id) = item.flag_id else {
            continue;
        };
        sqlx::query("INSERT INTO instance_flag (instance_id, flag_id, value) VALUES (?, ?, ?)")
            .bind(run_id)
            .bind(flag_id)
            .bind(&item.flag.value)
            .execute(pool)
            .await?;
    }
    Ok(())
}
