use serde::{Deserialize, Serialize};

pub type TeamId = u32;
pub type MatchId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    SingleElimination,
    DoubleElimination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Winners,
    Losers,
    GrandFinal,
    GrandReset,
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
    pub next_win: Option<Dest>,
    pub next_lose: Option<Dest>,
}

impl Match {
    pub fn is_ready(&self) -> bool {
        matches!(self.slots, [Slot::Team(_), Slot::Team(_)])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bracket {
    pub format: Format,
    pub matches: Vec<Match>,
}

impl Bracket {
    pub fn champion(&self) -> Option<TeamId> {
        let slot_of = |m: &Match| m.winner.map(|s| m.slots[s.index()]);
        let winner = match self.format {
            Format::SingleElimination => slot_of(self.matches.last()?),
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
