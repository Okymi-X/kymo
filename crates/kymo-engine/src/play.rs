use crate::error::EngineError;
use crate::generate::seed_order;
use crate::model::{Bracket, Dest, Format, MatchId, Side, Slot, Stage, TeamId};

fn auto_winner(slots: [Slot; 2]) -> Option<Side> {
    match slots {
        [Slot::Bye, Slot::Bye] | [Slot::Team(_), Slot::Bye] => Some(Side::A),
        [Slot::Bye, Slot::Team(_)] => Some(Side::B),
        _ => None,
    }
}

impl Bracket {
    /// Records a result. The bracket is left untouched when this returns an error.
    pub fn set_result(&mut self, id: MatchId, a: u32, b: u32) -> Result<(), EngineError> {
        let mut next = self.clone();
        let m = next
            .matches
            .get_mut(id)
            .ok_or(EngineError::UnknownMatch(id))?;
        if !m.is_ready() {
            return Err(EngineError::NotReady(id));
        }
        if a == b && m.stage != Stage::League {
            return Err(EngineError::Tie);
        }
        m.score = Some([a, b]);
        m.forfeit = false;
        m.winner = match a.cmp(&b) {
            std::cmp::Ordering::Greater => Some(Side::A),
            std::cmp::Ordering::Less => Some(Side::B),
            std::cmp::Ordering::Equal => None,
        };
        next.settle(id)?;
        next.refresh_knockout()?;
        *self = next;
        Ok(())
    }

    /// Plans when and where a match is played. Passing `None` for both clears the plan.
    pub fn set_schedule(
        &mut self,
        id: MatchId,
        at: Option<i64>,
        venue: Option<String>,
    ) -> Result<(), EngineError> {
        let m = self
            .matches
            .get_mut(id)
            .ok_or(EngineError::UnknownMatch(id))?;
        m.scheduled_at = at;
        m.venue = venue;
        Ok(())
    }

    /// Awards a ready match to one side without a score.
    pub fn set_forfeit(&mut self, id: MatchId, winner: Side) -> Result<(), EngineError> {
        let mut next = self.clone();
        let m = next
            .matches
            .get_mut(id)
            .ok_or(EngineError::UnknownMatch(id))?;
        if !m.is_ready() {
            return Err(EngineError::NotReady(id));
        }
        m.score = None;
        m.forfeit = true;
        m.winner = Some(winner);
        next.settle(id)?;
        next.refresh_knockout()?;
        *self = next;
        Ok(())
    }

    /// Sets the order that breaks ties between teams level on points, difference and goals.
    /// Every id must belong to a team in this bracket. Qualifiers of a groups tournament are
    /// re-seeded, which fails once a knockout match depends on the old order.
    pub fn set_tiebreak(&mut self, order: Vec<TeamId>) -> Result<(), EngineError> {
        let known: Vec<TeamId> = self
            .matches
            .iter()
            .flat_map(|m| m.slots)
            .filter_map(|s| if let Slot::Team(t) = s { Some(t) } else { None })
            .collect();
        for (i, team) in order.iter().enumerate() {
            if !known.contains(team) {
                return Err(EngineError::UnknownTeam(*team));
            }
            if order[..i].contains(team) {
                return Err(EngineError::DuplicateTeam(*team));
            }
        }
        let mut next = self.clone();
        next.tiebreak = order;
        next.refresh_knockout()?;
        *self = next;
        Ok(())
    }

    /// Removes a recorded result. Fails if a later match was already played.
    pub fn clear_result(&mut self, id: MatchId) -> Result<(), EngineError> {
        let mut next = self.clone();
        let m = next
            .matches
            .get_mut(id)
            .ok_or(EngineError::UnknownMatch(id))?;
        if !m.is_played() {
            return Err(EngineError::NoResult(id));
        }
        m.score = None;
        m.forfeit = false;
        m.winner = None;
        next.settle(id)?;
        next.refresh_knockout()?;
        *self = next;
        Ok(())
    }

    /// Groups then knockout: once every group match is played, the qualifiers are placed in
    /// the first knockout round. Before that, and whenever a group result changes, the slots
    /// follow the table. A knockout match that was already played blocks the change.
    pub(crate) fn refresh_knockout(&mut self) -> Result<(), EngineError> {
        if self.format != Format::GroupKnockout {
            return Ok(());
        }
        let league: Vec<_> = self
            .matches
            .iter()
            .filter(|m| m.stage == Stage::League)
            .collect();
        let done = !league.is_empty() && league.iter().all(|m| m.is_played());
        let first_round: Vec<MatchId> = self
            .matches
            .iter()
            .filter(|m| m.stage == Stage::Winners && m.round == 1)
            .map(|m| m.id)
            .collect();
        let order = seed_order(first_round.len() * 2);
        let qualifiers = first_round
            .iter()
            .flat_map(|&id| self.matches[id].slots)
            .filter(|s| *s != Slot::Bye)
            .count();
        let seeded = if done {
            self.knockout_seeds(qualifiers, &order)
        } else {
            Vec::new()
        };
        for (i, &id) in first_round.iter().enumerate() {
            for side in [Side::A, Side::B] {
                if self.matches[id].slots[side.index()] == Slot::Bye {
                    continue;
                }
                let seed = order[2 * i + side.index()];
                let slot = seeded.get(seed - 1).map_or(Slot::Empty, |t| Slot::Team(*t));
                self.set_slot(Dest { id, side }, slot)?;
            }
        }
        Ok(())
    }

