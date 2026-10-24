use std::collections::HashSet;

use crate::error::EngineError;
use crate::model::{Bracket, Format, Match, Side, Slot, Stage, TeamId};

const MAX_SWISS_ROUNDS: usize = 12;
/// Backtracking budget for one pairing. Past it, rematches are allowed so a round always exists.
const PAIRING_BUDGET: usize = 200_000;

/// Rounds an event plays when the organizer does not choose: enough for one clear leader.
fn default_rounds(teams: usize) -> usize {
    (teams.next_power_of_two().trailing_zeros() as usize).max(1)
}

type Pair = (TeamId, TeamId);

fn key(a: TeamId, b: TeamId) -> Pair {
    (a.min(b), a.max(b))
}

/// Pairs teams from the top of `order` down so nobody meets twice, or `None` when impossible
/// within the budget.
fn pair_without_rematches(
    order: &[TeamId],
    played: &HashSet<Pair>,
    budget: &mut usize,
) -> Option<Vec<Pair>> {
    let Some((&first, rest)) = order.split_first() else {
        return Some(Vec::new());
    };
    for (j, &other) in rest.iter().enumerate() {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if played.contains(&key(first, other)) {
            continue;
        }
        let mut remaining = rest.to_vec();
        remaining.remove(j);
        if let Some(mut pairs) = pair_without_rematches(&remaining, played, budget) {
            pairs.insert(0, (first, other));
            return Some(pairs);
        }
    }
    None
}

fn round_matches(round: u32, pairs: &[Pair], bye: Option<TeamId>, first_id: usize) -> Vec<Match> {
    let mut slots: Vec<[Slot; 2]> = pairs
        .iter()
        .map(|&(a, b)| [Slot::Team(a), Slot::Team(b)])
        .collect();
    if let Some(team) = bye {
        slots.push([Slot::Team(team), Slot::Bye]);
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(i, slots)| Match {
            id: first_id + i,
            stage: Stage::League,
            round,
            position: i as u32,
            slots,
            score: None,
            winner: (slots[1] == Slot::Bye).then_some(Side::A),
            forfeit: false,
            games: Vec::new(),
            started_at: None,
            group: None,
            scheduled_at: None,
            venue: None,
            next_win: None,
            next_lose: None,
        })
        .collect()
}

pub(crate) fn swiss(teams: &[TeamId], rounds: u8) -> Result<Bracket, EngineError> {
    let total = if rounds == 0 {
        default_rounds(teams.len())
    } else {
        usize::from(rounds)
    };
    if total > MAX_SWISS_ROUNDS || total >= teams.len() {
        return Err(EngineError::BadGroupSetup(format!(
            "Swiss plays 1 to {} rounds",
            MAX_SWISS_ROUNDS.min(teams.len() - 1)
        )));
    }
    // Round one: the top half meets the bottom half, the last seed sits out when the count is odd.
    let bye = (teams.len() % 2 == 1).then(|| teams[teams.len() - 1]);
    let field = &teams[..teams.len() - usize::from(bye.is_some())];
    let half = field.len() / 2;
    let pairs: Vec<Pair> = (0..half).map(|i| (field[i], field[i + half])).collect();
    Ok(Bracket {
        format: Format::Swiss,
        matches: round_matches(1, &pairs, bye, 0),
        tiebreak: Vec::new(),
        swiss_rounds: total as u32,
        best_of: 1,
    })
}

impl Bracket {
    /// A Swiss result cannot change once a later round has been played, because those pairings
    /// were drawn from it.
    pub(crate) fn swiss_guard(&self, id: usize) -> Result<(), EngineError> {
        if self.format != Format::Swiss {
            return Ok(());
        }
        let round = self.matches.get(id).map_or(0, |m| m.round);
        if self
            .matches
            .iter()
            .any(|m| m.round > round && m.is_played())
        {
            Err(EngineError::DownstreamPlayed)
        } else {
            Ok(())
        }
    }

