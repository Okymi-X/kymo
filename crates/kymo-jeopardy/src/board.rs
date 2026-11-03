//! Scoreboard ranking.
//!
//! Solves carry no points. Every solve is valued from its challenge's total
//! solve count at ranking time, so dynamic challenges re-value all earlier
//! solvers automatically. In team events the `participant` of a [`Solve`] is
//! the team id.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::scoring::{Scoring, current_value, score_for_solve};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    pub id: u64,
    pub scoring: Scoring,
}

/// One accepted correct submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Solve {
    pub participant: u64,
    pub challenge: u64,
    /// Unix seconds.
    pub at: i64,
    /// Total cost of hints this participant unlocked on the challenge.
    pub hints_cost: i64,
}

/// Ordered tie-breakers applied when points are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TieBreak {
    /// Whoever made their latest solve first (reached the score first) wins.
    EarliestLastSolve,
    /// More solved challenges wins.
    MostSolves,
    /// More first bloods wins.
    FirstBlood,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Repeated solves of the same challenge by one participant count once:
    /// the earliest (ties: first in input order).
    #[default]
    IgnoreLater,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardConfig {
    pub tie_breaks: Vec<TieBreak>,
    pub duplicates: DuplicatePolicy,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            tie_breaks: vec![TieBreak::EarliestLastSolve],
            duplicates: DuplicatePolicy::IgnoreLater,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Competition ranking: equal rows share a rank, the next rank skips.
    pub rank: u32,
    pub participant: u64,
    pub points: i64,
    pub solves: u32,
    pub last_solve_at: Option<i64>,
}

/// Counted solves: known participant, known challenge, duplicates removed
/// per [`DuplicatePolicy`], ordered by time then input order.
fn counted(
    participants: &BTreeSet<u64>,
    challenges: &BTreeMap<u64, Challenge>,
    solves: &[Solve],
    filter_participants: bool,
) -> Vec<Solve> {
    let mut indexed: Vec<(usize, &Solve)> = solves
        .iter()
        .enumerate()
        .filter(|(_, s)| challenges.contains_key(&s.challenge))
        .filter(|(_, s)| !filter_participants || participants.contains(&s.participant))
        .collect();
    indexed.sort_by_key(|(i, s)| (s.at, *i));
    let mut seen = BTreeSet::new();
    indexed
        .into_iter()
        .filter(|(_, s)| seen.insert((s.participant, s.challenge)))
        .map(|(_, s)| *s)
        .collect()
}

fn challenge_map(challenges: &[Challenge]) -> BTreeMap<u64, Challenge> {
    challenges.iter().map(|c| (c.id, *c)).collect()
}

/// Number of counted solves per challenge, the input to [`current_value`].
pub fn solve_counts(
    participants: &[Participant],
    challenges: &[Challenge],
    solves: &[Solve],
) -> BTreeMap<u64, u32> {
    let ps: BTreeSet<u64> = participants.iter().map(|p| p.id).collect();
    let cs = challenge_map(challenges);
    let mut counts = BTreeMap::new();
    for s in counted(&ps, &cs, solves, true) {
        *counts.entry(s.challenge).or_insert(0u32) += 1;
    }
    counts
}

/// Earliest solver of `challenge` (ties on time: first in input order).
pub fn first_blood(solves: &[Solve], challenge: u64) -> Option<u64> {
    solves
        .iter()
        .enumerate()
        .filter(|(_, s)| s.challenge == challenge)
        .min_by_key(|(i, s)| (s.at, *i))
        .map(|(_, s)| s.participant)
}

struct Acc {
    points: i64,
    solves: u32,
    last: Option<i64>,
    bloods: u32,
}

