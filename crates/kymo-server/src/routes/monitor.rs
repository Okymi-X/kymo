//! Monitoring endpoints: the request log, rate rules, bans and the moderation audit trail.
//! Reading needs the monitor permission; changing rules or bans needs the moderation one.

use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use kymo_core::Permission;
use kymo_monitor::{BanKind, Rule, Scope, expiry, parse_subject};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{ClientIp, Require, needs};
use crate::error::ApiError;
use crate::monitor::reload;
use crate::monitor_store::{self as store, LogFilter, NewBan, RuleFields};
use crate::routes::admin_challenges::{HistoryQuery, submissions_page};
use crate::state::AppState;
use crate::{text, users};

const MAX_PER_PAGE: i64 = 100;
const MAX_BAN_SECS: i64 = 31_536_000;

fn bad(message: &str) -> ApiError {
    ApiError::BadRequest(message.into())
}

#[derive(Deserialize)]
pub struct RequestsQuery {
    ip: Option<String>,
    user_id: Option<i64>,
    route: Option<String>,
    status: Option<i64>,
    since: Option<i64>,
    until: Option<i64>,
    page: Option<i64>,
    per_page: Option<i64>,
}

pub async fn requests(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
    Query(q): Query<RequestsQuery>,
) -> Result<Json<store::LogPage>, ApiError> {
    state.monitor().flush().await;
    let filter = LogFilter {
        ip: q.ip.filter(|v| !v.is_empty()),
        user_id: q.user_id,
        route: q.route.filter(|v| !v.is_empty()),
        status: q.status,
        since: q.since,
        until: q.until,
    };
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, MAX_PER_PAGE);
    Ok(Json(
        store::requests(&state.pool, &filter, page, per_page).await?,
    ))
}

#[derive(Deserialize)]
pub struct SummaryQuery {
    minutes: Option<i64>,
}

pub async fn summary(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<store::Summary>, ApiError> {
    state.monitor().flush().await;
    let minutes = q.minutes.unwrap_or(60).clamp(1, 7 * 24 * 60);
    Ok(Json(
        store::summary(&state.pool, state.now() - minutes * 60).await?,
    ))
}

pub async fn submissions(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<crate::jeopardy_store::SubmissionPage>, ApiError> {
    submissions_page(&state, event_id, q).await
}

// ---------- rules ----------

#[derive(Deserialize)]
pub struct RuleBody {
    endpoint: String,
    max_requests: u32,
    window_secs: i64,
    #[serde(default = "default_scope")]
    scope: String,
    strikes_to_ban: Option<u32>,
    #[serde(default)]
    ban_secs: i64,
    #[serde(default = "yes")]
    enabled: bool,
}

fn default_scope() -> String {
    "ip".into()
}

fn yes() -> bool {
    true
}

impl RuleBody {
    fn fields(&self) -> Result<RuleFields<'_>, ApiError> {
        let scope = Scope::parse(&self.scope).ok_or_else(|| bad("scope is ip or user"))?;
        let rule = Rule {
            id: 0,
            endpoint: self.endpoint.trim().to_owned(),
            max_requests: self.max_requests,
            window_secs: self.window_secs,
            scope,
            strikes_to_ban: self.strikes_to_ban,
            ban_secs: self.ban_secs,
        };
        rule.validate()
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        Ok(RuleFields {
            endpoint: self.endpoint.trim(),
            max_requests: self.max_requests,
            window_secs: self.window_secs,
            scope,
            strikes_to_ban: self.strikes_to_ban,
            ban_secs: self.ban_secs,
            enabled: self.enabled,
        })
    }
}

pub async fn list_rules(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "rules": store::rules(&state.pool).await? })))
}

pub async fn create_rule(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Json(body): Json<RuleBody>,
) -> Result<(StatusCode, Json<store::RuleRow>), ApiError> {
    let fields = body.fields()?;
    let id = store::insert_rule(&state.pool, &fields, state.now()).await?;
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "rule_create",
        "rule",
        id,
        fields.endpoint,
    )
    .await?;
    reload(&state).await?;
    Ok((
        StatusCode::CREATED,
        Json(store::rule_by_id(&state.pool, id).await?),
    ))
}

pub async fn update_rule(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RuleBody>,
) -> Result<Json<store::RuleRow>, ApiError> {
    store::rule_by_id(&state.pool, id).await?;
    let fields = body.fields()?;
    store::update_rule(&state.pool, id, &fields).await?;
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "rule_update",
        "rule",
        id,
        fields.endpoint,
    )
    .await?;
    reload(&state).await?;
    Ok(Json(store::rule_by_id(&state.pool, id).await?))
}

