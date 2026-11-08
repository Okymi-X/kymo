//! Pure rules for tournament matches that are decided by challenges.
//!
//! A match gives each side the same set of challenges. A side's match score is the points it
//! earned on them, and the tournament format turns the two scores into a win, a draw and
//! standings exactly as it does for a typed-in score. No IO, no clock, no randomness: time is
//! passed in as unix seconds and the draw is seeded.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Most challenges one match can carry.
pub const MAX_PER_MATCH: usize = 10;
/// Most question items one DFIR challenge can carry.
pub const MAX_ITEMS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DuelError {
    #[error("a DFIR challenge needs 1 to {MAX_ITEMS} questions")]
    ItemCount,
    #[error("every question is worth at least 1 point")]
    ZeroItem,
    #[error("the questions are worth {items} points together but the challenge is worth {total}")]
    PointsMismatch { items: u32, total: u32 },
    #[error("a match carries 1 to {MAX_PER_MATCH} challenges")]
    BadCount,
}

/// How challenges reach matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assignment {
    /// Matches carry no challenges: scores are entered by hand, as before.
    #[default]
    None,
    /// An organizer picks the challenges of every match.
    Manual,
    /// Kymo draws the challenges for each match.
    PerMatch,
    /// Kymo draws once per round; every match of the round gets the same ones.
    PerRound,
}

impl Assignment {
    pub fn as_str(self) -> &'static str {
        match self {
            Assignment::None => "none",
            Assignment::Manual => "manual",
            Assignment::PerMatch => "per_match",
            Assignment::PerRound => "per_round",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Assignment::None,
            Assignment::Manual,
            Assignment::PerMatch,
            Assignment::PerRound,
        ]
        .into_iter()
        .find(|a| a.as_str() == s)
    }
}

pub fn check_count(count: usize) -> Result<(), DuelError> {
    if (1..=MAX_PER_MATCH).contains(&count) {
        Ok(())
    } else {
        Err(DuelError::BadCount)
    }
}

/// The items of a DFIR challenge must add up to the challenge's points, so partial credit can
/// never exceed the total.
pub fn check_items(total: u32, items: &[u32]) -> Result<(), DuelError> {
    if items.is_empty() || items.len() > MAX_ITEMS {
        return Err(DuelError::ItemCount);
    }
    if items.contains(&0) {
        return Err(DuelError::ZeroItem);
    }
    let sum: u64 = items.iter().map(|p| u64::from(*p)).sum();
    if sum != u64::from(total) {
        return Err(DuelError::PointsMismatch {
            items: u32::try_from(sum).unwrap_or(u32::MAX),
            total,
        });
    }
    Ok(())
}

/// Points earned for a challenge: the correct items added up, never more than its total.
pub fn earned(total: u32, correct_items: &[u32]) -> u32 {
    let sum: u64 = correct_items.iter().map(|p| u64::from(*p)).sum();
    u32::try_from(sum.min(u64::from(total))).unwrap_or(total)
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Draws `count` challenges from `pool`. The same seed and key always give the same draw.
/// Challenges in `seen` (already played by either side) come last, so repeats only happen when
/// the pool is too small to avoid them. Returns fewer than `count` only when the pool is smaller.
pub fn draw(pool: &[i64], count: usize, seed: u64, key: u64, seen: &HashSet<i64>) -> Vec<i64> {
    let mut ranked: Vec<(bool, u64, i64)> = pool
        .iter()
        .map(|id| {
            let h = mix(seed ^ mix(key) ^ mix(*id as u64));
            (seen.contains(id), h, *id)
        })
        .collect();
    ranked.sort_unstable();
    ranked
        .into_iter()
        .take(count)
        .map(|(_, _, id)| id)
        .collect()
}

/// One correct answer a side has earned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Award {
    /// 0 for side A, 1 for side B.
    pub side: usize,
    pub points: u32,
    pub at: i64,
}

/// What one side still has open, so the match can tell when everybody is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Open {
    /// Questions or challenges neither answered correctly nor out of attempts.
    pub unsettled: usize,
}

/// What the rules say about a match right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Still being played.
    Open,
    /// Over, with a score. `tiebreak` names the side that wins a tied score when the format
    /// needs a winner: the one that reached its score first.
    Final {
        score: [u32; 2],
        tiebreak: Option<usize>,
    },
    /// Over and tied with nothing to break the tie: an organizer must decide.
    NeedsDecision { score: [u32; 2] },
}

