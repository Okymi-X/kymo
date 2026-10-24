use kymo_engine::{Bracket, Format, Options, Side, Slot, Stage, TeamId, generate_with};
use serde_json::json;
use sqlx::SqlitePool;

use crate::db::{self, NewTeam, NewTournament, Status};
use crate::error::ApiError;
use crate::events;
use crate::tokens::now;

const NAMES: [&str; 32] = [
    "Null Pointers",
    "Stack Smashers",
    "Zero Day Club",
    "Root Shell",
    "Heap Spray",
    "Format String",
    "Ret2libc",
    "Sudo Su",
    "Packet Pirates",
    "Byte Bandits",
    "Cipher Punks",
    "Kernel Panic",
    "Bit Flippers",
    "Rop Chain",
    "Race Condition",
    "Fuzz Lords",
    "Buffer Overlords",
    "Shellcode Sages",
    "Syscall Squad",
    "Pwn Stars",
    "Hash Collision",
    "Salt and Pepper",
    "Rainbow Tables",
    "Dead Drop",
    "Open Redirect",
    "Blind Sqli",
    "Type Juggling",
    "Use After Free",
    "Off By One",
    "Magic Bytes",
    "Stego Saurus",
    "Reverse Engineers",
];

const ADJECTIVES: [&str; 10] = [
    "Silent", "Rapid", "Crimson", "Hollow", "Neon", "Arctic", "Binary", "Rogue", "Lunar", "Iron",
];
const NOUNS: [&str; 10] = [
    "Falcons", "Wolves", "Ravens", "Foxes", "Sharks", "Tigers", "Vipers", "Owls", "Lynx", "Bears",
];

/// A readable, unique name for team number `index`: the hand written list first, then combinations.
fn team_name(index: usize) -> String {
    match NAMES.get(index) {
        Some(name) => (*name).to_owned(),
        None => {
            let n = index - NAMES.len();
            format!("{} {}", ADJECTIVES[n % 10], NOUNS[n / 10 % 10])
        }
    }
}

struct Plan {
    name: &'static str,
    format: Format,
    teams: usize,
    third_place: bool,
    groups: u8,
    advance: u8,
    rounds: u8,
    /// Games per match.
    best_of: u8,
    /// How many matches to play: `None` plays everything, `Some(0)` leaves the roster only.
    play: Option<usize>,
    started: bool,
    story: bool,
}

const PLANS: [Plan; 9] = [
    Plan {
        name: "Swiss Open",
        format: Format::Swiss,
        teams: 12,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 4,
        best_of: 1,
        play: Some(14),
        started: true,
        story: false,
    },
    Plan {
        name: "Grand Finals Bo5",
        format: Format::SingleElimination,
        teams: 4,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 5,
        play: Some(1),
        started: true,
        story: false,
    },
    Plan {
        name: "Spring Cup 2026",
        format: Format::DoubleElimination,
        teams: 8,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(7),
        started: true,
        story: false,
    },
    Plan {
        name: "Winter Finals",
        format: Format::SingleElimination,
        teams: 8,
        third_place: true,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: None,
        started: true,
        story: false,
    },
    Plan {
        name: "Summer Qualifier",
        format: Format::RoundRobin,
        teams: 6,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(11),
        started: true,
        story: false,
    },
    Plan {
        name: "Worlds 2026",
        format: Format::GroupKnockout,
        teams: 12,
        third_place: false,
        groups: 3,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(20),
        started: true,
        story: false,
    },
    Plan {
        name: "Autumn Open",
        format: Format::SingleElimination,
        teams: 16,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(0),
        started: false,
        story: false,
    },
    Plan {
        name: "CTF World Open",
        format: Format::SingleElimination,
        teams: 100,
        third_place: true,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(70),
        started: true,
        story: false,
    },
    Plan {
        name: "Night Owls Invitational",
        format: Format::SingleElimination,
        teams: 5,
        third_place: false,
        groups: 2,
        advance: 2,
        rounds: 0,
        best_of: 1,
        play: Some(2),
        started: true,
        story: true,
    },
];

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Stronger (lower numbered) team wins; every fifth league match is a draw and scores vary a little.
fn demo_score(bracket: &Bracket, id: usize) -> (u32, u32) {
    let [Slot::Team(a), Slot::Team(b)] = bracket.matches[id].slots else {
        return (1, 0);
    };
    let margin = 1 + (id as u32 * 7 + a + b) % 3;
    if bracket.matches[id].stage == Stage::League && id % 5 == 4 {
        let goals = id as u32 % 3;
        return (goals, goals);
    }
    if a < b {
        (margin + 1, margin - 1)
    } else {
        (margin - 1, margin + 1)
    }
}

