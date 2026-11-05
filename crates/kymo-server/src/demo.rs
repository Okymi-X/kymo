use kymo_core::{EventKind, EventState, Mode};
use kymo_engine::{Bracket, Format, Options, Side, Slot, Stage, TeamId, generate_with};
use kymo_jeopardy::{Curve, FlagSpec, Scoring};
use serde_json::json;
use sqlx::SqlitePool;

use crate::db::{self, NewTeam, NewTournament, Status};
use crate::error::ApiError;
use crate::tokens::now;
use crate::{challenge_store, event_store, events, jeopardy_store};

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

/// One sample challenge: where it sits, what it is worth, its flag and a hint.
struct Challenge {
    category: usize,
    title: &'static str,
    about: &'static str,
    flag: &'static str,
    scoring: Scoring,
    hint: Option<(&'static str, i64)>,
}

struct Jeopardy {
    name: &'static str,
    about: &'static str,
    categories: &'static [&'static str],
    state: EventState,
    /// Seconds from now that it starts and ends.
    starts: i64,
    ends: i64,
    players: usize,
    challenges: &'static [Challenge],
}

const DYNAMIC: Scoring = Scoring::Dynamic {
    initial: 500,
    minimum: 100,
    decay: 12,
    curve: Curve::Logarithmic,
};

const fn fixed(points: i64) -> Scoring {
    Scoring::Static { points }
}

const CHALLENGES: [Challenge; 15] = [
    Challenge {
        category: 0,
        title: "Cookie Monster",
        about: "The admin panel trusts a cookie it should not.",
        flag: "KYMO{cookie_forgery}",
        scoring: fixed(100),
        hint: Some(("Look at how the session is signed.", 20)),
    },
    Challenge {
        category: 0,
        title: "Union Station",
        about: "A search box that talks to a database a little too freely.",
        flag: "KYMO{union_select}",
        scoring: DYNAMIC,
        hint: Some(("How many columns does the query return?", 40)),
    },
    Challenge {
        category: 0,
        title: "Template Tantrum",
        about: "Your name appears on the page. Make it do more than that.",
        flag: "KYMO{ssti_pwned}",
        scoring: DYNAMIC,
        hint: None,
    },
    Challenge {
        category: 1,
        title: "Caesar's Mistake",
        about: "An old cipher with a new twist.",
        flag: "KYMO{shift_by_thirteen}",
        scoring: fixed(50),
        hint: None,
    },
    Challenge {
        category: 1,
        title: "Reused Pad",
        about: "Two messages, one key. What could go wrong?",
        flag: "KYMO{xor_all_the_things}",
        scoring: DYNAMIC,
        hint: Some(("XOR the two ciphertexts together.", 30)),
    },
    Challenge {
        category: 1,
        title: "Small Exponent",
        about: "RSA with e = 3 and a short message.",
        flag: "KYMO{cube_root_attack}",
        scoring: fixed(300),
        hint: Some(("Is the message smaller than the cube root of n?", 50)),
    },
    Challenge {
        category: 2,
        title: "Hidden in Plain Sight",
        about: "A picture of a cat. Is that all it is?",
        flag: "KYMO{strings_find_it}",
        scoring: fixed(75),
        hint: None,
    },
    Challenge {
        category: 2,
        title: "Packet Detective",
        about: "A capture of a login over plain HTTP.",
        flag: "KYMO{wireshark_wizard}",
        scoring: DYNAMIC,
        hint: None,
    },
    Challenge {
        category: 2,
        title: "Deleted but not Gone",
        about: "A disk image someone tried to clean.",
        flag: "KYMO{undelete_me}",
        scoring: fixed(250),
        hint: Some(("The file system keeps more than it shows.", 40)),
    },
    Challenge {
        category: 3,
        title: "Smash the Stack",
        about: "A classic buffer overflow with no protections.",
        flag: "KYMO{ret2win_classic}",
        scoring: fixed(200),
        hint: Some(("Where does the saved return address live?", 30)),
    },
    Challenge {
        category: 3,
        title: "Format Fiasco",
        about: "printf trusts you. Do not let it.",
        flag: "KYMO{percent_n_power}",
        scoring: DYNAMIC,
        hint: None,
    },
    Challenge {
        category: 3,
        title: "Heap of Trouble",
        about: "Use after free, the Kymo way.",
        flag: "KYMO{tcache_poison}",
        scoring: fixed(450),
        hint: Some(("Which bin does the freed chunk go to?", 60)),
    },
    Challenge {
        category: 4,
        title: "Welcome",
        about: "The flag is in the rules.",
        flag: "KYMO{read_the_rules}",
        scoring: fixed(10),
        hint: None,
    },
    Challenge {
        category: 4,
        title: "Trivia Night",
        about: "Who invented the first buffer overflow worm?",
        flag: "KYMO{morris_1988}",
        scoring: fixed(25),
        hint: None,
    },
    Challenge {
        category: 4,
        title: "Jail Break",
        about: "A restricted shell with a creative way out.",
        flag: "KYMO{escape_artist}",
        scoring: DYNAMIC,
        hint: Some(("What can you do with a built-in command?", 30)),
    },
];

