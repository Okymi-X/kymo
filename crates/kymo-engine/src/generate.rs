use std::collections::HashSet;

use crate::error::EngineError;
use crate::model::{Bracket, Dest, Format, Match, MatchId, Side, Slot, Stage, TeamId};

pub const MAX_TEAMS: usize = 256;
/// Every pair plays, so the match count grows with the square of the team count.
pub const MAX_ROUND_ROBIN_TEAMS: usize = 32;
const MIN_THIRD_PLACE: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Single elimination only: the two semifinal losers play for third place.
    pub third_place: bool,
    /// Groups then knockout only: number of groups (2 to 8).
    pub groups: u8,
    /// Groups then knockout only: how many teams per group reach the knockout (1 to 4).
    pub advance: u8,
    /// Swiss only: number of rounds, 1 to 12. Zero picks the usual log2 of the team count.
    pub rounds: u8,
    /// Games per match: 1, 3, 5 or 7.
    pub best_of: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            third_place: false,
            groups: 2,
            advance: 2,
            rounds: 0,
            best_of: 1,
        }
    }
}

/// `teams` is ordered by seed: index 0 is seed 1.
pub fn generate(teams: &[TeamId], format: Format) -> Result<Bracket, EngineError> {
    generate_with(teams, format, Options::default())
}

pub fn generate_with(
    teams: &[TeamId],
    format: Format,
    options: Options,
) -> Result<Bracket, EngineError> {
    let best_of = match options.best_of {
        0 | 1 => 1,
        n @ (3 | 5 | 7) => n,
        _ => return Err(bad_setup("a match is best of 1, 3, 5 or 7 games")),
    };
    let mut bracket = build(teams, format, options)?;
    bracket.best_of = best_of;
    Ok(bracket)
}

fn build(teams: &[TeamId], format: Format, options: Options) -> Result<Bracket, EngineError> {
    validate(teams, format)?;
    if options.third_place && format == Format::SingleElimination && teams.len() < MIN_THIRD_PLACE {
        return Err(EngineError::TooFewTeams {
            min: MIN_THIRD_PLACE,
        });
    }
    match format {
        Format::RoundRobin => return Ok(round_robin(teams)),
        Format::GroupKnockout => return group_knockout(teams, options),
        Format::Swiss => return crate::swiss::swiss(teams, options.rounds),
        _ => {}
    }
    let size = teams.len().next_power_of_two();
    let rounds = size.trailing_zeros() as usize;

    let mut matches = Vec::new();
    let mut winners: Vec<Vec<MatchId>> = vec![Vec::new()];
    for round in 1..=rounds {
        let count = size >> round;
        winners.push(add_round(&mut matches, Stage::Winners, round, count));
    }
    link_winners_bracket(&mut matches, &winners);
    seed_first_round(&mut matches, &winners[1], teams, size);

    if format == Format::DoubleElimination {
        add_losers_bracket(&mut matches, &winners, size, rounds);
    } else if options.third_place {
        add_third_place(&mut matches, &winners, rounds);
    }

    let mut bracket = Bracket {
        format,
        matches,
        tiebreak: Vec::new(),
        swiss_rounds: 0,
        best_of: 1,
    };
    for id in winners[1].clone() {
        bracket.settle(id)?;
    }
    Ok(bracket)
}

fn validate(teams: &[TeamId], format: Format) -> Result<(), EngineError> {
    let min = match format {
        Format::SingleElimination => 2,
        Format::DoubleElimination | Format::RoundRobin | Format::GroupKnockout => 3,
        Format::Swiss => 4,
    };
    if teams.len() < min {
        return Err(EngineError::TooFewTeams { min });
    }
    let max = if matches!(format, Format::RoundRobin | Format::GroupKnockout) {
        MAX_ROUND_ROBIN_TEAMS
    } else {
        MAX_TEAMS
    };
    if teams.len() > max {
        return Err(EngineError::TooManyTeams { max });
    }
    let mut seen = HashSet::new();
    match teams.iter().find(|t| !seen.insert(**t)) {
        Some(dup) => Err(EngineError::DuplicateTeam(*dup)),
        None => Ok(()),
    }
}

