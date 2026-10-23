use crate::error::EngineError;
use crate::model::{Bracket, Dest, MatchId, Side, Slot, Stage};

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
        *self = next;
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
        *self = next;
        Ok(())
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
