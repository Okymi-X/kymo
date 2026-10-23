use std::collections::HashSet;

use crate::error::EngineError;
use crate::model::{Bracket, Dest, Format, Match, MatchId, Side, Slot, Stage, TeamId};

pub const MAX_TEAMS: usize = 256;

/// `teams` is ordered by seed: index 0 is seed 1.
pub fn generate(teams: &[TeamId], format: Format) -> Result<Bracket, EngineError> {
    validate(teams, format)?;
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
    }

    let mut bracket = Bracket { format, matches };
    for id in winners[1].clone() {
        bracket.settle(id)?;
    }
    Ok(bracket)
}

fn validate(teams: &[TeamId], format: Format) -> Result<(), EngineError> {
    let min = match format {
        Format::SingleElimination => 2,
        Format::DoubleElimination => 3,
    };
    if teams.len() < min {
        return Err(EngineError::TooFewTeams { min });
    }
    if teams.len() > MAX_TEAMS {
        return Err(EngineError::TooManyTeams { max: MAX_TEAMS });
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
fn seed_order(size: usize) -> Vec<usize> {
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