const CATEGORIES: [&str; 5] = ["Web", "Crypto", "Forensics", "Pwn", "Misc"];

const PLAYERS: [&str; 16] = [
    "ada", "linus", "grace", "dennis", "ken", "margaret", "alan", "barbara", "tim", "radia",
    "vint", "shafi", "bruce", "whit", "adi", "ron",
];

/// The sample Jeopardy events. Players are made without a usable password: they fill the
/// scoreboard but nobody can sign in as them.
fn jeopardy_plans() -> [Jeopardy; 3] {
    [
        Jeopardy {
            name: "Spring Jeopardy CTF",
            about: "A live sample event with categories, hints, dynamic scoring and a busy scoreboard.",
            categories: &CATEGORIES,
            state: EventState::Live,
            starts: -6 * 3600,
            ends: 18 * 3600,
            players: 14,
            challenges: &CHALLENGES,
        },
        Jeopardy {
            name: "Winter Jeopardy",
            about: "A finished sample event: the final scoreboard and the score over time.",
            categories: &CATEGORIES,
            state: EventState::Ended,
            starts: -40 * 86_400,
            ends: -39 * 86_400,
            players: 10,
            challenges: &CHALLENGES,
        },
        Jeopardy {
            name: "Summer Jeopardy",
            about: "An upcoming sample event: registration is open and nobody has solved anything yet.",
            categories: &CATEGORIES,
            state: EventState::RegistrationOpen,
            starts: 20 * 86_400,
            ends: 21 * 86_400,
            players: 4,
            challenges: &CHALLENGES,
        },
    ]
}