/// Ranks all participants, including those with no solves.
pub fn rank(
    config: &BoardConfig,
    participants: &[Participant],
    challenges: &[Challenge],
    solves: &[Solve],
) -> Vec<Row> {
    let ps: BTreeSet<u64> = participants.iter().map(|p| p.id).collect();
    let cs = challenge_map(challenges);
    let kept = counted(&ps, &cs, solves, true);

    let mut totals: BTreeMap<u64, u32> = BTreeMap::new();
    for s in &kept {
        *totals.entry(s.challenge).or_insert(0) += 1;
    }
    let mut accs: BTreeMap<u64, Acc> = ps
        .iter()
        .map(|id| {
            (
                *id,
                Acc {
                    points: 0,
                    solves: 0,
                    last: None,
                    bloods: 0,
                },
            )
        })
        .collect();
    let mut bloods_given = BTreeSet::new();
    for s in &kept {
        let Some(ch) = cs.get(&s.challenge) else {
            continue;
        };
        let value = current_value(&ch.scoring, totals.get(&s.challenge).copied().unwrap_or(0));
        let Some(acc) = accs.get_mut(&s.participant) else {
            continue;
        };
        acc.points = acc
            .points
            .saturating_add(score_for_solve(value, &[s.hints_cost]));
        acc.solves += 1;
        acc.last = Some(acc.last.map_or(s.at, |l| l.max(s.at)));
        // kept is time ordered, so the first solve seen per challenge is the blood
        if bloods_given.insert(s.challenge) {
            acc.bloods += 1;
        }
    }

    let mut rows: Vec<(u64, Acc)> = accs.into_iter().collect();
    rows.sort_by(|(ia, a), (ib, b)| compare(config, a, b).then(ia.cmp(ib)));

    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    let mut prev: Option<&Acc> = None;
    let mut rank_no = 0u32;
    for (pos, (id, acc)) in rows.iter().enumerate() {
        let tied = prev.is_some_and(|p| compare(config, p, acc) == Ordering::Equal);
        if !tied {
            rank_no = u32::try_from(pos + 1).unwrap_or(u32::MAX);
        }
        out.push(Row {
            rank: rank_no,
            participant: *id,
            points: acc.points,
            solves: acc.solves,
            last_solve_at: acc.last,
        });
        prev = Some(acc);
    }
    out
}

/// One step of a score history: the total a participant has after the solve at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub at: i64,
    pub total: i64,
}

/// How every participant's total grew over time, from the same counted solves and values as
/// [`rank`], so the last step of a participant equals their ranked points. Participants
/// without a solve have no steps. Solves valued 0 are kept: they still move the line's time.
pub fn progression(
    participants: &[Participant],
    challenges: &[Challenge],
    solves: &[Solve],
) -> BTreeMap<u64, Vec<Step>> {
    let ps: BTreeSet<u64> = participants.iter().map(|p| p.id).collect();
    let cs = challenge_map(challenges);
    let kept = counted(&ps, &cs, solves, true);
    let mut totals: BTreeMap<u64, u32> = BTreeMap::new();
    for s in &kept {
        *totals.entry(s.challenge).or_insert(0) += 1;
    }
    let mut running: BTreeMap<u64, i64> = BTreeMap::new();
    let mut out: BTreeMap<u64, Vec<Step>> = BTreeMap::new();
    for s in &kept {
        let Some(ch) = cs.get(&s.challenge) else {
            continue;
        };
        let value = current_value(&ch.scoring, totals.get(&s.challenge).copied().unwrap_or(0));
        let total = running.entry(s.participant).or_insert(0);
        *total = total.saturating_add(score_for_solve(value, &[s.hints_cost]));
        out.entry(s.participant).or_default().push(Step {
            at: s.at,
            total: *total,
        });
    }
    out
}

/// `Less` means `a` ranks higher than `b`.
fn compare(config: &BoardConfig, a: &Acc, b: &Acc) -> Ordering {
    let mut ord = b.points.cmp(&a.points);
    for tb in &config.tie_breaks {
        if ord != Ordering::Equal {
            break;
        }
        ord = match tb {
            TieBreak::EarliestLastSolve => match (a.last, b.last) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            TieBreak::MostSolves => b.solves.cmp(&a.solves),
            TieBreak::FirstBlood => b.bloods.cmp(&a.bloods),
        };
    }
    ord
}

/// One member's counted solve, valued at the current challenge value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberSolve {
    pub challenge: u64,
    pub at: i64,
    pub points: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberBreakdown {
    pub participant: u64,
    pub solves: Vec<MemberSolve>,
    pub points: i64,
}

