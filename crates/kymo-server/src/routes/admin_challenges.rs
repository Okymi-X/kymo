//! Organizer side of Jeopardy: categories, challenges, hints, attachments, release control,
//! flag testing, submission history and event settings. Needs the ManageChallenges permission.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kymo_jeopardy::{FlagSpec, Scoring, check};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::events::set_or_clear;
use crate::auth::{Require, needs};
use crate::challenge_store::{
    self as store, AttachmentMeta, Base64Upload, CategoryRow, ChallengeFields, ChallengeRow,
    HintRow, MAX_ATTACHMENT, MAX_TITLE,
};
use crate::error::ApiError;
use crate::event_store::{self, EventOut, EventRow};
use crate::jeopardy_store::{self, HistoryFilter, JeopardySettings, MAX_PER_PAGE, SubmissionPage};
use crate::state::AppState;
use crate::submit::verdict_str;
use crate::text::clean_name;

const MAX_ATTACHMENTS_PER_CHALLENGE: usize = 20;
const MAX_CATEGORY_NAME: usize = 60;

/// Raw upload size plus room for the base64 form and the JSON wrapper.
pub const UPLOAD_BODY_LIMIT: usize = MAX_ATTACHMENT / 3 * 4 + 16 * 1024;

fn bad(message: &str) -> ApiError {
    ApiError::BadRequest(message.into())
}

async fn jeopardy_event(state: &AppState, id: i64) -> Result<EventRow, ApiError> {
    let row = event_store::by_id(&state.pool, id).await?;
    if row.kind != "jeopardy" {
        return Err(ApiError::Conflict("this is not a Jeopardy event".into()));
    }
    Ok(row)
}

async fn event_of_challenge(state: &AppState, row: &ChallengeRow) -> Result<EventRow, ApiError> {
    event_store::by_id(&state.pool, row.event_id).await
}

// Categories

#[derive(Deserialize)]
pub struct CategoryBody {
    name: String,
    #[serde(default)]
    position: i64,
}

pub async fn list_categories(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
) -> Result<Json<Vec<CategoryRow>>, ApiError> {
    jeopardy_event(&state, event_id).await?;
    Ok(Json(store::categories(&state.pool, event_id).await?))
}

pub async fn create_category(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Json(body): Json<CategoryBody>,
) -> Result<(StatusCode, Json<CategoryRow>), ApiError> {
    jeopardy_event(&state, event_id).await?;
    let name = clean_name(&body.name, MAX_CATEGORY_NAME)?;
    let id = store::category_insert(&state.pool, event_id, &name, body.position).await?;
    Ok((
        StatusCode::CREATED,
        Json(store::category_by_id(&state.pool, id).await?),
    ))
}

#[derive(Deserialize)]
pub struct CategoryPatch {
    name: Option<String>,
    position: Option<i64>,
}

pub async fn update_category(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CategoryPatch>,
) -> Result<Json<CategoryRow>, ApiError> {
    let row = store::category_by_id(&state.pool, id).await?;
    let name = match &body.name {
        Some(name) => clean_name(name, MAX_CATEGORY_NAME)?,
        None => row.name,
    };
    store::category_update(
        &state.pool,
        id,
        &name,
        body.position.unwrap_or(row.position),
    )
    .await?;
    Ok(Json(store::category_by_id(&state.pool, id).await?))
}

pub async fn delete_category(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    store::category_by_id(&state.pool, id).await?;
    store::category_delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// Challenges

/// A challenge as the organizer sees it, flag included.
#[derive(Serialize)]
pub struct AdminChallenge {
    id: i64,
    event_id: i64,
    category_id: i64,
    title: String,
    description: String,
    scoring: Scoring,
    flag: FlagSpec,
    flag_prefix: String,
    flag_suffix: String,
    case_sensitive: bool,
    max_attempts: Option<i64>,
    state: String,
    release_at: Option<i64>,
    retire_at: Option<i64>,
    position: i64,
    requires_instance: bool,
    instance_config: Option<Value>,
    created_at: i64,
    solves: i64,
    hints: Vec<HintRow>,
    attachments: Vec<AttachmentMeta>,
}

fn admin_out(
    row: ChallengeRow,
    solves: i64,
    hints: Vec<HintRow>,
    attachments: Vec<AttachmentMeta>,
) -> AdminChallenge {
    AdminChallenge {
        instance_config: row
            .instance_config
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok()),
        id: row.id,
        event_id: row.event_id,
        category_id: row.category_id,
        title: row.title,
        description: row.description,
        scoring: row.scoring,
        flag: row.flag,
        flag_prefix: row.flag_prefix,
        flag_suffix: row.flag_suffix,
        case_sensitive: row.case_sensitive,
        max_attempts: row.max_attempts,
        state: row.state,
        release_at: row.release_at,
        retire_at: row.retire_at,
        position: row.position,
        requires_instance: row.requires_instance,
        created_at: row.created_at,
        solves,
        hints,
        attachments,
    }
}