/// The stronger team clinches the series, after losing a game or two on the way.
fn demo_series(bracket: &Bracket, id: usize) -> Vec<[u32; 2]> {
    let [Slot::Team(a), Slot::Team(b)] = bracket.matches[id].slots else {
        return Vec::new();
    };
    let need = usize::from(bracket.best_of / 2 + 1);
    let lost = id % need;
    let mut games = Vec::new();
    for game in 0..need + lost {
        // The weaker side takes its games early so the series stays alive.
        let weaker_wins = game < lost;
        let strong = 10 + (game as u32 * 3 + id as u32) % 5;
        let weak = strong.saturating_sub(2 + game as u32 % 3);
        let strong_is_a = a < b;
        games.push(match (strong_is_a, weaker_wins) {
            (true, false) | (false, true) => [strong, weak],
            _ => [weak, strong],
        });
    }
    games
}

fn next_ready(bracket: &Bracket) -> Option<usize> {
    bracket
        .matches
        .iter()
        .find(|m| m.is_ready() && !m.is_played())
        .map(|m| m.id)
}

async fn create_one(pool: &SqlitePool, plan: &Plan) -> Result<(), ApiError> {
    let slug = slugify(plan.name);
    if db::get_by_slug(pool, &slug).await.is_ok() {
        return Ok(());
    }
    let id = db::create(
        pool,
        NewTournament {
            name: plan.name,
            slug: &slug,
            format: plan.format,
            third_place: plan.third_place,
            groups: plan.groups,
            advance: plan.advance,
            rounds: plan.rounds,
            best_of: plan.best_of,
        },
    )
    .await?;
    let roster: Vec<NewTeam> = (0..plan.teams)
        .map(|i| NewTeam {
            name: team_name(i),
            score: (4000 - 37 * i as i64).max(0),
        })
        .collect();
    db::replace_teams(pool, id, &roster).await?;
    if !plan.started {
        return Ok(());
    }
    let ids: Vec<TeamId> = db::get(pool, id)
        .await?
        .teams
        .iter()
        .map(|t| t.id as TeamId)
        .collect();
    let options = Options {
        third_place: plan.third_place,
        groups: plan.groups,
        advance: plan.advance,
        rounds: plan.rounds,
        best_of: plan.best_of,
    };
    let mut bracket = generate_with(&ids, plan.format, options)?;

    let hours_ago = 5 * 3600;
    let mut at = now() - hours_ago;
    events::record_tournament_at(
        pool,
        id,
        "started",
        json!({ "teams": ids.len(), "format": plan.format }),
        at,
    )
    .await?;
    let limit = plan.play.unwrap_or(usize::MAX);
    for played in 0..limit {
        let Some(match_id) = next_ready(&bracket) else {
            break;
        };
        let (a, b) = demo_score(&bracket, match_id);
        if bracket.best_of > 1 {
            bracket.set_games(match_id, demo_series(&bracket, match_id))?;
        } else {
            bracket.set_result(match_id, a, b)?;
        }
        at += 240;
        events::record_match_at(pool, id, "result", &bracket, match_id, at).await?;
        if plan.story && played == 0 {
            // A correction and a forfeit make the activity log worth reading.
            bracket.set_result(match_id, b, a + 1)?;
            at += 90;
            events::record_match_at(pool, id, "corrected", &bracket, match_id, at).await?;
        }
    }
    if let (true, Some(match_id)) = (plan.story, next_ready(&bracket)) {
        bracket.set_forfeit(match_id, Side::B)?;
        at += 300;
        events::record_match_at(pool, id, "forfeit", &bracket, match_id, at).await?;
    }
    // Plan the next few open matches so the schedule, calendar and countdowns have something to show.
    let upcoming: Vec<usize> = bracket
        .matches
        .iter()
        .filter(|m| m.is_ready() && !m.is_played())
        .take(4)
        .map(|m| m.id)
        .collect();
    for (i, match_id) in upcoming.into_iter().enumerate() {
        let venue = if i % 2 == 0 { "Room A" } else { "Room B" };
        bracket.set_schedule(
            match_id,
            Some(now() + 900 * (i as i64 + 1)),
            Some(venue.to_owned()),
        )?;
        at += 60;
        events::record_match_at(pool, id, "scheduled", &bracket, match_id, at).await?;
    }
    let status = if bracket.champion().is_some() {
        Status::Done
    } else {
        Status::Live
    };
    db::save_bracket(pool, id, Some(&bracket), status).await
}

/// Creates the sample tournaments that do not exist yet and returns how many were added.
pub async fn seed(pool: &SqlitePool) -> Result<usize, ApiError> {
    let mut added = 0;
    for plan in &PLANS {
        if db::get_by_slug(pool, &slugify(plan.name)).await.is_err() {
            create_one(pool, plan).await?;
            added += 1;
        }
    }
    Ok(added)
}