    fn swiss_round_done(&self, round: u32) -> bool {
        let mut any = false;
        for m in self.matches.iter().filter(|m| m.round == round) {
            any = true;
            if !m.slots.contains(&Slot::Bye) && !m.is_played() {
                return false;
            }
        }
        any
    }

    /// Plans the next round once every earlier round is fully played, and keeps it in step when
    /// an earlier result changes. A round whose matches were already played blocks the change.
    pub(crate) fn refresh_swiss(&mut self) -> Result<(), EngineError> {
        let finished = (1..=self.swiss_rounds)
            .take_while(|&r| self.swiss_round_done(r))
            .count() as u32;
        if finished == 0 {
            // Round one is fixed at generation; nothing earlier can change it.
            return Ok(());
        }
        let target = (finished + 1).min(self.swiss_rounds + 1);
        let stale =
            |b: &Bracket, from: u32| b.matches.iter().any(|m| m.round >= from && m.is_played());

        let wanted = if target <= self.swiss_rounds {
            Some(self.plan_swiss_round(finished, target))
        } else {
            None
        };
        let current: Vec<&Match> = self.matches.iter().filter(|m| m.round == target).collect();
        let same = wanted.as_ref().is_some_and(|(pairs, bye)| {
            let plan: HashSet<Pair> = pairs.iter().map(|&(a, b)| key(a, b)).collect();
            let now: HashSet<Pair> = current
                .iter()
                .filter_map(|m| match m.slots {
                    [Slot::Team(a), Slot::Team(b)] => Some(key(a, b)),
                    _ => None,
                })
                .collect();
            let now_bye = current.iter().find_map(|m| match m.slots {
                [Slot::Team(a), Slot::Bye] => Some(a),
                _ => None,
            });
            plan == now && *bye == now_bye && !current.is_empty()
        });
        let later_exists = self.matches.iter().any(|m| m.round > target);
        if same && !later_exists {
            return Ok(());
        }
        if same {
            if stale(self, target + 1) {
                return Err(EngineError::DownstreamPlayed);
            }
            self.matches.retain(|m| m.round <= target);
            return Ok(());
        }
        if stale(self, target) {
            return Err(EngineError::DownstreamPlayed);
        }
        self.matches.retain(|m| m.round < target);
        if let Some((pairs, bye)) = wanted {
            let first = self.matches.len();
            self.matches
                .extend(round_matches(target, &pairs, bye, first));
        }
        Ok(())
    }

    /// Pairs for `round`, using only the standings after `finished` rounds.
    fn plan_swiss_round(&self, finished: u32, round: u32) -> (Vec<Pair>, Option<TeamId>) {
        let history = Bracket {
            matches: self
                .matches
                .iter()
                .filter(|m| m.round <= finished)
                .cloned()
                .collect(),
            ..self.clone()
        };
        let order: Vec<TeamId> = history.standings().iter().map(|s| s.team).collect();
        let had_bye: HashSet<TeamId> = history
            .matches
            .iter()
            .filter_map(|m| match m.slots {
                [Slot::Team(a), Slot::Bye] => Some(a),
                _ => None,
            })
            .collect();
        let played: HashSet<Pair> = history
            .matches
            .iter()
            .filter_map(|m| match m.slots {
                [Slot::Team(a), Slot::Team(b)] => Some(key(a, b)),
                _ => None,
            })
            .collect();
        let bye = (order.len() % 2 == 1)
            .then(|| {
                order
                    .iter()
                    .rev()
                    .find(|t| !had_bye.contains(t))
                    .or(order.last())
                    .copied()
            })
            .flatten();
        let field: Vec<TeamId> = order.iter().copied().filter(|t| Some(*t) != bye).collect();
        let mut budget = PAIRING_BUDGET;
        let pairs = pair_without_rematches(&field, &played, &mut budget)
            .or_else(|| {
                pair_without_rematches(&field, &HashSet::new(), &mut PAIRING_BUDGET.clone())
            })
            .unwrap_or_default();
        let _ = round;
        (pairs, bye)
    }
}