async fn create_jeopardy(pool: &SqlitePool, plan: &Jeopardy, hash: &str) -> Result<(), ApiError> {
    let at = now();
    let fields = event_store::EventFields {
        kind: EventKind::Jeopardy,
        mode: Mode::Solo,
        name: plan.name.to_owned(),
        description: plan.about.to_owned(),
        registration_opens: None,
        registration_closes: None,
        starts_at: Some(at + plan.starts),
        ends_at: Some(at + plan.ends),
        scoreboard_public: true,
        team_min: 1,
        team_max: 4,
        allow_invites: true,
        roster_locked: true,
    };
    let mut tx = db::begin_write(pool).await?;
    let event_id = event_store::insert(&mut tx, &fields, &slugify(plan.name), at).await?;
    tx.commit().await?;
    let mut categories = Vec::new();
    for (position, name) in plan.categories.iter().enumerate() {
        categories
            .push(challenge_store::category_insert(pool, event_id, name, position as i64).await?);
    }
    let mut challenges = Vec::new();
    for (position, c) in plan.challenges.iter().enumerate() {
        let id = challenge_store::challenge_insert(
            pool,
            event_id,
            &challenge_store::ChallengeFields {
                category_id: categories[c.category],
                title: c.title.to_owned(),
                description: c.about.to_owned(),
                scoring: c.scoring,
                flag: FlagSpec::Static {
                    value: c.flag.to_owned(),
                },
                flag_prefix: "KYMO{".into(),
                flag_suffix: "}".into(),
                case_sensitive: true,
                max_attempts: None,
                release_at: None,
                retire_at: None,
                position: position as i64,
                requires_instance: false,
                instance_config: None,
            },
            at,
        )
        .await?;
        challenge_store::set_challenge_state(pool, id, "published").await?;
        if let Some((text, cost)) = c.hint {
            challenge_store::hint_insert(pool, id, text, cost, 0).await?;
        }
        challenges.push((id, c.flag));
    }
    // Players, registered. Earlier names are the strongest, so the scoreboard has a shape.
    let mut users = Vec::new();
    for name in PLAYERS.iter().take(plan.players) {
        let username = format!("demo-{name}");
        let id = match crate::users::by_username(pool, &username).await? {
            Some(u) => u.id,
            None => crate::users::create(pool, &username, None, hash, at).await?,
        };
        event_store::register(pool, event_id, Some(id), None, at - 86_400).await?;
        users.push(id);
    }
    if matches!(plan.state, EventState::RegistrationOpen | EventState::Draft) {
        return event_store::set_state(pool, event_id, plan.state).await;
    }
    // Solves: the player at rank r solves the first (n - 2r) challenges, easiest first, with
    // some wrong guesses on the way, spread over the event so the chart has a story.
    let span = (plan.ends.min(0) - plan.starts).max(3600);
    let mut tx = db::begin_write(pool).await?;
    for (rank, user) in users.iter().enumerate() {
        let solved = challenges
            .len()
            .saturating_sub(rank + 1)
            .min(challenges.len());
        for (k, (challenge_id, flag)) in challenges.iter().take(solved).enumerate() {
            let when = at
                + plan.starts
                + span * (k as i64 * 3 + rank as i64 + 1)
                    / (challenges.len() as i64 * 3 + users.len() as i64 + 2);
            let submission = |value: &'static str, verdict: &'static str, t: i64| {
                jeopardy_store::NewSubmission {
                    event_id,
                    challenge_id: *challenge_id,
                    scorer: jeopardy_store::Scorer::User(*user),
                    user_id: *user,
                    ip: "192.0.2.10",
                    value,
                    verdict,
                    at: t,
                }
            };
            if (k + rank) % 3 == 0 {
                jeopardy_store::insert_submission(
                    &mut tx,
                    &submission("KYMO{not_this_one}", "incorrect", when - 120),
                )
                .await?;
            }
            let ok = submission(flag, "correct", when);
            jeopardy_store::insert_submission(&mut tx, &ok).await?;
            jeopardy_store::insert_solve(&mut tx, &ok, 0).await?;
        }
    }
    tx.commit().await?;
    event_store::set_state(pool, event_id, plan.state).await
}

/// Creates the sample tournaments and Jeopardy events that do not exist yet and returns how
/// many were added.
pub async fn seed(pool: &SqlitePool) -> Result<usize, ApiError> {
    let mut added = 0;
    for plan in &PLANS {
        if db::get_by_slug(pool, &slugify(plan.name)).await.is_err() {
            create_one(pool, plan).await?;
            added += 1;
        }
    }
    // Nobody can sign in as a sample player: the password is thrown away.
    let hash = crate::passwords::hash(format!("{:032x}", rand::random::<u128>())).await?;
    for plan in &jeopardy_plans() {
        if event_store::by_slug(pool, &slugify(plan.name))
            .await
            .is_err()
        {
            create_jeopardy(pool, plan, &hash).await?;
            added += 1;
        }
    }
    Ok(added)
}