fn add_round(matches: &mut Vec<Match>, stage: Stage, round: usize, count: usize) -> Vec<MatchId> {
    (0..count)
        .map(|position| {
            let id = matches.len();
            matches.push(Match {
                id,
                stage,
                round: round as u32,
                position: position as u32,
                slots: [Slot::Empty; 2],
                score: None,
                winner: None,
                forfeit: false,
                games: Vec::new(),
                started_at: None,
                group: None,
                scheduled_at: None,
                venue: None,
                next_win: None,
                next_lose: None,
            });
            id
        })
        .collect()
}

fn side_of(position: usize) -> Side {
    if position.is_multiple_of(2) {
        Side::A
    } else {
        Side::B
    }
}

fn link_winners_bracket(matches: &mut [Match], winners: &[Vec<MatchId>]) {
    for round in 1..winners.len() - 1 {
        for (i, &id) in winners[round].iter().enumerate() {
            matches[id].next_win = Some(Dest {
                id: winners[round + 1][i / 2],
                side: side_of(i),
            });
        }
    }
}

/// Standard seed order so seed 1 meets the lowest seed and byes go to the top seeds.
pub(crate) fn seed_order(size: usize) -> Vec<usize> {
    let mut order = vec![1];
    while order.len() < size {
        let total = order.len() * 2 + 1;
        order = order.iter().flat_map(|&s| [s, total - s]).collect();
    }
    order
}

fn seed_first_round(matches: &mut [Match], first: &[MatchId], teams: &[TeamId], size: usize) {
    let order = seed_order(size);
    let slot = |seed: usize| match teams.get(seed - 1) {
        Some(&team) => Slot::Team(team),
        None => Slot::Bye,
    };
    for (i, &id) in first.iter().enumerate() {
        matches[id].slots = [slot(order[2 * i]), slot(order[2 * i + 1])];
    }
}

fn losers_round_size(size: usize, round: usize) -> usize {
    size >> (round.div_ceil(2) + 1)
}

fn add_losers_bracket(
    matches: &mut Vec<Match>,
    winners: &[Vec<MatchId>],
    size: usize,
    rounds: usize,
) {
    let losers_rounds = 2 * (rounds - 1);
    let mut losers: Vec<Vec<MatchId>> = vec![Vec::new()];
    for round in 1..=losers_rounds {
        let count = losers_round_size(size, round);
        losers.push(add_round(matches, Stage::Losers, round, count));
    }
    let grand = add_round(matches, Stage::GrandFinal, 1, 1)[0];
    let reset = add_round(matches, Stage::GrandReset, 1, 1)[0];

    for (i, &id) in winners[1].iter().enumerate() {
        matches[id].next_lose = Some(Dest {
            id: losers[1][i / 2],
            side: side_of(i),
        });
    }
    for round in 2..=rounds {
        for (i, &id) in winners[round].iter().enumerate() {
            matches[id].next_lose = Some(Dest {
                id: losers[2 * (round - 1)][i],
                side: Side::B,
            });
        }
    }

    for round in 1..=losers_rounds {
        for (i, &id) in losers[round].iter().enumerate() {
            matches[id].next_win = Some(if round == losers_rounds {
                Dest {
                    id: grand,
                    side: Side::B,
                }
            } else if round % 2 == 1 {
                Dest {
                    id: losers[round + 1][i],
                    side: Side::A,
                }
            } else {
                Dest {
                    id: losers[round + 1][i / 2],
                    side: side_of(i),
                }
            });
        }
    }

    let winners_final = winners[rounds][0];
    matches[winners_final].next_win = Some(Dest {
        id: grand,
        side: Side::A,
    });
    matches[grand].next_win = Some(Dest {
        id: reset,
        side: Side::A,
    });
}

fn add_third_place(matches: &mut Vec<Match>, winners: &[Vec<MatchId>], rounds: usize) {
    let third = add_round(matches, Stage::ThirdPlace, rounds, 1)[0];
    for (i, &semi) in winners[rounds - 1].iter().enumerate() {
        matches[semi].next_lose = Some(Dest {
            id: third,
            side: side_of(i),
        });
    }
}

