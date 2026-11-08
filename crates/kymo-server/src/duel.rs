//! Challenge-based tournament matches: who plays a match, what it asks, how an answer is judged
//! and when the match turns into a bracket result. The rules are `kymo-duel`'s; this module
//! loads data, applies them and writes the result through the engine like a typed score.

use std::collections::{HashMap, HashSet};

use kymo_core::can_submit;
use kymo_duel::{Assignment, Award, Open, Outcome, draw};
use kymo_engine::{Bracket, Side, Stage};
use kymo_jeopardy::{FlagFormat, FlagSpec, RateRule, Verdict, check, current_value};
use serde::Serialize;

use crate::challenge_store::{self as store, ChallengeRow};
use crate::db::{self, Tournament};
use crate::duel_store::{self as dstore, AnswerRow, ItemRow, NewAnswer};
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::monitor_store;
use crate::state::AppState;
use crate::team_store;

const MAX_ANSWER: usize = 500;
/// Wrong answers allowed per side in a short window, on top of each question's own limit.
const RATE: RateRule = RateRule {
    max_attempts: 12,
    window_secs: 30,
    lockout_secs: 0,
};

/// One thing a side can answer: a flag challenge, or one question of a DFIR challenge.
#[derive(Debug, Clone)]
pub struct Unit {
    pub challenge_id: i64,
    pub item_id: Option<i64>,
    pub points: u32,
    pub max_attempts: Option<u32>,
}

fn points_of(row: &ChallengeRow) -> u32 {
    u32::try_from(current_value(&row.scoring, 0)).unwrap_or(0)
}

fn cap(value: Option<i64>) -> Option<u32> {
    value.and_then(|m| u32::try_from(m).ok())
}

pub fn side_ids(bracket: &Bracket, match_id: usize) -> Option<[i64; 2]> {
    let m = bracket.matches.get(match_id)?;
    match m.slots {
        [kymo_engine::Slot::Team(a), kymo_engine::Slot::Team(b)] => {
            Some([i64::from(a), i64::from(b)])
        }
        _ => None,
    }
}

/// How many challenges one match carries in this event: the solo or the team setting.
pub fn expected_count(t: &Tournament, event: &EventRow) -> usize {
    usize::from(if event.mode == "solo" {
        t.challenges.solo
    } else {
        t.challenges.team
    })
}

async fn rows_in_order(state: &AppState, ids: &[i64]) -> Result<Vec<ChallengeRow>, ApiError> {
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        rows.push(store::challenge_by_id(&state.pool, *id).await?);
    }
    Ok(rows)
}

async fn units_of(state: &AppState, rows: &[ChallengeRow]) -> Result<Vec<Unit>, ApiError> {
    let mut units = Vec::new();
    for row in rows {
        if row.kind == "dfir" {
            for item in dstore::items(&state.pool, row.id).await? {
                units.push(Unit {
                    challenge_id: row.id,
                    item_id: Some(item.id),
                    points: u32::try_from(item.points).unwrap_or(0),
                    max_attempts: cap(item.max_attempts),
                });
            }
        } else {
            units.push(Unit {
                challenge_id: row.id,
                item_id: None,
                points: points_of(row),
                max_attempts: cap(row.max_attempts),
            });
        }
    }
    Ok(units)
}

/// Everything the rules need to judge one match.
pub struct Eval {
    pub teams: [i64; 2],
    pub rows: Vec<ChallengeRow>,
    pub units: Vec<Unit>,
    pub answers: Vec<AnswerRow>,
}

impl Eval {
    fn side_of_team(&self, team_id: i64) -> Option<usize> {
        self.teams.iter().position(|t| *t == team_id)
    }

    fn correct(&self, side: usize, unit: &Unit) -> bool {
        self.answers.iter().any(|a| {
            a.verdict == "correct"
                && a.side_team_id == self.teams[side]
                && a.challenge_id == unit.challenge_id
                && a.item_id == unit.item_id
        })
    }

