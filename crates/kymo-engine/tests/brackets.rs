use kymo_engine::{
    Bracket, EngineError, Format, Options, Side, Slot, Stage, generate, generate_with,
};

fn teams(n: u32) -> Vec<u32> {
    (1..=n).collect()
}

/// Plays every ready, unplayed match with side A winning (the better seed in round one).
fn play_all(bracket: &mut Bracket, pick: impl Fn(&kymo_engine::Match) -> (u32, u32)) {
    loop {
        let next = bracket
            .matches
            .iter()
            .find(|m| m.is_ready() && m.score.is_none())
            .map(|m| (m.id, pick(m)));
        match next {
            Some((id, (a, b))) => bracket.set_result(id, a, b).unwrap(),
            None => break,
        }
    }
}

fn lower_team_wins(m: &kymo_engine::Match) -> (u32, u32) {
    match m.slots {
        [Slot::Team(a), Slot::Team(b)] if a < b => (1, 0),
        _ => (0, 1),
    }
}

#[test]
fn single_8_has_7_matches_and_standard_pairings() {
    let b = generate(&teams(8), Format::SingleElimination).unwrap();
    assert_eq!(b.matches.len(), 7);
    let pairs: Vec<_> = b.matches[..4].iter().map(|m| m.slots).collect();
    assert_eq!(pairs[0], [Slot::Team(1), Slot::Team(8)]);
    assert_eq!(pairs[1], [Slot::Team(4), Slot::Team(5)]);
    assert_eq!(pairs[2], [Slot::Team(2), Slot::Team(7)]);
    assert_eq!(pairs[3], [Slot::Team(3), Slot::Team(6)]);
}

#[test]
fn single_with_byes_advances_top_seeds() {
    let b = generate(&teams(5), Format::SingleElimination).unwrap();
    let round2: Vec<_> = b.matches.iter().filter(|m| m.round == 2).collect();
    assert_eq!(round2[0].slots[0], Slot::Team(1));
    assert_eq!(round2[1].slots, [Slot::Team(2), Slot::Team(3)]);
    assert!(b.matches[0].score.is_none());
    assert_eq!(b.matches[0].winner, Some(Side::A));
}

#[test]
fn single_plays_to_champion_for_many_sizes() {
    for n in 2..=40 {
        let mut b = generate(&teams(n), Format::SingleElimination).unwrap();
        play_all(&mut b, lower_team_wins);
        assert_eq!(b.champion(), Some(1), "n={n}");
    }
}

#[test]
fn double_8_has_15_matches() {
    let b = generate(&teams(8), Format::DoubleElimination).unwrap();
    assert_eq!(b.matches.len(), 15);
    assert_eq!(
        b.matches
            .iter()
            .filter(|m| m.stage == Stage::Losers)
            .count(),
        6
    );
}

#[test]
fn double_winners_champion_skips_reset() {
    for n in 3..=33 {
        let mut b = generate(&teams(n), Format::DoubleElimination).unwrap();
        play_all(&mut b, lower_team_wins);
        assert_eq!(b.champion(), Some(1), "n={n}");
        assert!(!b.matches.last().unwrap().is_ready());
    }
}

#[test]
fn double_losers_champion_forces_reset() {
    let mut b = generate(&teams(8), Format::DoubleElimination).unwrap();
    play_all(&mut b, |m| match (m.stage, m.slots) {
        (Stage::GrandFinal, _) => (0, 1),
        (Stage::GrandReset, [Slot::Team(a), Slot::Team(b)]) if a > b => (1, 0),
        (_, [Slot::Team(a), Slot::Team(b)]) if a < b => (1, 0),
        _ => (0, 1),
    });
    let reset = b.matches.last().unwrap();
    assert!(reset.is_ready());
    assert!(b.champion().is_some());
}

#[test]
fn rejects_bad_input() {
    assert_eq!(
        generate(&[1], Format::SingleElimination).unwrap_err(),
        EngineError::TooFewTeams { min: 2 }
    );
    assert_eq!(
        generate(&[1, 1], Format::SingleElimination).unwrap_err(),
        EngineError::DuplicateTeam(1)
    );
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    assert_eq!(b.set_result(0, 2, 2), Err(EngineError::Tie));
    assert_eq!(b.set_result(2, 1, 0), Err(EngineError::NotReady(2)));
}

#[test]
fn edit_changes_downstream_until_it_is_played() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    b.set_result(0, 3, 1).unwrap();
    assert_eq!(b.matches[2].slots[0], Slot::Team(1));
    b.set_result(0, 1, 3).unwrap();
    assert_eq!(b.matches[2].slots[0], Slot::Team(4));

    b.set_result(1, 2, 0).unwrap();
    b.set_result(2, 5, 1).unwrap();
    let before = b.clone();
    assert_eq!(b.set_result(0, 3, 1), Err(EngineError::DownstreamPlayed));
    assert_eq!(b, before);
    b.set_result(0, 1, 7).unwrap();
    assert_eq!(b.matches[0].score, Some([1, 7]));
}

