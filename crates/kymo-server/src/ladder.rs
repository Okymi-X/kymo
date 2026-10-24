use std::collections::HashMap;

use kymo_engine::{Side, Slot};
use serde::Serialize;

use crate::db::Tournament;

/// Points for a match win, a draw and for winning a whole tournament.
pub const WIN: u32 = 3;
pub const DRAW: u32 = 1;
pub const TITLE: u32 = 10;

/// One team's line in the cross tournament ladder. Teams are the same team when their names
/// match ignoring case.
#[derive(Debug, Serialize)]
pub struct Line {
    pub name: String,
    pub tournaments: u32,
    pub played: u32,
    pub wins: u32,
    pub draws: u32,
    pub titles: u32,
    pub points: u32,
}

/// Adds up every played match and every title across the given tournaments, best first.
pub fn build(all: &[Tournament]) -> Vec<Line> {
    let mut lines: HashMap<String, Line> = HashMap::new();
    for t in all {
        let Some(bracket) = &t.bracket else {
            continue;
        };
        let name_of = |slot: Slot| match slot {
            Slot::Team(id) => t.teams.iter().find(|x| x.id as u32 == id),
            _ => None,
        };
        let mut seen: Vec<String> = Vec::new();
        for team in &t.teams {
            let key = team.name.to_lowercase();
            let l = lines.entry(key.clone()).or_insert_with(|| Line {
                name: team.name.clone(),
                tournaments: 0,
                played: 0,
                wins: 0,
                draws: 0,
                titles: 0,
                points: 0,
            });
            if !seen.contains(&key) {
                seen.push(key);
                l.tournaments += 1;
            }
        }
        for m in bracket.matches.iter().filter(|m| m.is_played()) {
            let (Some(a), Some(b)) = (name_of(m.slots[0]), name_of(m.slots[1])) else {
                continue;
            };
            for (team, side) in [(a, Side::A), (b, Side::B)] {
                let key = team.name.to_lowercase();
                let Some(l) = lines.get_mut(&key) else {
                    continue;
                };
                l.played += 1;
                match m.winner {
                    Some(w) if w == side => {
                        l.wins += 1;
                        l.points += WIN;
                    }
                    None => {
                        l.draws += 1;
                        l.points += DRAW;
                    }
                    Some(_) => {}
                }
            }
        }
        if let Some(champion) = t.champion
            && let Some(team) = t.teams.iter().find(|x| x.id as u32 == champion)
            && let Some(l) = lines.get_mut(&team.name.to_lowercase())
        {
            l.titles += 1;
            l.points += TITLE;
        }
    }
    let mut out: Vec<Line> = lines.into_values().collect();
    out.sort_by(|a, b| {
        b.points
            .cmp(&a.points)
            .then(b.titles.cmp(&a.titles))
            .then(b.wins.cmp(&a.wins))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}