    /// Qualifiers ordered by group placement, then points, goal difference and goals scored.
    /// Same group teams are moved apart in the first round when a swap allows it.
    fn knockout_seeds(&self, qualifiers: usize, order: &[usize]) -> Vec<TeamId> {
        let groups = self.group_count() as usize;
        let advance = qualifiers / groups;
        let mut ranked = Vec::new();
        for g in 0..groups {
            for (place, line) in self
                .group_standings(Some(g as u32))
                .into_iter()
                .take(advance)
                .enumerate()
            {
                ranked.push((place, line, g));
            }
        }
        ranked.sort_by_key(|(place, line, _)| {
            (
                *place,
                std::cmp::Reverse(line.points),
                std::cmp::Reverse(i64::from(line.points_for) - i64::from(line.points_against)),
                std::cmp::Reverse(line.points_for),
                self.tie_rank(line.team),
                line.team,
            )
        });
        let mut seeds: Vec<(TeamId, usize)> =
            ranked.iter().map(|(_, line, g)| (line.team, *g)).collect();
        separate_groups(&mut seeds, order);
        seeds.into_iter().map(|(team, _)| team).collect()
    }

    pub(crate) fn settle(&mut self, id: MatchId) -> Result<(), EngineError> {
        let m = &mut self.matches[id];
        if !m.is_played() {
            m.winner = auto_winner(m.slots);
        }
        for (dest, slot) in self.route(id) {
            self.set_slot(dest, slot)?;
        }
        Ok(())
    }

    fn set_slot(&mut self, dest: Dest, slot: Slot) -> Result<(), EngineError> {
        let target = &mut self.matches[dest.id];
        if target.slots[dest.side.index()] == slot {
            return Ok(());
        }
        if target.is_played() {
            return Err(EngineError::DownstreamPlayed);
        }
        target.slots[dest.side.index()] = slot;
        self.settle(dest.id)
    }

    fn route(&self, id: MatchId) -> Vec<(Dest, Slot)> {
        let m = &self.matches[id];
        if m.stage == Stage::GrandFinal {
            return self.route_grand_final(id);
        }
        let (won, lost) = match m.winner {
            Some(side) => (m.slots[side.index()], m.slots[side.other().index()]),
            None => (Slot::Empty, Slot::Empty),
        };
        [(m.next_win, won), (m.next_lose, lost)]
            .into_iter()
            .filter_map(|(dest, slot)| dest.map(|d| (d, slot)))
            .collect()
    }

    /// The reset match is only played when the losers bracket champion wins the grand final.
    fn route_grand_final(&self, id: MatchId) -> Vec<(Dest, Slot)> {
        let m = &self.matches[id];
        let Some(reset) = m.next_win else {
            return Vec::new();
        };
        let [a, b] = match m.winner {
            Some(Side::B) => m.slots,
            Some(Side::A) => [Slot::Bye; 2],
            None => [Slot::Empty; 2],
        };
        vec![
            (
                Dest {
                    id: reset.id,
                    side: Side::A,
                },
                a,
            ),
            (
                Dest {
                    id: reset.id,
                    side: Side::B,
                },
                b,
            ),
        ]
    }
}

/// Swaps the lower seed of a same group first round pair with another pair's lower seed when
/// that leaves both pairs with teams from different groups.
fn separate_groups(seeds: &mut [(TeamId, usize)], order: &[usize]) {
    let pairs = order.len() / 2;
    let seed_at = |pair: usize, side: usize| order[2 * pair + side] - 1;
    for i in 0..pairs {
        let (a, b) = (seed_at(i, 0), seed_at(i, 1));
        if a >= seeds.len() || b >= seeds.len() || seeds[a].1 != seeds[b].1 {
            continue;
        }
        for j in 0..pairs {
            let (c, d) = (seed_at(j, 0), seed_at(j, 1));
            if j == i || d >= seeds.len() || c >= seeds.len() {
                continue;
            }
            if seeds[a].1 != seeds[d].1 && seeds[c].1 != seeds[b].1 {
                seeds.swap(b, d);
                break;
            }
        }
    }
}
