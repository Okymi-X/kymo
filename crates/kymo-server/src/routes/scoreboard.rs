//! Scoreboard and progress views. Ranking and values come from kymo-jeopardy.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, Query, State};
use kymo_core::{Permission, Role, scoreboard_visible};
use kymo_jeopardy::{Challenge, Solve, rank, team_breakdown};
use serde::{Deserialize, Serialize};

use super::challenges::solve_points;
use crate::auth::{MaybeCaller, Player};
use crate::challenge_store::{self as store, ChallengeRow};
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::jeopardy_store::{self, Board, Scorer};
use crate::state::AppState;
use crate::submit::{play_context, released};
use crate::team_store;

#[derive(Deserialize)]
pub struct BoardQuery {
    matrix: Option<bool>,
}

#[derive(Serialize)]
pub struct BoardRow {
    rank: u32,
    participant_id: i64,
    kind: &'static str,
    name: String,
    avatar_seed: String,
    points: i64,
    solves: u32,
    last_solve_at: Option<i64>,
    solved: Option<Vec<i64>>,
}

#[derive(Serialize)]
pub struct MatrixChallenge {
    id: i64,
    title: String,
    category_id: i64,
}

#[derive(Serialize)]
pub struct ScoreboardOut {
    event_id: i64,
    mode: String,
    public: bool,
    rows: Vec<BoardRow>,
    challenges: Option<Vec<MatrixChallenge>>,
}

fn kind_of(event: &EventRow) -> &'static str {
    if event.mode == "team" { "team" } else { "user" }
}

async fn jeopardy_by_slug(state: &AppState, slug: &str) -> Result<EventRow, ApiError> {
    let event = event_store::by_slug(&state.pool, slug).await?;
    if event.kind != "jeopardy" {
        return Err(ApiError::NotFound);
    }
    Ok(event)
}

pub async fn scoreboard(
    MaybeCaller(caller): MaybeCaller,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<BoardQuery>,
) -> Result<Json<ScoreboardOut>, ApiError> {
    let event = jeopardy_by_slug(&state, &slug).await?;
    let role = caller.map_or(Role::Visitor, |c| c.role);
    if event.state == "draft" && !role.can(Permission::ManageChallenges) {
        return Err(ApiError::NotFound);
    }
    if !scoreboard_visible(event.scoreboard_public, role) {
        let registered = match caller.and_then(|c| c.user_id) {
            Some(user_id) => jeopardy_store::scorer_of(&state.pool, &event, user_id)
                .await
                .is_ok(),
            None => false,
        };
        if !registered && !role.can(Permission::ManageChallenges) {
            return Err(ApiError::Denied("the scoreboard is not public yet".into()));
        }
    }
    let board = Board::load(&state.pool, &event).await?;
    let settings = jeopardy_store::settings(&state.pool, event.id).await?;
    let ranked = rank(
        &settings.board_config(),
        &board.participants,
        &board.challenges,
        &board.solves,
    );
    let matrix = query.matrix.unwrap_or(false);
    let kind = kind_of(&event);
    let rows = ranked
        .into_iter()
        .map(|row| {
            let name = board
                .names
                .get(&row.participant)
                .cloned()
                .unwrap_or_default();
            let id = i64::try_from(row.participant).unwrap_or_default();
            BoardRow {
                rank: row.rank,
                participant_id: id,
                kind,
                avatar_seed: name.clone(),
                name,
                points: row.points,
                solves: row.solves,
                last_solve_at: row.last_solve_at,
                solved: matrix.then(|| board.solves_of(id).map(|s| s.challenge_id).collect()),
            }
        })
        .collect();
    let challenges = if matrix {
        let now = state.now();
        Some(
            store::challenges(&state.pool, event.id)
                .await?
                .into_iter()
                .filter(|c| c.state != "draft" && released(c, now).is_ok())
                .map(|c| MatrixChallenge {
                    id: c.id,
                    title: c.title,
                    category_id: c.category_id,
                })
                .collect(),
        )
    } else {
        None
    };
    Ok(Json(ScoreboardOut {
        event_id: event.id,
        mode: event.mode,
        public: event.scoreboard_public,
        rows,
        challenges,
    }))
}

#[derive(Serialize)]
pub struct SolveOut {
    challenge_id: i64,
    title: String,
    at: i64,
    hints_cost: i64,
    points: i64,
}

#[derive(Serialize)]
pub struct ProgressOut {
    participant_id: i64,
    kind: &'static str,
    name: String,
    rank: Option<u32>,
    points: i64,
    solved_count: usize,
    total_challenges: usize,
    solves: Vec<SolveOut>,
}

fn titles(rows: &[ChallengeRow]) -> HashMap<i64, &ChallengeRow> {
    rows.iter().map(|c| (c.id, c)).collect()
}