/// What each team member solved and what it is worth.
///
/// `solves` hold individual member submissions (`participant` is the member
/// user id), `counts` is the challenge solve count at scoring unit level
/// (see [`solve_counts`] on team level solves) so values match the board.
/// Member totals can add up to more than the team score because teammates
/// may solve the same challenge, which the team counts once.
pub fn team_breakdown(
    team_members: &[u64],
    challenges: &[Challenge],
    solves: &[Solve],
    counts: &BTreeMap<u64, u32>,
) -> Vec<MemberBreakdown> {
    let cs = challenge_map(challenges);
    let members: BTreeSet<u64> = team_members.iter().copied().collect();
    let kept = counted(&members, &cs, solves, true);
    team_members
        .iter()
        .map(|m| {
            let mut list = Vec::new();
            let mut points = 0i64;
            for s in kept.iter().filter(|s| s.participant == *m) {
                let Some(ch) = cs.get(&s.challenge) else {
                    continue;
                };
                let value =
                    current_value(&ch.scoring, counts.get(&s.challenge).copied().unwrap_or(0));
                let p = score_for_solve(value, &[s.hints_cost]);
                points = points.saturating_add(p);
                list.push(MemberSolve {
                    challenge: s.challenge,
                    at: s.at,
                    points: p,
                });
            }
            MemberBreakdown {
                participant: *m,
                solves: list,
                points,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::Curve;

    fn ps(ids: &[u64]) -> Vec<Participant> {
        ids.iter().map(|id| Participant { id: *id }).collect()
    }
    fn stat(id: u64, points: i64) -> Challenge {
        Challenge {
            id,
            scoring: Scoring::Static { points },
        }
    }
    fn solve(p: u64, c: u64, at: i64) -> Solve {
        Solve {
            participant: p,
            challenge: c,
            at,
            hints_cost: 0,
        }
    }
    #[test]
    fn progression_ends_on_the_ranked_points_and_revalues_dynamic_challenges() {
        let dynamic = Challenge {
            id: 12,
            scoring: Scoring::Dynamic {
                initial: 100,
                minimum: 10,
                decay: 2,
                curve: Curve::Linear,
            },
        };
        let challenges = [stat(10, 100), stat(11, 50), dynamic];
        let solves = [
            solve(1, 10, 5),
            solve(2, 10, 6),
            solve(2, 11, 7),
            solve(1, 12, 8),
            solve(2, 12, 9),
            Solve {
                participant: 3,
                challenge: 11,
                at: 10,
                hints_cost: 20,
            },
        ];
        let people = ps(&[1, 2, 3, 4]);
        let steps = progression(&people, &challenges, &solves);
        let rows = rank(&BoardConfig::default(), &people, &challenges, &solves);
        for row in &rows {
            let last = steps.get(&row.participant).and_then(|s| s.last());
            assert_eq!(
                last.map_or(0, |s| s.total),
                row.points,
                "participant {}",
                row.participant
            );
        }
        // Totals only grow and follow time.
        for list in steps.values() {
            assert!(
                list.windows(2)
                    .all(|w| w[0].at <= w[1].at && w[0].total <= w[1].total)
            );
        }
        assert!(!steps.contains_key(&4), "no solves, no steps");
        assert_eq!(steps[&3][0].total, 30, "hint cost is taken off the solve");
    }

    fn ranks(rows: &[Row]) -> Vec<(u32, u64)> {
        rows.iter().map(|r| (r.rank, r.participant)).collect()
    }

    #[test]
    fn basic_ordering() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1, 2, 3]),
            &[stat(10, 100), stat(11, 50)],
            &[solve(1, 10, 5), solve(2, 10, 6), solve(2, 11, 7)],
        );
        assert_eq!(ranks(&rows), vec![(1, 2), (2, 1), (3, 3)]);
        assert_eq!(rows[0].points, 150);
        assert_eq!(rows[0].solves, 2);
        assert_eq!(rows[0].last_solve_at, Some(7));
        assert_eq!(rows[2].last_solve_at, None);
    }

    #[test]
    fn tie_broken_by_earliest_last_solve() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1, 2]),
            &[stat(10, 100)],
            &[solve(2, 10, 5), solve(1, 10, 9)],
        );
        assert_eq!(ranks(&rows), vec![(1, 2), (2, 1)]);
    }

    #[test]
    fn full_ties_share_rank_1_1_3() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1, 2, 3]),
            &[stat(10, 100), stat(11, 10)],
            &[solve(1, 10, 5), solve(2, 10, 5), solve(3, 11, 1)],
        );
        assert_eq!(ranks(&rows), vec![(1, 1), (1, 2), (3, 3)]);
    }

    #[test]
    fn zero_solvers_tie_together() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[3, 1, 2]),
            &[stat(10, 5)],
            &[],
        );
        assert_eq!(ranks(&rows), vec![(1, 1), (1, 2), (1, 3)]);
    }

    #[test]
    fn most_solves_tiebreak() {
        let cfg = BoardConfig {
            tie_breaks: vec![TieBreak::MostSolves, TieBreak::EarliestLastSolve],
            ..BoardConfig::default()
        };
        let rows = rank(
            &cfg,
            &ps(&[1, 2]),
            &[stat(10, 60), stat(11, 30), stat(12, 30)],
            &[solve(1, 10, 1), solve(2, 11, 2), solve(2, 12, 3)],
        );
        assert_eq!(ranks(&rows), vec![(1, 2), (2, 1)]);
        // only EarliestLastSolve would have favored participant 1
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1, 2]),
            &[stat(10, 60), stat(11, 30), stat(12, 30)],
            &[solve(1, 10, 1), solve(2, 11, 2), solve(2, 12, 3)],
        );
        assert_eq!(ranks(&rows), vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn first_blood_tiebreak() {
        let cfg = BoardConfig {
            tie_breaks: vec![TieBreak::FirstBlood],
            ..BoardConfig::default()
        };
        let rows = rank(
            &cfg,
            &ps(&[1, 2]),
            &[stat(10, 50), stat(11, 50)],
            &[
                solve(1, 10, 9),
                solve(2, 11, 2),
                solve(2, 10, 3),
                solve(1, 11, 4),
            ],
        );
        // p2 holds both first bloods (11 at 2, 10 at 3)
        assert_eq!(ranks(&rows), vec![(1, 2), (2, 1)]);
    }

    #[test]
    fn duplicates_count_once_earliest() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1]),
            &[stat(10, 100)],
            &[solve(1, 10, 20), solve(1, 10, 5), solve(1, 10, 30)],
        );
        assert_eq!(rows[0].points, 100);
        assert_eq!(rows[0].solves, 1);
        assert_eq!(rows[0].last_solve_at, Some(5));
    }

    #[test]
    fn dynamic_revalues_earlier_solvers() {
        let dynamic = Challenge {
            id: 10,
            scoring: Scoring::Dynamic {
                initial: 500,
                minimum: 100,
                decay: 10,
                curve: Curve::Parabolic,
            },
        };
        let one = rank(
            &BoardConfig::default(),
            &ps(&[1, 2]),
            &[dynamic],
            &[solve(1, 10, 1)],
        );
        assert_eq!(one[0].points, 500);
        let two = rank(
            &BoardConfig::default(),
            &ps(&[1, 2]),
            &[dynamic],
            &[solve(1, 10, 1), solve(2, 10, 2)],
        );
        assert_eq!(two[0].points, 496);
        assert_eq!(two[1].points, 496);
        assert_eq!(two[0].rank, 1);
        assert_eq!(two[1].rank, 2);
    }

    #[test]
    fn hint_cost_reduces_points_not_below_zero() {
        let mut s = solve(1, 10, 1);
        s.hints_cost = 30;
        let mut t = solve(2, 10, 2);
        t.hints_cost = 500;
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1, 2]),
            &[stat(10, 100)],
            &[s, t],
        );
        assert_eq!(rows[0].points, 70);
        assert_eq!(rows[1].points, 0);
        assert_eq!(rows[1].solves, 1);
    }

    #[test]
    fn unknown_participants_and_challenges_ignored() {
        let rows = rank(
            &BoardConfig::default(),
            &ps(&[1]),
            &[stat(10, 100)],
            &[solve(9, 10, 1), solve(1, 99, 2), solve(1, 10, 3)],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].points, 100);
        assert_eq!(
            solve_counts(
                &ps(&[1]),
                &[stat(10, 1)],
                &[solve(9, 10, 1), solve(1, 10, 3)]
            )[&10],
            1
        );
    }

    #[test]
    fn first_blood_lookup() {
        let s = [
            solve(1, 10, 9),
            solve(2, 10, 3),
            solve(3, 10, 3),
            solve(4, 11, 1),
        ];
        assert_eq!(first_blood(&s, 10), Some(2));
        assert_eq!(first_blood(&s, 11), Some(4));
        assert_eq!(first_blood(&s, 12), None);
    }

    #[test]
    fn team_breakdown_lists_each_member() {
        let chs = [stat(10, 100), stat(11, 40)];
        let solves = [
            solve(1, 10, 1),
            solve(2, 11, 2),
            solve(2, 10, 3),
            solve(2, 10, 4),
            solve(9, 10, 5),
        ];
        let mut counts = BTreeMap::new();
        counts.insert(10, 1);
        counts.insert(11, 1);
        let b = team_breakdown(&[1, 2, 3], &chs, &solves, &counts);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].participant, 1);
        assert_eq!(b[0].points, 100);
        assert_eq!(
            b[1].solves,
            vec![
                MemberSolve {
                    challenge: 11,
                    at: 2,
                    points: 40
                },
                MemberSolve {
                    challenge: 10,
                    at: 3,
                    points: 100
                },
            ]
        );
        assert_eq!(b[1].points, 140);
        assert!(b[2].solves.is_empty());
        assert_eq!(b[2].points, 0);
    }
}