/// Circle method: one team stays put and the rest rotate, so every pair meets exactly once.
/// With an odd number of teams one team rests each round and no match is created for it.
fn robin_matches(teams: &[TeamId], group: Option<u32>, matches: &mut Vec<Match>) {
    let mut ring: Vec<Option<TeamId>> = teams.iter().copied().map(Some).collect();
    if ring.len() % 2 == 1 {
        ring.push(None);
    }
    let n = ring.len();
    for round in 1..n {
        let mut position = 0;
        for i in 0..n / 2 {
            if let (Some(a), Some(b)) = (ring[i], ring[n - 1 - i]) {
                let id = matches.len();
                matches.push(Match {
                    id,
                    stage: Stage::League,
                    round: round as u32,
                    position,
                    slots: [Slot::Team(a), Slot::Team(b)],
                    score: None,
                    winner: None,
                    forfeit: false,
                    games: Vec::new(),
                    started_at: None,
                    group,
                    scheduled_at: None,
                    venue: None,
                    next_win: None,
                    next_lose: None,
                });
                position += 1;
            }
        }
        ring[1..].rotate_right(1);
    }
}

fn round_robin(teams: &[TeamId]) -> Bracket {
    let mut matches = Vec::new();
    robin_matches(teams, None, &mut matches);
    Bracket {
        format: Format::RoundRobin,
        matches,
        tiebreak: Vec::new(),
        swiss_rounds: 0,
        best_of: 1,
    }
}

fn bad_setup(message: impl Into<String>) -> EngineError {
    EngineError::BadGroupSetup(message.into())
}

/// Groups are filled in a snake over the seeds so each group gets a similar spread of strength.
fn group_of(seed_index: usize, groups: usize) -> usize {
    let (row, col) = (seed_index / groups, seed_index % groups);
    if row % 2 == 0 { col } else { groups - 1 - col }
}

fn group_knockout(teams: &[TeamId], options: Options) -> Result<Bracket, EngineError> {
    let (groups, advance) = (usize::from(options.groups), usize::from(options.advance));
    if !(2..=8).contains(&groups) {
        return Err(bad_setup("use 2 to 8 groups"));
    }
    if !(1..=4).contains(&advance) {
        return Err(bad_setup("1 to 4 teams per group can advance"));
    }
    if teams.len() < 3 * groups {
        return Err(bad_setup(format!(
            "{groups} groups need at least {} teams",
            3 * groups
        )));
    }
    let smallest = teams.len() / groups;
    if advance >= smallest {
        return Err(bad_setup(
            "the smallest group must be larger than the number advancing",
        ));
    }

    let mut members: Vec<Vec<TeamId>> = vec![Vec::new(); groups];
    for (i, &team) in teams.iter().enumerate() {
        members[group_of(i, groups)].push(team);
    }
    let mut matches = Vec::new();
    for (g, group) in members.iter().enumerate() {
        robin_matches(group, Some(g as u32), &mut matches);
    }

    let entrants = groups * advance;
    let size = entrants.next_power_of_two();
    let rounds = size.trailing_zeros() as usize;
    let mut winners: Vec<Vec<MatchId>> = vec![Vec::new()];
    for round in 1..=rounds {
        winners.push(add_round(
            &mut matches,
            Stage::Winners,
            round,
            size >> round,
        ));
    }
    link_winners_bracket(&mut matches, &winners);
    let order = seed_order(size);
    for (i, &id) in winners[1].iter().enumerate() {
        let slot = |seed: usize| {
            if seed > entrants {
                Slot::Bye
            } else {
                Slot::Empty
            }
        };
        matches[id].slots = [slot(order[2 * i]), slot(order[2 * i + 1])];
    }

    let mut bracket = Bracket {
        format: Format::GroupKnockout,
        matches,
        tiebreak: Vec::new(),
        swiss_rounds: 0,
        best_of: 1,
    };
    for id in winners[1].clone() {
        bracket.settle(id)?;
    }
    Ok(bracket)
}