pub async fn progress(
    player: Player,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ProgressOut>, ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    let board = Board::load(&state.pool, &play.event).await?;
    let settings = jeopardy_store::settings(&state.pool, play.event.id).await?;
    let ranked = rank(
        &settings.board_config(),
        &board.participants,
        &board.challenges,
        &board.solves,
    );
    let me = u64::try_from(play.scorer.id()).unwrap_or_default();
    let own = ranked.iter().find(|r| r.participant == me);
    let counts = board.counts();
    let all = store::challenges(&state.pool, play.event.id).await?;
    let by_id = titles(&all);
    let now = state.now();
    let solves: Vec<SolveOut> = board
        .solves_of(play.scorer.id())
        .filter_map(|s| {
            let ch = by_id.get(&s.challenge_id)?;
            let n = counts.get(&(s.challenge_id as u64)).copied().unwrap_or(0);
            Some(SolveOut {
                challenge_id: s.challenge_id,
                title: ch.title.clone(),
                at: s.at,
                hints_cost: s.hints_cost,
                points: solve_points(&ch.scoring, n, s.hints_cost),
            })
        })
        .collect();
    let total = all
        .iter()
        .filter(|c| c.state != "draft" && released(c, now).is_ok())
        .count();
    Ok(Json(ProgressOut {
        participant_id: play.scorer.id(),
        kind: play.scorer.kind(),
        name: board.names.get(&me).cloned().unwrap_or_default(),
        rank: own.map(|r| r.rank),
        points: own.map_or(0, |r| r.points),
        solved_count: solves.len(),
        total_challenges: total,
        solves,
    }))
}

#[derive(Deserialize)]
pub struct TeamQuery {
    event: String,
}

#[derive(Serialize)]
pub struct MemberOut {
    user_id: i64,
    username: String,
    display_name: String,
    points: i64,
    solves: Vec<MemberSolveOut>,
}

#[derive(Serialize)]
pub struct MemberSolveOut {
    challenge_id: i64,
    title: String,
    at: i64,
    points: i64,
}

#[derive(Serialize)]
pub struct TeamProgressOut {
    team: ProgressOut,
    members: Vec<MemberOut>,
}

/// Team total and each teammate's solves. Members only; everyone else gets 404.
pub async fn team_progress(
    player: Player,
    State(state): State<AppState>,
    Path(team_id): Path<i64>,
    Query(query): Query<TeamQuery>,
) -> Result<Json<TeamProgressOut>, ApiError> {
    if !team_store::is_member(&state.pool, team_id, player.id).await? {
        return Err(ApiError::NotFound);
    }
    let play = play_context(&state, &query.event, player.id)
        .await
        .map_err(|_| ApiError::NotFound)?;
    if play.scorer != Scorer::Team(team_id) {
        return Err(ApiError::NotFound);
    }
    let viewer = player.id;
    let Json(team) = progress(player, State(state.clone()), Path(query.event.clone())).await?;
    let board = Board::load(&state.pool, &play.event).await?;
    let all = store::challenges(&state.pool, play.event.id).await?;
    let by_id = titles(&all);
    let challenges: Vec<Challenge> = board.challenges.clone();
    let row = team_store::by_id(&state.pool, team_id).await?;
    let view = team_store::view(&state.pool, row, viewer).await?;
    let member_ids: Vec<u64> = view
        .members
        .iter()
        .filter_map(|m| u64::try_from(m.user_id).ok())
        .collect();
    let member_solves: Vec<Solve> = board
        .solves_of(team_id)
        .map(|s| Solve {
            participant: s.user_id as u64,
            ..s.to_solve()
        })
        .collect();
    let breakdown = team_breakdown(&member_ids, &challenges, &member_solves, &board.counts());
    let names: BTreeMap<i64, (&str, &str)> = view
        .members
        .iter()
        .map(|m| (m.user_id, (m.username.as_str(), m.display_name.as_str())))
        .collect();
    let members = breakdown
        .into_iter()
        .map(|b| {
            let user_id = i64::try_from(b.participant).unwrap_or_default();
            let (username, display_name) = names.get(&user_id).copied().unwrap_or(("", ""));
            MemberOut {
                user_id,
                username: username.to_owned(),
                display_name: display_name.to_owned(),
                points: b.points,
                solves: b
                    .solves
                    .into_iter()
                    .map(|s| {
                        let challenge_id = i64::try_from(s.challenge).unwrap_or_default();
                        MemberSolveOut {
                            title: by_id
                                .get(&challenge_id)
                                .map(|c| c.title.clone())
                                .unwrap_or_default(),
                            challenge_id,
                            at: s.at,
                            points: s.points,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Json(TeamProgressOut { team, members }))
}
