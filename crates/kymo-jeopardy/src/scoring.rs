//! Challenge scoring.
//!
//! Solves never store points. Dynamic challenges lose value as more
//! participants solve them, and every earlier solver is worth the new lower
//! value too, so the scoreboard recomputes each solve from the challenge's
//! current total solve count via [`current_value`].
//!
//! Like CTFd, the first solver receives the full initial value: the curve is
//! evaluated at `solves - 1`. In the formulas below `n = solves - 1`
//! (saturating at 0).

use serde::{Deserialize, Serialize};

use crate::error::ScoringError;

/// Shape of a dynamic decay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    /// `initial - decay * n`, floored at the minimum. `decay` is points lost
    /// per extra solve.
    Linear,
    /// CTFd parabolic: `ceil(((minimum - initial) / decay^2) * n^2 + initial)`,
    /// floored at the minimum. `decay` is the solve count at which the value
    /// reaches the minimum.
    Parabolic,
    /// `ceil(initial - (initial - minimum) * ln(1 + n) / ln(1 + decay))`.
    /// Drops fast, then flattens. Reaches the minimum at `n = decay`.
    Logarithmic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scoring {
    Static {
        points: i64,
    },
    Dynamic {
        initial: i64,
        minimum: i64,
        decay: u32,
        curve: Curve,
    },
}