async fn one(state: &AppState, row: ChallengeRow) -> Result<AdminChallenge, ApiError> {
    let totals = jeopardy_store::solve_totals(&state.pool, row.event_id).await?;
    let hints = store::hints(&state.pool, row.id).await?;
    let attachments = store::attachments_of_challenge(&state.pool, row.id).await?;
    let solves = totals.get(&row.id).copied().unwrap_or(0);
    Ok(admin_out(row, solves, hints, attachments))
}

pub async fn list_challenges(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
) -> Result<Json<Vec<AdminChallenge>>, ApiError> {
    jeopardy_event(&state, event_id).await?;
    let totals = jeopardy_store::solve_totals(&state.pool, event_id).await?;
    let hints = store::hints_of_event(&state.pool, event_id).await?;
    let attachments = store::attachments_of_event(&state.pool, event_id).await?;
    let list = store::challenges(&state.pool, event_id)
        .await?
        .into_iter()
        .map(|row| {
            let own_hints = hints
                .iter()
                .filter(|h| h.challenge_id == row.id)
                .cloned()
                .collect();
            let own_files = attachments
                .iter()
                .filter(|a| a.challenge_id == row.id)
                .cloned()
                .collect();
            let solves = totals.get(&row.id).copied().unwrap_or(0);
            admin_out(row, solves, own_hints, own_files)
        })
        .collect();
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct ChallengeBody {
    category_id: i64,
    title: String,
    #[serde(default)]
    description: String,
    scoring: Scoring,
    flag: FlagSpec,
    #[serde(default)]
    flag_prefix: String,
    #[serde(default)]
    flag_suffix: String,
    case_sensitive: Option<bool>,
    max_attempts: Option<i64>,
    release_at: Option<i64>,
    retire_at: Option<i64>,
    #[serde(default)]
    position: i64,
    #[serde(default)]
    requires_instance: bool,
    instance_config: Option<Value>,
}

pub async fn create_challenge(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Json(body): Json<ChallengeBody>,
) -> Result<(StatusCode, Json<AdminChallenge>), ApiError> {
    jeopardy_event(&state, event_id).await?;
    let fields = ChallengeFields {
        category_id: body.category_id,
        title: clean_name(&body.title, MAX_TITLE)?,
        description: body.description.trim().to_owned(),
        scoring: body.scoring,
        flag: body.flag,
        flag_prefix: body.flag_prefix,
        flag_suffix: body.flag_suffix,
        case_sensitive: body.case_sensitive.unwrap_or(true),
        max_attempts: body.max_attempts,
        release_at: body.release_at,
        retire_at: body.retire_at,
        position: body.position,
        requires_instance: body.requires_instance,
        instance_config: body.instance_config,
    };
    fields.validate()?;
    let id = store::challenge_insert(&state.pool, event_id, &fields, state.now()).await?;
    let row = store::challenge_by_id(&state.pool, id).await?;
    Ok((StatusCode::CREATED, Json(one(&state, row).await?)))
}

pub async fn show_challenge(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminChallenge>, ApiError> {
    let row = store::challenge_by_id(&state.pool, id).await?;
    Ok(Json(one(&state, row).await?))
}

#[derive(Deserialize)]
pub struct ChallengePatch {
    category_id: Option<i64>,
    title: Option<String>,
    description: Option<String>,
    scoring: Option<Scoring>,
    flag: Option<FlagSpec>,
    flag_prefix: Option<String>,
    flag_suffix: Option<String>,
    case_sensitive: Option<bool>,
    #[serde(default, deserialize_with = "set_or_clear")]
    max_attempts: Option<Option<i64>>,
    #[serde(default, deserialize_with = "set_or_clear")]
    release_at: Option<Option<i64>>,
    #[serde(default, deserialize_with = "set_or_clear")]
    retire_at: Option<Option<i64>>,
    position: Option<i64>,
    requires_instance: Option<bool>,
    #[serde(default, deserialize_with = "set_or_clear")]
    instance_config: Option<Option<Value>>,
}

pub async fn update_challenge(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ChallengePatch>,
) -> Result<Json<AdminChallenge>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = store::challenge_by_id(&state.pool, id).await?;
    let mut f = ChallengeFields::from(&row);
    f.category_id = body.category_id.unwrap_or(f.category_id);
    if let Some(title) = &body.title {
        f.title = clean_name(title, MAX_TITLE)?;
    }
    if let Some(description) = body.description {
        f.description = description.trim().to_owned();
    }
    f.scoring = body.scoring.unwrap_or(f.scoring);
    f.flag = body.flag.unwrap_or(f.flag);
    f.flag_prefix = body.flag_prefix.unwrap_or(f.flag_prefix);
    f.flag_suffix = body.flag_suffix.unwrap_or(f.flag_suffix);
    f.case_sensitive = body.case_sensitive.unwrap_or(f.case_sensitive);
    f.max_attempts = body.max_attempts.unwrap_or(f.max_attempts);
    f.release_at = body.release_at.unwrap_or(f.release_at);
    f.retire_at = body.retire_at.unwrap_or(f.retire_at);
    f.position = body.position.unwrap_or(f.position);
    f.requires_instance = body.requires_instance.unwrap_or(f.requires_instance);
    f.instance_config = body.instance_config.unwrap_or(f.instance_config);
    f.validate()?;
    store::challenge_update(&state.pool, &row, &f).await?;
    let fresh = store::challenge_by_id(&state.pool, id).await?;
    notify(&state, &fresh).await?;
    Ok(Json(one(&state, fresh).await?))
}

pub async fn delete_challenge(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = store::challenge_by_id(&state.pool, id).await?;
    store::challenge_delete(&state.pool, id).await?;
    notify(&state, &row).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Tells open clients of the event to refresh.
async fn notify(state: &AppState, row: &ChallengeRow) -> Result<(), ApiError> {
    let event = event_of_challenge(state, row).await?;
    state.notify(&event.slug);
    Ok(())
}

async fn move_to(
    state: &AppState,
    id: i64,
    target: &str,
) -> Result<Json<AdminChallenge>, ApiError> {
    let _guard = state.write_lock.lock().await;
    store::challenge_by_id(&state.pool, id).await?;
    store::set_challenge_state(&state.pool, id, target).await?;
    let fresh = store::challenge_by_id(&state.pool, id).await?;
    notify(state, &fresh).await?;
    Ok(Json(one(state, fresh).await?))
}

pub async fn publish(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminChallenge>, ApiError> {
    move_to(&state, id, "published").await
}

pub async fn retire(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminChallenge>, ApiError> {
    move_to(&state, id, "retired").await
}

#[derive(Deserialize)]
pub struct ScheduleBody {
    release_at: Option<i64>,
    retire_at: Option<i64>,
}

pub async fn schedule(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ScheduleBody>,
) -> Result<Json<AdminChallenge>, ApiError> {
    let _guard = state.write_lock.lock().await;
    store::challenge_by_id(&state.pool, id).await?;
    store::set_schedule(&state.pool, id, body.release_at, body.retire_at).await?;
    let fresh = store::challenge_by_id(&state.pool, id).await?;
    notify(&state, &fresh).await?;
    Ok(Json(one(&state, fresh).await?))
}

#[derive(Deserialize)]
pub struct TestBody {
    flag: String,
}

/// Judges a flag against the challenge without recording anything.
pub async fn test_flag(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TestBody>,
) -> Result<Json<Value>, ApiError> {
    let row = store::challenge_by_id(&state.pool, id).await?;
    let verdict = check(&row.flag, &row.format(), row.case_sensitive, &body.flag);
    Ok(Json(json!({ "verdict": verdict_str(verdict) })))
}

// Hints

#[derive(Deserialize)]
pub struct HintBody {
    content: String,
    #[serde(default)]
    cost: i64,
    #[serde(default)]
    position: i64,
}

pub async fn create_hint(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(challenge_id): Path<i64>,
    Json(body): Json<HintBody>,
) -> Result<(StatusCode, Json<HintRow>), ApiError> {
    store::challenge_by_id(&state.pool, challenge_id).await?;
    let content = store::check_hint(&body.content, body.cost)?;
    let id = store::hint_insert(
        &state.pool,
        challenge_id,
        &content,
        body.cost,
        body.position,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(store::hint_by_id(&state.pool, id).await?),
    ))
}

#[derive(Deserialize)]
pub struct HintPatch {
    content: Option<String>,
    cost: Option<i64>,
    position: Option<i64>,
}

pub async fn update_hint(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<HintPatch>,
) -> Result<Json<HintRow>, ApiError> {
    let row = store::hint_by_id(&state.pool, id).await?;
    let cost = body.cost.unwrap_or(row.cost);
    let content = store::check_hint(body.content.as_deref().unwrap_or(&row.content), cost)?;
    store::hint_update(
        &state.pool,
        id,
        &content,
        cost,
        body.position.unwrap_or(row.position),
    )
    .await?;
    Ok(Json(store::hint_by_id(&state.pool, id).await?))
}

pub async fn delete_hint(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    store::hint_by_id(&state.pool, id).await?;
    store::hint_delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// Attachments

#[derive(Deserialize)]
pub struct UploadQuery {
    name: Option<String>,
}

/// Uploads a file as the raw request body (`?name=` and Content-Type say what it is) or as
/// JSON `{name, mime?, data_base64}`. At most 10 MB.
pub async fn upload_attachment(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(challenge_id): Path<i64>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<AttachmentMeta>), ApiError> {
    store::challenge_by_id(&state.pool, challenge_id).await?;
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (name, mime, data) = if content_type.starts_with("application/json") {
        let upload: Base64Upload =
            serde_json::from_slice(&body).map_err(|_| bad("send {name, mime, data_base64}"))?;
        let data = STANDARD
            .decode(upload.data_base64.trim())
            .map_err(|_| bad("data_base64 is not valid base64"))?;
        (upload.name, upload.mime.unwrap_or_default(), data)
    } else {
        (
            query.name.unwrap_or_default(),
            content_type.to_owned(),
            body.to_vec(),
        )
    };
    if data.is_empty() {
        return Err(bad("the file is empty"));
    }
    if data.len() > MAX_ATTACHMENT {
        return Err(bad("attachments are at most 10 MB"));
    }
    let existing = store::attachments_of_challenge(&state.pool, challenge_id).await?;
    if existing.len() >= MAX_ATTACHMENTS_PER_CHALLENGE {
        return Err(ApiError::Conflict(
            "a challenge has at most 20 attachments".into(),
        ));
    }
    let meta = store::attachment_insert(
        &state.pool,
        challenge_id,
        &store::clean_filename(&name),
        &store::clean_mime(&mime),
        &data,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

pub async fn delete_attachment(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    store::attachment_meta(&state.pool, id).await?;
    store::attachment_delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// History, settings, scoreboard

#[derive(Deserialize)]
pub struct HistoryQuery {
    page: Option<i64>,
    per_page: Option<i64>,
    user_id: Option<i64>,
    challenge_id: Option<i64>,
    verdict: Option<String>,
    ip: Option<String>,
}

pub async fn submissions(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<SubmissionPage>, ApiError> {
    submissions_page(&state, event_id, query).await
}

/// Shared by the challenge admin and the monitor, which both read flag submission history.
pub async fn submissions_page(
    state: &AppState,
    event_id: i64,
    query: HistoryQuery,
) -> Result<Json<SubmissionPage>, ApiError> {
    jeopardy_event(state, event_id).await?;
    if let Some(v) = query.verdict.as_deref()
        && !matches!(v, "correct" | "incorrect" | "bad_format")
    {
        return Err(bad("verdict is correct, incorrect or bad_format"));
    }
    let filter = HistoryFilter {
        user_id: query.user_id,
        challenge_id: query.challenge_id,
        verdict: query.verdict,
        ip: query.ip.filter(|ip| !ip.is_empty()),
    };
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, MAX_PER_PAGE);
    Ok(Json(
        jeopardy_store::history(&state.pool, event_id, &filter, page, per_page).await?,
    ))
}

pub async fn get_settings(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
) -> Result<Json<JeopardySettings>, ApiError> {
    jeopardy_event(&state, event_id).await?;
    Ok(Json(jeopardy_store::settings(&state.pool, event_id).await?))
}

pub async fn put_settings(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Json(body): Json<JeopardySettings>,
) -> Result<Json<JeopardySettings>, ApiError> {
    jeopardy_event(&state, event_id).await?;
    body.validate()?;
    jeopardy_store::save_settings(&state.pool, event_id, &body).await?;
    Ok(Json(body))
}

#[derive(Deserialize)]
pub struct ScoreboardBody {
    public: bool,
}

pub async fn set_scoreboard(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Json(body): Json<ScoreboardBody>,
) -> Result<Json<EventOut>, ApiError> {
    let row = jeopardy_event(&state, event_id).await?;
    event_store::set_scoreboard_public(&state.pool, event_id, body.public).await?;
    state.notify(&row.slug);
    let fresh = event_store::by_id(&state.pool, event_id).await?;
    Ok(Json(EventOut::new(fresh, state.now())?))
}