pub fn scores(awards: &[Award]) -> [u32; 2] {
    let mut out = [0u32; 2];
    for a in awards {
        out[a.side] = out[a.side].saturating_add(a.points);
    }
    out
}

/// Decides whether the match is over and how it ended.
///
/// It is over when both sides have settled everything they were given, or when `closed` (the
/// time limit passed or an organizer closed it). A tied score is a draw when the format allows
/// one; otherwise the side that reached the score first wins, and if that cannot be told apart
/// an organizer decides.
pub fn decide(awards: &[Award], open: [Open; 2], closed: bool, allows_draw: bool) -> Outcome {
    if !closed && open.iter().any(|o| o.unsettled > 0) {
        return Outcome::Open;
    }
    let score = scores(awards);
    if score[0] != score[1] || allows_draw {
        return Outcome::Final {
            score,
            tiebreak: None,
        };
    }
    let last = |side: usize| awards.iter().filter(|a| a.side == side).map(|a| a.at).max();
    match (last(0), last(1)) {
        (Some(a), Some(b)) if a < b => Outcome::Final {
            score,
            tiebreak: Some(0),
        },
        (Some(a), Some(b)) if b < a => Outcome::Final {
            score,
            tiebreak: Some(1),
        },
        _ => Outcome::NeedsDecision { score },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn award(side: usize, points: u32, at: i64) -> Award {
        Award { side, points, at }
    }

    #[test]
    fn items_must_add_up_to_the_total() {
        assert_eq!(check_items(30, &[10, 20]), Ok(()));
        assert_eq!(
            check_items(30, &[10, 10]),
            Err(DuelError::PointsMismatch {
                items: 20,
                total: 30
            })
        );
        assert_eq!(check_items(30, &[]), Err(DuelError::ItemCount));
        assert_eq!(check_items(10, &[10, 0]), Err(DuelError::ZeroItem));
    }

    #[test]
    fn partial_credit_never_exceeds_the_total() {
        assert_eq!(earned(30, &[10, 5]), 15);
        assert_eq!(earned(30, &[10, 20, 20]), 30);
        assert_eq!(earned(30, &[]), 0);
    }

    #[test]
    fn a_draw_is_stable_and_avoids_repeats() {
        let pool = [1, 2, 3, 4, 5, 6];
        let none = HashSet::new();
        assert_eq!(draw(&pool, 3, 7, 42, &none), draw(&pool, 3, 7, 42, &none));
        assert_ne!(draw(&pool, 3, 7, 42, &none), draw(&pool, 3, 7, 43, &none));
        let seen: HashSet<i64> = [1, 2, 3].into_iter().collect();
        let picked = draw(&pool, 3, 7, 42, &seen);
        assert!(picked.iter().all(|id| !seen.contains(id)), "{picked:?}");
        // A small pool repeats rather than returning too few.
        assert_eq!(draw(&[1, 2], 5, 1, 1, &none).len(), 2);
    }

    #[test]
    fn a_match_stays_open_until_both_sides_are_done() {
        let busy = [Open { unsettled: 1 }, Open::default()];
        assert_eq!(
            decide(&[award(0, 10, 1)], busy, false, false),
            Outcome::Open
        );
        let done = [Open::default(); 2];
        assert_eq!(
            decide(&[award(0, 10, 1)], done, false, false),
            Outcome::Final {
                score: [10, 0],
                tiebreak: None
            }
        );
        // Closing ends it even with work left.
        assert!(matches!(
            decide(&[award(0, 10, 1)], busy, true, false),
            Outcome::Final { .. }
        ));
    }

    #[test]
    fn ties_draw_or_go_to_whoever_got_there_first() {
        let done = [Open::default(); 2];
        let tied = [award(0, 10, 100), award(1, 10, 90)];
        assert_eq!(
            decide(&tied, done, false, true),
            Outcome::Final {
                score: [10, 10],
                tiebreak: None
            }
        );
        assert_eq!(
            decide(&tied, done, false, false),
            Outcome::Final {
                score: [10, 10],
                tiebreak: Some(1)
            }
        );
        // Same moment, or nothing scored: a person decides.
        let same = [award(0, 10, 5), award(1, 10, 5)];
        assert_eq!(
            decide(&same, done, false, false),
            Outcome::NeedsDecision { score: [10, 10] }
        );
        assert_eq!(
            decide(&[], done, false, false),
            Outcome::NeedsDecision { score: [0, 0] }
        );
    }
}