impl Scoring {
    pub fn validate(&self) -> Result<(), ScoringError> {
        match *self {
            Scoring::Static { points } if points < 0 => Err(ScoringError::NegativePoints),
            Scoring::Static { .. } => Ok(()),
            Scoring::Dynamic {
                initial,
                minimum,
                decay,
                ..
            } => {
                if initial < 0 {
                    Err(ScoringError::NegativeInitial)
                } else if minimum < 0 || minimum > initial {
                    Err(ScoringError::BadMinimum)
                } else if decay == 0 {
                    Err(ScoringError::ZeroDecay)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// A purchasable hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hint {
    pub id: u64,
    pub cost: i64,
}

/// Value of the challenge when `total_solves` participants have solved it.
/// This is what every solver of the challenge is worth right now.
pub fn current_value(scoring: &Scoring, total_solves: u32) -> i64 {
    match *scoring {
        Scoring::Static { points } => points.max(0),
        Scoring::Dynamic {
            initial,
            minimum,
            decay,
            curve,
        } => {
            let initial = initial.max(0);
            let minimum = minimum.clamp(0, initial);
            let n = i128::from(total_solves.saturating_sub(1));
            let decay_i = i128::from(decay);
            if n == 0 {
                return initial;
            }
            if decay == 0 || n >= decay_i {
                return minimum;
            }
            let span = i128::from(initial) - i128::from(minimum);
            let value = match curve {
                Curve::Linear => i128::from(initial) - decay_i * n,
                Curve::Parabolic => {
                    // ceil of a non positive fraction is truncation toward zero.
                    i128::from(initial) - (span * n * n) / (decay_i * decay_i)
                }
                Curve::Logarithmic => {
                    let ratio = (1.0 + n as f64).ln() / (1.0 + decay_i as f64).ln();
                    let drop = (span as f64 * ratio).floor() as i128;
                    i128::from(initial) - drop
                }
            };
            let clamped = value.clamp(i128::from(minimum), i128::from(initial));
            i64::try_from(clamped).unwrap_or(minimum)
        }
    }
}

/// Value of a challenge for the solver who is number `solve_count_including_this`.
/// Equal to [`current_value`]; kept as a separate name for submission time use.
pub fn points_for(scoring: &Scoring, solve_count_including_this: u32) -> i64 {
    current_value(scoring, solve_count_including_this)
}

/// Points a solve is worth after hint penalties. Never below zero.
pub fn score_for_solve(base_points: i64, unlocked_hint_costs: &[i64]) -> i64 {
    let penalty = unlocked_hint_costs
        .iter()
        .fold(0i64, |acc, c| acc.saturating_add((*c).max(0)));
    base_points.saturating_sub(penalty).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dynamic(curve: Curve) -> Scoring {
        Scoring::Dynamic {
            initial: 500,
            minimum: 100,
            decay: 10,
            curve,
        }
    }

    #[test]
    fn static_is_constant() {
        let s = Scoring::Static { points: 300 };
        assert_eq!(current_value(&s, 0), 300);
        assert_eq!(current_value(&s, 1), 300);
        assert_eq!(current_value(&s, 1000), 300);
        assert_eq!(points_for(&s, 5), 300);
    }

    #[test]
    fn parabolic_values() {
        let s = dynamic(Curve::Parabolic);
        assert_eq!(current_value(&s, 0), 500);
        assert_eq!(current_value(&s, 1), 500);
        // n = 1: 500 - ceil(400/100) = 496
        assert_eq!(current_value(&s, 2), 496);
        // n = 5: 500 - 400*25/100 = 400
        assert_eq!(current_value(&s, 6), 400);
        // n = 9: 500 - floor(400*81/100) = 500 - 324 = 176
        assert_eq!(current_value(&s, 10), 176);
        // n = decay
        assert_eq!(current_value(&s, 11), 100);
        assert_eq!(current_value(&s, 10_000), 100);
    }

    #[test]
    fn parabolic_matches_ctfd_ceil() {
        // value = ceil(((100-500)/100) * 9 + 500) = ceil(-36 + 500) = 464 for n = 3
        let s = dynamic(Curve::Parabolic);
        assert_eq!(current_value(&s, 4), 464);
        // non integer: initial 10, min 1, decay 4, n = 1: ceil(-9/16 + 10) = 10
        let t = Scoring::Dynamic {
            initial: 10,
            minimum: 1,
            decay: 4,
            curve: Curve::Parabolic,
        };
        assert_eq!(current_value(&t, 2), 10);
        // n = 2: ceil(-9*4/16 + 10) = ceil(7.75) = 8
        assert_eq!(current_value(&t, 3), 8);
    }

    #[test]
    fn linear_values() {
        let s = Scoring::Dynamic {
            initial: 500,
            minimum: 100,
            decay: 20,
            curve: Curve::Linear,
        };
        assert_eq!(current_value(&s, 0), 500);
        assert_eq!(current_value(&s, 1), 500);
        assert_eq!(current_value(&s, 2), 480);
        assert_eq!(current_value(&s, 6), 400);
        // reaches minimum at n = decay and below never
        assert_eq!(current_value(&s, 21), 100);
        assert_eq!(current_value(&s, 500), 100);
        // floored when the linear line crosses the minimum before n = decay
        let t = Scoring::Dynamic {
            initial: 500,
            minimum: 450,
            decay: 30,
            curve: Curve::Linear,
        };
        assert_eq!(current_value(&t, 3), 450);
    }

    #[test]
    fn logarithmic_values() {
        let s = dynamic(Curve::Logarithmic);
        assert_eq!(current_value(&s, 0), 500);
        assert_eq!(current_value(&s, 1), 500);
        assert_eq!(current_value(&s, 11), 100);
        assert_eq!(current_value(&s, 5000), 100);
        let mut prev = 500;
        for n in 1..=12 {
            let v = current_value(&s, n);
            assert!(v <= prev && (100..=500).contains(&v));
            prev = v;
        }
        // drops fast early: more than half the span is gone by n = 3 of 10
        assert!(current_value(&s, 4) < 300);
    }

    #[test]
    fn parabolic_monotonic() {
        let s = dynamic(Curve::Parabolic);
        let mut prev = i64::MAX;
        for n in 0..30 {
            let v = current_value(&s, n);
            assert!(v <= prev);
            prev = v;
        }
    }

    #[test]
    fn minimum_above_initial_is_clamped() {
        let s = Scoring::Dynamic {
            initial: 100,
            minimum: 300,
            decay: 5,
            curve: Curve::Parabolic,
        };
        assert_eq!(current_value(&s, 3), 100);
        assert!(s.validate().is_err());
    }

    #[test]
    fn zero_decay_does_not_panic() {
        let s = Scoring::Dynamic {
            initial: 100,
            minimum: 10,
            decay: 0,
            curve: Curve::Parabolic,
        };
        assert_eq!(current_value(&s, 1), 100);
        assert_eq!(current_value(&s, 2), 10);
        assert_eq!(s.validate(), Err(ScoringError::ZeroDecay));
    }

    #[test]
    fn validate_cases() {
        assert_eq!(
            Scoring::Static { points: -1 }.validate(),
            Err(ScoringError::NegativePoints)
        );
        assert_eq!(Scoring::Static { points: 0 }.validate(), Ok(()));
        let neg = Scoring::Dynamic {
            initial: -1,
            minimum: 0,
            decay: 1,
            curve: Curve::Linear,
        };
        assert_eq!(neg.validate(), Err(ScoringError::NegativeInitial));
        assert_eq!(dynamic(Curve::Linear).validate(), Ok(()));
    }

    #[test]
    fn hint_penalties() {
        assert_eq!(score_for_solve(100, &[]), 100);
        assert_eq!(score_for_solve(100, &[10, 20]), 70);
        assert_eq!(score_for_solve(100, &[100]), 0);
        assert_eq!(score_for_solve(100, &[60, 60]), 0);
        assert_eq!(score_for_solve(100, &[-50]), 100);
        assert_eq!(score_for_solve(0, &[5]), 0);
        assert_eq!(score_for_solve(10, &[i64::MAX, i64::MAX]), 0);
    }
}