    fn wrong(&self, side: usize, unit: &Unit) -> u32 {
        let n = self
            .answers
            .iter()
            .filter(|a| {
                a.verdict == "incorrect"
                    && a.side_team_id == self.teams[side]
                    && a.challenge_id == unit.challenge_id
                    && a.item_id == unit.item_id
            })
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    pub fn attempts_left(&self, side: usize, unit: &Unit) -> Option<u32> {
        unit.max_attempts
            .map(|max| max.saturating_sub(self.wrong(side, unit)))
    }

    fn earned_in(&self, side: usize, challenge_id: i64) -> u32 {
        self.answers
            .iter()
            .filter(|a| {
                a.verdict == "correct"
                    && a.side_team_id == self.teams[side]
                    && a.challenge_id == challenge_id
            })
            .map(|a| u32::try_from(a.points).unwrap_or(0))
            .sum()
    }

    fn awards(&self) -> Vec<Award> {
        self.answers
            .iter()
            .filter(|a| a.verdict == "correct")
            .filter_map(|a| {
                Some(Award {
                    side: self.side_of_team(a.side_team_id)?,
                    points: u32::try_from(a.points).unwrap_or(0),
                    at: a.at,
                })
            })
            .collect()
    }

    fn open(&self) -> [Open; 2] {
        let count = |side: usize| Open {
            unsettled: self
                .units
                .iter()
                .filter(|u| !self.correct(side, u) && self.attempts_left(side, u) != Some(0))
                .count(),
        };
        [count(0), count(1)]
    }
}

pub async fn eval(state: &AppState, t: &Tournament, match_id: usize) -> Result<Eval, ApiError> {
    let teams = t
        .bracket
        .as_ref()
        .and_then(|b| side_ids(b, match_id))
        .ok_or_else(|| ApiError::Conflict("this match does not have both teams yet".into()))?;
    let ids = dstore::assigned(&state.pool, t.id, match_id as i64).await?;
    let rows = rows_in_order(state, &ids).await?;
    let units = units_of(state, &rows).await?;
    let answers = dstore::answers(&state.pool, t.id, match_id as i64).await?;
    Ok(Eval {
        teams,
        rows,
        units,
        answers,
    })
}

fn time_up(t: &Tournament, opened_at: Option<i64>, now: i64) -> bool {
    matches!((t.challenges.time_limit, opened_at), (Some(limit), Some(at)) if now >= at + limit)
}

/// Which side of a match this account plays for, if either.
async fn side_of(
    state: &AppState,
    t: &Tournament,
    event: &EventRow,
    teams: [i64; 2],
    user_id: i64,
) -> Result<Option<usize>, ApiError> {
    let player_team = if event.mode == "solo" {
        None
    } else {
        team_store::of_user(&state.pool, user_id)
            .await?
            .map(|team| team.id)
    };
    for (index, id) in teams.iter().enumerate() {
        let Some(team) = t.teams.iter().find(|team| team.id == *id) else {
            continue;
        };
        let yours = if event.mode == "solo" {
            team.user_id == Some(user_id)
        } else {
            player_team.is_some() && team.player_team_id == player_team
        };
        if yours {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// The challenges of a match, drawing them first when the assignment mode asks for it.
pub async fn ensure_assigned(
    state: &AppState,
    t: &Tournament,
    event: &EventRow,
    match_id: usize,
) -> Result<Vec<i64>, ApiError> {
    let existing = dstore::assigned(&state.pool, t.id, match_id as i64).await?;
    let mode = t.challenges.assignment;
    if !existing.is_empty() || !matches!(mode, Assignment::PerMatch | Assignment::PerRound) {
        return Ok(existing);
    }
    let Some(bracket) = &t.bracket else {
        return Ok(existing);
    };
    let _guard = state.write_lock.lock().await;
    let existing = dstore::assigned(&state.pool, t.id, match_id as i64).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    let mut pool = Vec::new();
    for row in store::challenges(&state.pool, event.id).await? {
        if row.state != "published" {
            continue;
        }
        if row.kind == "dfir" && dstore::items(&state.pool, row.id).await?.is_empty() {
            continue;
        }
        pool.push(row.id);
    }
    let all = dstore::all_assigned(&state.pool, t.id).await?;
    let me = &bracket.matches[match_id];
    let siblings: Vec<i64> = bracket
        .matches
        .iter()
        .filter(|m| m.stage == me.stage && m.round == me.round && m.id != me.id)
        .map(|m| m.id as i64)
        .collect();
    let (picked, key) = if mode == Assignment::PerRound {
        if let Some((_, _)) = all.iter().find(|(m, _)| siblings.contains(m)) {
            let first = all
                .iter()
                .find(|(m, _)| siblings.contains(m))
                .map(|(m, _)| *m);
            let copied: Vec<i64> = all
                .iter()
                .filter(|(m, _)| Some(*m) == first)
                .map(|(_, c)| *c)
                .collect();
            dstore::assign(&state.pool, t.id, match_id as i64, &copied).await?;
            return Ok(copied);
        }
        let seen: HashSet<i64> = all.iter().map(|(_, c)| *c).collect();
        (seen, ((me.stage as u64) << 32) | u64::from(me.round))
    } else {
        let mine: HashSet<i64> = [me.slots[0], me.slots[1]]
            .iter()
            .filter_map(|s| match s {
                kymo_engine::Slot::Team(id) => Some(i64::from(*id)),
                _ => None,
            })
            .collect();
        let their_matches: HashSet<i64> = bracket
            .matches
            .iter()
            .filter(|m| {
                m.id != me.id
                    && m.slots.iter().any(|s| matches!(s, kymo_engine::Slot::Team(id) if mine.contains(&i64::from(*id))))
            })
            .map(|m| m.id as i64)
            .collect();
        let seen: HashSet<i64> = all
            .iter()
            .filter(|(m, _)| their_matches.contains(m))
            .map(|(_, c)| *c)
            .collect();
        (seen, match_id as u64)
    };
    let seed = u64::try_from(t.challenges.seed).unwrap_or(0);
    let chosen = draw(&pool, expected_count(t, event), seed, key, &picked);
    dstore::assign(&state.pool, t.id, match_id as i64, &chosen).await?;
    Ok(chosen)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Finish {
    NotYet,
    NeedsDecision,
    Done,
}

/// Turns the match into a bracket result when the rules say it is over (or `close` forces it).
/// `winner` decides a tie that nothing else can break. Safe to call from anywhere, any number
/// of times: only the first call that finds the match over writes the result.
pub async fn settle(
    state: &AppState,
    tournament_id: i64,
    match_id: usize,
    close: bool,
    winner: Option<Side>,
) -> Result<Finish, ApiError> {
    let t = db::get(&state.pool, tournament_id).await?;
    if t.challenges.assignment == Assignment::None {
        return Ok(Finish::NotYet);
    }
    let Some(bracket) = &t.bracket else {
        return Ok(Finish::NotYet);
    };
    let Some(m) = bracket.matches.get(match_id) else {
        return Ok(Finish::NotYet);
    };
    if m.is_played() || !m.is_ready() {
        return Ok(Finish::NotYet);
    }
    let mstate = dstore::match_state(&state.pool, t.id, match_id as i64).await?;
    if mstate.is_some_and(|s| s.finalized_at.is_some()) {
        return Ok(Finish::NotYet);
    }
    let ev = eval(state, &t, match_id).await?;
    if ev.units.is_empty() {
        return Ok(Finish::NotYet);
    }
    let closed = close || time_up(&t, mstate.and_then(|s| s.opened_at), state.now());
    let allows_draw = m.stage == Stage::League;
    let (score, tiebreak) = match kymo_duel::decide(&ev.awards(), ev.open(), closed, allows_draw) {
        Outcome::Open => return Ok(Finish::NotYet),
        Outcome::Final { score, tiebreak } => (score, tiebreak.map(side_of_index)),
        Outcome::NeedsDecision { score } => match winner {
            Some(side) => (score, Some(side)),
            None => return Ok(Finish::NeedsDecision),
        },
    };
    if !dstore::claim_final(&state.pool, t.id, match_id as i64, state.now()).await? {
        return Ok(Finish::NotYet);
    }
    let applied =
        crate::routes::admin::edit_bracket(state, t.id, match_id, "result", |b| match tiebreak {
            Some(side) => b.set_result_tiebreak(match_id, score[0], score[1], side),
            None => b.set_result(match_id, score[0], score[1]),
        })
        .await;
    if let Err(e) = applied {
        dstore::release_final(&state.pool, t.id, match_id as i64).await?;
        return Err(e);
    }
    monitor_store::audit(
        &state.pool,
        state.now(),
        None,
        "duel_finalized",
        "match",
        format!("{}:{}", t.id, match_id),
        &format!("score {}-{}", score[0], score[1]),
    )
    .await?;
    Ok(Finish::Done)
}

fn side_of_index(i: usize) -> Side {
    if i == 0 { Side::A } else { Side::B }
}

/// Finishes every open match whose time limit has passed. Run by the background sweep.
pub async fn sweep(state: &AppState) -> Result<(), ApiError> {
    for (tournament_id, match_id) in dstore::expired(&state.pool, state.now()).await? {
        if let Err(e) = settle(state, tournament_id, match_id as usize, false, None).await {
            tracing::warn!(
                "could not finish match {match_id} of tournament {tournament_id}: {e:?}"
            );
        }
    }
    Ok(())
}

// What a player or an organizer sees of a match.

#[derive(Serialize)]
pub struct SideOut {
    pub id: i64,
    pub name: String,
    pub score: u32,
}

#[derive(Serialize)]
pub struct ItemView {
    pub id: i64,
    pub question: String,
    pub points: u32,
    pub solved: bool,
    pub attempts_left: Option<u32>,
}

#[derive(Serialize)]
pub struct AttachmentOut {
    pub id: i64,
    pub name: String,
    pub size: i64,
}

#[derive(Serialize)]
pub struct FormatOut {
    pub prefix: String,
    pub suffix: String,
}

#[derive(Serialize)]
pub struct ChallengeView {
    pub id: i64,
    pub title: String,
    pub category: String,
    pub description: String,
    /// The accepted shape of a flag, for the answer box. None for DFIR questions.
    pub flag_format: Option<FormatOut>,
    pub attachments: Vec<AttachmentOut>,
    /// `flag`, `instance` or `dfir`.
    pub kind: &'static str,
    pub points: u32,
    /// Points each side has earned on this challenge.
    pub earned: [u32; 2],
    pub solved: bool,
    pub attempts_left: Option<u32>,
    pub items: Vec<ItemView>,
}

#[derive(Serialize)]
pub struct MatchView {
    pub match_id: usize,
    /// `waiting` (no challenges or teams yet), `open`, `needs_decision` or `final`.
    pub status: &'static str,
    pub closes_at: Option<i64>,
    /// 0 or 1 when the viewer plays in this match.
    pub you: Option<usize>,
    pub sides: [SideOut; 2],
    pub challenges: Vec<ChallengeView>,
    /// Every judged answer; staff only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answers: Option<Vec<AnswerRow>>,
}

/// Looks up the tournament, its event and checks the match exists.
pub async fn context(
    state: &AppState,
    slug: &str,
    match_id: usize,
) -> Result<(Tournament, EventRow), ApiError> {
    let t = db::get_by_slug(&state.pool, slug).await?;
    if t.challenges.assignment == Assignment::None {
        return Err(ApiError::NotFound);
    }
    let event_id = t.event_id.ok_or(ApiError::NotFound)?;
    let event = event_store::by_id(&state.pool, event_id).await?;
    let ready = t
        .bracket
        .as_ref()
        .is_some_and(|b| b.matches.get(match_id).is_some_and(|m| m.is_ready()));
    if !ready {
        return Err(ApiError::NotFound);
    }
    Ok((t, event))
}

/// The viewer's side of a match, `None` for staff or anyone not playing.
pub async fn viewer_side(
    state: &AppState,
    t: &Tournament,
    event: &EventRow,
    match_id: usize,
    user_id: Option<i64>,
) -> Result<Option<usize>, ApiError> {
    let (Some(user_id), Some(teams)) = (
        user_id,
        t.bracket.as_ref().and_then(|b| side_ids(b, match_id)),
    ) else {
        return Ok(None);
    };
    side_of(state, t, event, teams, user_id).await
}

pub async fn view(
    state: &AppState,
    t: &Tournament,
    event: &EventRow,
    match_id: usize,
    you: Option<usize>,
    staff: bool,
) -> Result<MatchView, ApiError> {
    ensure_assigned(state, t, event, match_id).await?;
    let ev = eval(state, t, match_id).await?;
    let mstate = dstore::match_state(&state.pool, t.id, match_id as i64).await?;
    let played = t
        .bracket
        .as_ref()
        .is_some_and(|b| b.matches[match_id].is_played());
    let closed = time_up(t, mstate.and_then(|s| s.opened_at), state.now());
    let status = if ev.units.is_empty() {
        "waiting"
    } else if played || mstate.is_some_and(|s| s.finalized_at.is_some()) {
        "final"
    } else if closed || ev.open().iter().all(|o| o.unsettled == 0) {
        "needs_decision"
    } else {
        "open"
    };
    let scores = kymo_duel::scores(&ev.awards());
    let name = |id: i64| {
        t.teams
            .iter()
            .find(|team| team.id == id)
            .map(|team| team.name.clone())
            .unwrap_or_default()
    };
    let categories: HashMap<i64, String> = store::categories(&state.pool, event.id)
        .await?
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    let files = store::attachments_of_event(&state.pool, event.id).await?;
    let mut challenges = Vec::new();
    for row in &ev.rows {
        let units: Vec<&Unit> = ev
            .units
            .iter()
            .filter(|u| u.challenge_id == row.id)
            .collect();
        let me = you.unwrap_or(0);
        let mut items = Vec::new();
        if row.kind == "dfir" {
            for item in dstore::items(&state.pool, row.id).await? {
                let unit = units.iter().find(|u| u.item_id == Some(item.id));
                items.push(ItemView {
                    id: item.id,
                    question: item.question.clone(),
                    points: u32::try_from(item.points).unwrap_or(0),
                    solved: you.is_some() && unit.is_some_and(|u| ev.correct(me, u)),
                    attempts_left: you.and_then(|_| unit.and_then(|u| ev.attempts_left(me, u))),
                });
            }
        }
        let flag_unit = units.iter().find(|u| u.item_id.is_none());
        challenges.push(ChallengeView {
            id: row.id,
            title: row.title.clone(),
            category: categories
                .get(&row.category_id)
                .cloned()
                .unwrap_or_default(),
            description: row.description.clone(),
            attachments: files
                .iter()
                .filter(|a| a.challenge_id == row.id)
                .map(|a| AttachmentOut {
                    id: a.id,
                    name: a.name.clone(),
                    size: a.size,
                })
                .collect(),
            flag_format: (row.kind != "dfir"
                && (!row.flag_prefix.is_empty() || !row.flag_suffix.is_empty()))
            .then(|| FormatOut {
                prefix: row.flag_prefix.clone(),
                suffix: row.flag_suffix.clone(),
            }),
            kind: if row.kind == "dfir" {
                "dfir"
            } else if row.requires_instance {
                "instance"
            } else {
                "flag"
            },
            points: points_of(row),
            earned: [ev.earned_in(0, row.id), ev.earned_in(1, row.id)],
            solved: you.is_some() && flag_unit.is_some_and(|u| ev.correct(me, u)),
            attempts_left: you.and_then(|_| flag_unit.and_then(|u| ev.attempts_left(me, u))),
            items,
        });
    }
    Ok(MatchView {
        match_id,
        status,
        closes_at: match (t.challenges.time_limit, mstate.and_then(|s| s.opened_at)) {
            (Some(limit), Some(at)) => Some(at + limit),
            _ => None,
        },
        you,
        sides: [
            SideOut {
                id: ev.teams[0],
                name: name(ev.teams[0]),
                score: scores[0],
            },
            SideOut {
                id: ev.teams[1],
                name: name(ev.teams[1]),
                score: scores[1],
            },
        ],
        challenges,
        answers: staff.then(|| ev.answers.clone()),
    })
}

pub struct Submission<'a> {
    pub user_id: i64,
    pub ip: &'a str,
    pub challenge_id: i64,
    pub item_id: Option<i64>,
    pub value: &'a str,
}

#[derive(Serialize)]
pub struct SubmitOut {
    pub correct: bool,
    pub points: u32,
    pub attempts_left: Option<u32>,
    /// True when this answer ended the match.
    pub finished: bool,
}

pub async fn submit(
    state: &AppState,
    slug: &str,
    match_id: usize,
    sub: &Submission<'_>,
) -> Result<SubmitOut, ApiError> {
    if sub.value.chars().count() > MAX_ANSWER {
        return Err(ApiError::BadRequest("that answer is too long".into()));
    }
    let (t, event) = context(state, slug, match_id).await?;
    let now = state.now();
    can_submit(event.effective_state(now)?, None, now)
        .map_err(|denied| ApiError::Conflict(denied.message().into()))?;
    let Some(side) = viewer_side(state, &t, &event, match_id, Some(sub.user_id)).await? else {
        return Err(ApiError::Denied("you are not playing in this match".into()));
    };
    ensure_assigned(state, &t, &event, match_id).await?;

    let guard = state.write_lock.lock().await;
    let ev = eval(state, &t, match_id).await?;
    let mstate = dstore::match_state(&state.pool, t.id, match_id as i64).await?;
    if t.bracket
        .as_ref()
        .is_some_and(|b| b.matches[match_id].is_played())
        || mstate.is_some_and(|s| s.finalized_at.is_some())
    {
        return Err(ApiError::Conflict("this match is over".into()));
    }
    if time_up(&t, mstate.and_then(|s| s.opened_at), now) {
        drop(guard);
        settle(state, t.id, match_id, false, None).await?;
        return Err(ApiError::Conflict("time is up for this match".into()));
    }
    let row = ev
        .rows
        .iter()
        .find(|r| r.id == sub.challenge_id)
        .ok_or(ApiError::NotFound)?;
    let unit = ev
        .units
        .iter()
        .find(|u| u.challenge_id == row.id && u.item_id == sub.item_id)
        .ok_or_else(|| ApiError::BadRequest("answer one of the listed questions".into()))?;
    if ev.correct(side, unit) {
        return Err(ApiError::Conflict("already answered correctly".into()));
    }
    if ev.attempts_left(side, unit) == Some(0) {
        return Err(ApiError::Conflict(
            "no attempts left for this question".into(),
        ));
    }
    let (spec, format, case_sensitive): (FlagSpec, FlagFormat, bool) = match sub.item_id {
        Some(item_id) => {
            let item: ItemRow = dstore::items(&state.pool, row.id)
                .await?
                .into_iter()
                .find(|i| i.id == item_id)
                .ok_or(ApiError::NotFound)?;
            (item.answer, FlagFormat::default(), item.case_sensitive)
        }
        None => (row.flag.clone(), row.format(), row.case_sensitive),
    };
    let verdict = check(&spec, &format, case_sensitive, sub.value);
    if verdict == Verdict::BadFormat {
        return Err(ApiError::BadRequest(
            "that answer is not in the expected format".into(),
        ));
    }
    let team_id = ev.teams[side];
    state
        .rate_check(event.id, team_id, &RATE)
        .map_err(|denied| ApiError::RateLimited {
            retry_after: denied.retry_after_secs,
        })?;
    let correct = verdict == Verdict::Correct;
    // Partial credit never goes above the challenge's total.
    let remaining = points_of(row).saturating_sub(ev.earned_in(side, row.id));
    let points = if correct {
        unit.points.min(remaining)
    } else {
        0
    };
    if mstate.and_then(|s| s.opened_at).is_none() {
        dstore::open_match(&state.pool, t.id, match_id as i64, now).await?;
    }
    dstore::insert_answer(
        &state.pool,
        &NewAnswer {
            tournament_id: t.id,
            match_id: match_id as i64,
            challenge_id: row.id,
            item_id: sub.item_id,
            side_team_id: team_id,
            user_id: sub.user_id,
            correct,
            points: i64::from(points),
            value: sub.value.trim(),
            ip: sub.ip,
            at: now,
        },
    )
    .await?;
    drop(guard);
    state.notify(&t.slug);
    let finished = settle(state, t.id, match_id, false, None).await? == Finish::Done;
    let attempts_left = unit.max_attempts.map(|max| {
        let used = ev.wrong(side, unit) + u32::from(!correct);
        max.saturating_sub(used)
    });
    Ok(SubmitOut {
        correct,
        points,
        attempts_left,
        finished,
    })
}

/// Fills a draft tournament's roster from the event's registrations and links each team to its
/// account or player team. Replaces the current roster.
pub async fn fill_roster(state: &AppState, tournament_id: i64) -> Result<Tournament, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, tournament_id).await?;
    if t.status != db::Status::Draft {
        return Err(ApiError::Conflict(
            "the roster can only be filled before the tournament starts".into(),
        ));
    }
    let event_id = t
        .event_id
        .ok_or_else(|| ApiError::Conflict("this tournament has no event".into()))?;
    let max_teams = if matches!(
        t.format,
        kymo_engine::Format::RoundRobin | kymo_engine::Format::GroupKnockout
    ) {
        kymo_engine::MAX_ROUND_ROBIN_TEAMS
    } else {
        kymo_engine::MAX_TEAMS
    };
    let people = event_store::participants(&state.pool, event_id).await?;
    if people.len() > max_teams {
        return Err(ApiError::BadRequest(format!(
            "this format takes at most {max_teams} teams and {} are registered",
            people.len()
        )));
    }
    let mut seen = HashMap::new();
    let mut roster = Vec::with_capacity(people.len());
    for p in &people {
        let mut name = if p.name.trim().is_empty() {
            format!("Player {}", p.id)
        } else {
            p.name.trim().to_owned()
        };
        let n = seen.entry(name.to_lowercase()).or_insert(0u32);
        *n += 1;
        if *n > 1 {
            name = format!("{name} ({n})");
        }
        roster.push(db::NewTeam {
            name,
            score: 0,
            user_id: p.user_id,
            player_team_id: p.team_id,
        });
    }
    db::replace_teams(&state.pool, tournament_id, &roster).await?;
    state.notify(&t.slug);
    db::get(&state.pool, tournament_id).await
}

/// In a tournament event a challenge is only for the players of a match it is assigned to: an
/// instance or a file of any other challenge is not theirs to open. With `unsolved` the side must
/// also still have something to do on it (no new instance after a solve). Jeopardy events pass.
pub async fn player_gate(
    state: &AppState,
    event: &EventRow,
    challenge_id: i64,
    user_id: i64,
    unsolved: bool,
) -> Result<(), ApiError> {
    if event.kind != "tournament" {
        return Ok(());
    }
    let not_yours =
        || ApiError::Denied("this challenge is not part of a match you are playing".into());
    let tournament_id: i64 = sqlx::query_scalar("SELECT id FROM tournament WHERE event_id = ?")
        .bind(event.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(not_yours)?;
    let t = db::get(&state.pool, tournament_id).await?;
    let Some(bracket) = &t.bracket else {
        return Err(not_yours());
    };
    for m in bracket
        .matches
        .iter()
        .filter(|m| m.is_ready() && !m.is_played())
    {
        let assigned = dstore::assigned(&state.pool, t.id, m.id as i64).await?;
        if !assigned.contains(&challenge_id) {
            continue;
        }
        let Some(side) = viewer_side(state, &t, event, m.id, Some(user_id)).await? else {
            continue;
        };
        if unsolved {
            let ev = eval(state, &t, m.id).await?;
            let done = ev
                .units
                .iter()
                .filter(|u| u.challenge_id == challenge_id)
                .all(|u| ev.correct(side, u));
            if done {
                return Err(ApiError::Conflict(
                    "your team has solved this challenge, so a new instance cannot be started"
                        .into(),
                ));
            }
        }
        return Ok(());
    }
    Err(not_yours())
}