#[test]
fn undo_clears_downstream_slots() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    b.set_result(0, 3, 1).unwrap();
    b.clear_result(0).unwrap();
    assert_eq!(b.matches[2].slots[0], Slot::Empty);
    assert_eq!(b.clear_result(0), Err(EngineError::NoResult(0)));
}

#[test]
fn double_loser_drops_to_losers_bracket() {
    let mut b = generate(&teams(4), Format::DoubleElimination).unwrap();
    b.set_result(0, 0, 1).unwrap();
    let lb = b
        .matches
        .iter()
        .find(|m| m.stage == Stage::Losers && m.round == 1)
        .unwrap();
    assert_eq!(lb.slots[0], Slot::Team(1));
}

#[test]
fn bracket_round_trips_through_json() {
    let b = generate(&teams(6), Format::DoubleElimination).unwrap();
    let json = serde_json::to_string(&b).unwrap();
    assert_eq!(serde_json::from_str::<Bracket>(&json).unwrap(), b);
}

#[test]
fn forfeit_advances_the_chosen_side_without_a_score() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    b.set_forfeit(0, Side::B).unwrap();
    assert_eq!(b.matches[0].score, None);
    assert!(b.matches[0].forfeit);
    assert_eq!(b.matches[2].slots[0], Slot::Team(4));
}

#[test]
fn forfeit_needs_two_teams_and_respects_played_matches_downstream() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    assert_eq!(b.set_forfeit(2, Side::A), Err(EngineError::NotReady(2)));
    b.set_forfeit(0, Side::A).unwrap();
    b.set_result(1, 1, 0).unwrap();
    b.set_result(2, 3, 1).unwrap();
    assert_eq!(
        b.set_forfeit(0, Side::B),
        Err(EngineError::DownstreamPlayed)
    );
}

#[test]
fn forfeit_can_be_cleared_and_replaced_by_a_score() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    b.set_forfeit(0, Side::A).unwrap();
    b.set_result(0, 0, 2).unwrap();
    assert!(!b.matches[0].forfeit);
    assert_eq!(b.matches[2].slots[0], Slot::Team(4));
    b.set_forfeit(0, Side::A).unwrap();
    b.clear_result(0).unwrap();
    assert_eq!(b.matches[0].winner, None);
    assert_eq!(b.matches[2].slots[0], Slot::Empty);
}

#[test]
fn old_stored_matches_without_the_forfeit_field_still_load() {
    let b = generate(&teams(4), Format::SingleElimination).unwrap();
    let mut value = serde_json::to_value(&b).unwrap();
    for m in value["matches"].as_array_mut().unwrap() {
        m.as_object_mut().unwrap().remove("forfeit");
    }
    assert_eq!(serde_json::from_value::<Bracket>(value).unwrap(), b);
}

fn with_third_place(n: u32) -> Bracket {
    generate_with(
        &teams(n),
        Format::SingleElimination,
        Options { third_place: true },
    )
    .unwrap()
}

#[test]
fn third_place_match_receives_the_semifinal_losers() {
    let mut b = with_third_place(4);
    assert_eq!(b.matches.len(), 4);
    b.set_result(0, 3, 1).unwrap();
    b.set_result(1, 0, 2).unwrap();
    let third = b.matches.last().unwrap();
    assert_eq!(third.stage, Stage::ThirdPlace);
    assert_eq!(third.slots, [Slot::Team(4), Slot::Team(2)]);
}

#[test]
fn champion_ignores_the_third_place_match() {
    let mut b = with_third_place(8);
    play_all(&mut b, lower_team_wins);
    assert_eq!(b.champion(), Some(1));
    let third = b.matches.last().unwrap();
    assert_eq!(
        third.winner.map(|s| third.slots[s.index()]),
        Some(Slot::Team(3))
    );
}

#[test]
fn champion_is_known_before_third_place_is_played() {
    let mut b = with_third_place(4);
    b.set_result(0, 1, 0).unwrap();
    b.set_result(1, 1, 0).unwrap();
    b.set_result(2, 1, 0).unwrap();
    assert_eq!(b.champion(), Some(1));
    assert!(b.matches[3].score.is_none());
}

#[test]
fn third_place_needs_four_teams_and_is_ignored_for_double_elimination() {
    assert_eq!(
        generate_with(
            &teams(3),
            Format::SingleElimination,
            Options { third_place: true }
        )
        .unwrap_err(),
        EngineError::TooFewTeams { min: 4 }
    );
    let double = generate_with(
        &teams(8),
        Format::DoubleElimination,
        Options { third_place: true },
    )
    .unwrap();
    assert!(double.matches.iter().all(|m| m.stage != Stage::ThirdPlace));
}

#[test]
fn changing_a_semifinal_result_updates_the_third_place_slots() {
    let mut b = with_third_place(4);
    b.set_result(0, 3, 1).unwrap();
    b.set_result(0, 1, 3).unwrap();
    assert_eq!(b.matches[3].slots[0], Slot::Team(1));
}
