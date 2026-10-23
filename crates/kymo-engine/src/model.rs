use serde::{Deserialize, Serialize};

pub type TeamId = u32;
pub type MatchId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    SingleElimination,
    DoubleElimination,
    RoundRobin,
    GroupKnockout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Winners,
    Losers,
    GrandFinal,
    GrandReset,
    ThirdPlace,
    League,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    A,
    B,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::A => 0,
            Side::B => 1,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::A => Side::B,
            Side::B => Side::A,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "team", rename_all = "snake_case")]
pub enum Slot {
    Empty,
    Bye,
    Team(TeamId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dest {
    pub id: MatchId,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub id: MatchId,
    pub stage: Stage,
    pub round: u32,
    pub position: u32,
    pub slots: [Slot; 2],
    pub score: Option<[u32; 2]>,
    pub winner: Option<Side>,
    #[serde(default)]
    pub forfeit: bool,
    /// Group index (0 is group A) for league matches of a groups then knockout tournament.
    #[serde(default)]
    pub group: Option<u32>,
    /// Planned start, unix seconds. Set by the organizer, never used for bracket logic.
    #[serde(default)]
    pub scheduled_at: Option<i64>,
    /// Where the match is played: a room, a table, a server name.
    #[serde(default)]
    pub venue: Option<String>,
    pub next_win: Option<Dest>,
    pub next_lose: Option<Dest>,
}

impl Match {
    /// True once a person decided the match, by score or by forfeit. Byes resolve on their own
    /// and do not count.
    pub fn is_played(&self) -> bool {
        self.score.is_some() || self.forfeit
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.slots, [Slot::Team(_), Slot::Team(_)])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bracket {
    pub format: Format,
    pub matches: Vec<Match>,
    /// Organizer decided order for teams that tie on points, difference and goals. Teams
    /// listed earlier rank higher; teams not listed rank after them, lowest id first.
    #[serde(default)]
    pub tiebreak: Vec<TeamId>,
}

/// One team's line in a round robin table. A win is worth 3 points, a draw 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    pub team: TeamId,
    pub played: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub points_for: u32,
    pub points_against: u32,
    pub points: u32,
}

fn row_index(table: &mut Vec<Standing>, team: TeamId) -> usize {
    if let Some(i) = table.iter().position(|s| s.team == team) {
        return i;
    }
    table.push(Standing {
        team,
        played: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        points_for: 0,
        points_against: 0,
        points: 0,
    });
    table.len() - 1
}

impl Bracket {
    /// Position of a team in the organizer's tiebreak list, `usize::MAX` when it is not listed.
    pub(crate) fn tie_rank(&self, team: TeamId) -> usize {
        self.tiebreak
            .iter()
            .position(|&t| t == team)
            .unwrap_or(usize::MAX)
    }

    /// Round robin table, best first: points, then goal difference, then goals scored, then
    /// the lower team id (teams are numbered in seed order).
    pub fn standings(&self) -> Vec<Standing> {
        self.group_standings(None)
    }

    /// Number of groups in a groups then knockout tournament, zero otherwise.
    pub fn group_count(&self) -> u32 {
        self.matches
            .iter()
            .filter_map(|m| m.group)
            .max()
            .map_or(0, |g| g + 1)
    }

    /// Table for one group (or for the whole league when `group` is `None`).
    pub fn group_standings(&self, group: Option<u32>) -> Vec<Standing> {
        let mut table: Vec<Standing> = Vec::new();
        for m in self
            .matches
            .iter()
            .filter(|m| m.stage == Stage::League && m.group == group)
        {
            let [Slot::Team(a), Slot::Team(b)] = m.slots else {
                continue;
            };
            let (ia, ib) = (row_index(&mut table, a), row_index(&mut table, b));
            if !m.is_played() {
                continue;
            }
            let [sa, sb] = m.score.unwrap_or([0, 0]);
            for (index, own, other) in [(ia, sa, sb), (ib, sb, sa)] {
                let side = if index == ia { Side::A } else { Side::B };
                let row = &mut table[index];
                row.played += 1;
                row.points_for += own;
                row.points_against += other;
                match m.winner {
                    Some(w) if w == side => {
                        row.wins += 1;
                        row.points += 3;
                    }
                    Some(_) => row.losses += 1,
                    None => {
                        row.draws += 1;
                        row.points += 1;
                    }
                }
            }
        }
        table.sort_by_key(|s| {
            (
                std::cmp::Reverse(s.points),
                std::cmp::Reverse(i64::from(s.points_for) - i64::from(s.points_against)),
                std::cmp::Reverse(s.points_for),
                self.tie_rank(s.team),
                s.team,
            )
        });
        table
    }

    /// (played, total) over matches people actually play: byes and a void reset are skipped.
    pub fn progress(&self) -> (usize, usize) {
        let real = self
            .matches
            .iter()
            .filter(|m| !m.slots.contains(&Slot::Bye));
        real.fold((0, 0), |(played, total), m| {
            (played + usize::from(m.is_played()), total + 1)
        })
    }

    pub fn champion(&self) -> Option<TeamId> {
        let slot_of = |m: &Match| m.winner.map(|s| m.slots[s.index()]);
        if self.format == Format::RoundRobin {
            let all_played = !self.matches.is_empty() && self.matches.iter().all(Match::is_played);
            return all_played
                .then(|| self.standings().first().map(|s| s.team))
                .flatten();
        }
        let winner = match self.format {
            Format::SingleElimination | Format::GroupKnockout => {
                slot_of(self.matches.iter().rfind(|m| m.stage == Stage::Winners)?)
            }
            Format::RoundRobin => None,
            Format::DoubleElimination => {
                let n = self.matches.len();
                let (grand, reset) = (&self.matches[n - 2], &self.matches[n - 1]);
                if reset.is_ready() {
                    slot_of(reset)
                } else {
                    slot_of(grand).filter(|_| grand.winner == Some(Side::A))
                }
            }
        };
        match winner {
            Some(Slot::Team(id)) => Some(id),
            _ => None,
        }
    }
}