pub async fn delete_rule(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let row = store::rule_by_id(&state.pool, id).await?;
    store::delete_rule(&state.pool, id).await?;
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "rule_delete",
        "rule",
        id,
        &row.endpoint,
    )
    .await?;
    reload(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- bans ----------

pub async fn list_bans(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let now = state.now();
    let bans: Vec<Value> = store::bans(&state.pool)
        .await?
        .into_iter()
        .map(|b| {
            let active = b.lifted_at.is_none() && kymo_monitor::is_active(b.expires_at, now);
            let mut value = serde_json::to_value(&b).unwrap_or(Value::Null);
            value["active"] = json!(active);
            value
        })
        .collect();
    Ok(Json(json!({ "bans": bans })))
}

#[derive(Deserialize)]
pub struct BanBody {
    kind: String,
    subject: String,
    reason: String,
    /// Seconds; leave out for a permanent ban.
    duration_secs: Option<i64>,
}

pub async fn create_ban(
    who: Require<needs::ModerateBans>,
    ClientIp(own_ip): ClientIp,
    State(state): State<AppState>,
    Json(body): Json<BanBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let kind = BanKind::parse(&body.kind).ok_or_else(|| bad("kind is ip or user"))?;
    let subject = parse_subject(kind, &body.subject)
        .ok_or_else(|| bad("subject is an IP address, or a user id"))?;
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 200 || reason.chars().any(text::is_hidden_char)
    {
        return Err(bad("give a reason of at most 200 characters"));
    }
    if body
        .duration_secs
        .is_some_and(|d| !(1..=MAX_BAN_SECS).contains(&d))
    {
        return Err(bad(
            "duration_secs is 1 to 31536000, or leave it out for permanent",
        ));
    }
    match kind {
        BanKind::Ip if subject == own_ip.to_string() => {
            return Err(ApiError::Conflict("you cannot ban your own address".into()));
        }
        BanKind::User => {
            let id: i64 = subject.parse().map_err(|_| ApiError::Internal)?;
            if who.0.user_id == Some(id) {
                return Err(ApiError::Conflict("you cannot ban yourself".into()));
            }
            let target = users::by_id(&state.pool, id).await?;
            let role = kymo_core::Role::from_str(&target.role).map_err(|_| ApiError::Internal)?;
            if role.can(Permission::Monitor) {
                return Err(ApiError::Denied(
                    "staff accounts cannot be banned here; change their role under Users first"
                        .into(),
                ));
            }
        }
        BanKind::Ip => {}
    }
    let now = state.now();
    let expires_at = expiry(now, body.duration_secs);
    let id = store::insert_ban(
        &state.pool,
        &NewBan {
            kind,
            subject: &subject,
            reason,
            auto: false,
            created_by: who.0.user_id,
            now,
            expires_at,
        },
    )
    .await?;
    if kind == BanKind::User {
        let user_id: i64 = subject.parse().map_err(|_| ApiError::Internal)?;
        users::moderate(&state.pool, user_id, None, Some(true)).await?;
    }
    store::audit(
        &state.pool,
        now,
        Some(&who.0),
        "ban_create",
        kind.as_str(),
        &subject,
        reason,
    )
    .await?;
    reload(&state).await?;
    if kind == BanKind::User {
        crate::vpn::sync_quiet(&state).await;
    }
    let row = store::ban_by_id(&state.pool, id).await?;
    Ok((StatusCode::CREATED, Json(json!(row))))
}

pub async fn lift_ban(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let ban = store::ban_by_id(&state.pool, id).await?;
    if ban.lifted_at.is_some() {
        return Err(ApiError::Conflict("this ban was already lifted".into()));
    }
    let now = state.now();
    store::lift_ban(&state.pool, id, now, who.0.user_id).await?;
    if ban.kind == "user"
        && let Ok(user_id) = ban.subject.parse::<i64>()
    {
        users::moderate(&state.pool, user_id, None, Some(false)).await?;
    }
    store::audit(
        &state.pool,
        now,
        Some(&who.0),
        "ban_lift",
        &ban.kind,
        &ban.subject,
        "",
    )
    .await?;
    reload(&state).await?;
    if ban.kind == "user" {
        crate::vpn::sync_quiet(&state).await;
    }
    Ok(Json(json!(store::ban_by_id(&state.pool, id).await?)))
}

// ---------- audit ----------

#[derive(Deserialize)]
pub struct AuditQuery {
    action: Option<String>,
    actor: Option<i64>,
    page: Option<i64>,
    per_page: Option<i64>,
}

pub async fn audit(
    _: Require<needs::ViewAudit>,
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<store::AuditPage>, ApiError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, MAX_PER_PAGE);
    Ok(Json(
        store::audit_page(
            &state.pool,
            q.action.as_deref().filter(|a| !a.is_empty()),
            q.actor,
            page,
            per_page,
        )
        .await?,
    ))
}
