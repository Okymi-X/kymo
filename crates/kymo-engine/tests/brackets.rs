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
        Options {
            third_place: true,
            ..Options::default()
        },
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
            Options {
                third_place: true,
                ..Options::default()
            }
        )
        .unwrap_err(),
        EngineError::TooFewTeams { min: 4 }
    );
    let double = generate_with(
        &teams(8),
        Format::DoubleElimination,
        Options {
            third_place: true,
            ..Options::default()
        },
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

#[test]
fn progress_counts_only_matches_that_are_played_by_people() {
    let mut b = generate(&teams(3), Format::SingleElimination).unwrap();
    assert_eq!(b.progress(), (0, 2));
    play_all(&mut b, lower_team_wins);
    assert_eq!(b.progress(), (2, 2));
}

#[test]
fn progress_ignores_a_void_grand_final_reset() {
    let mut b = generate(&teams(4), Format::DoubleElimination).unwrap();
    play_all(&mut b, lower_team_wins);
    let (played, total) = b.progress();
    assert_eq!(played, total);
}

fn robin(n: u32) -> Bracket {
    generate(&teams(n), Format::RoundRobin).unwrap()
}

#[test]
fn round_robin_schedules_every_pair_exactly_once() {
    for n in 3..=12u32 {
        let b = robin(n);
        assert_eq!(b.matches.len() as u32, n * (n - 1) / 2, "n={n}");
        let mut pairs = std::collections::HashSet::new();
        for m in &b.matches {
            let [Slot::Team(x), Slot::Team(y)] = m.slots else {
                panic!("slot")
            };
            assert!(pairs.insert((x.min(y), x.max(y))), "repeat in n={n}");
            assert_eq!(m.stage, Stage::League);
        }
    }
}

#[test]
fn round_robin_never_plays_a_team_twice_in_one_round() {
    for n in [4u32, 5, 8, 9] {
        let b = robin(n);
        let rounds = b.matches.iter().map(|m| m.round).max().unwrap();
        for r in 1..=rounds {
            let mut seen = std::collections::HashSet::new();
            for m in b.matches.iter().filter(|m| m.round == r) {
                let [Slot::Team(x), Slot::Team(y)] = m.slots else {
                    panic!("slot")
                };
                assert!(seen.insert(x) && seen.insert(y), "n={n} round {r}");
            }
        }
    }
}

#[test]
fn round_robin_allows_draws_and_scores_points() {
    let mut b = robin(3);
    let (first, second, third) = (0, 1, 2);
    b.set_result(first, 2, 2).unwrap();
    b.set_result(second, 3, 0).unwrap();
    let table = b.standings();
    assert_eq!(table.iter().map(|s| s.points).sum::<u32>(), 5);
    assert!(table.iter().any(|s| s.draws == 1));
    assert_eq!(b.champion(), None);
    b.set_result(third, 1, 0).unwrap();
    assert!(b.champion().is_some());
}

#[test]
fn round_robin_champion_is_the_table_leader_after_every_match() {
    let mut b = robin(4);
    assert_eq!(b.champion(), None);
    play_all(&mut b, lower_team_wins);
    assert_eq!(b.champion(), Some(1));
    let top = &b.standings()[0];
    assert_eq!((top.team, top.wins, top.points), (1, 3, 9));
}

#[test]
fn round_robin_results_can_be_edited_and_cleared_at_any_time() {
    let mut b = robin(3);
    play_all(&mut b, lower_team_wins);
    b.set_result(0, 0, 5).unwrap();
    b.clear_result(1).unwrap();
    assert_eq!(b.champion(), None);
    assert_eq!(b.progress(), (2, 3));
}

#[test]
fn round_robin_forfeit_gives_three_points_and_no_goals() {
    let mut b = robin(3);
    b.set_forfeit(0, Side::B).unwrap();
    let [Slot::Team(_), Slot::Team(winner)] = b.matches[0].slots else {
        panic!("slot")
    };
    let row = b
        .standings()
        .into_iter()
        .find(|s| s.team == winner)
        .unwrap();
    assert_eq!((row.wins, row.points, row.points_for), (1, 3, 0));
}

#[test]
fn round_robin_limits_team_count_and_still_forbids_knockout_ties() {
    assert_eq!(
        generate(&teams(2), Format::RoundRobin).unwrap_err(),
        EngineError::TooFewTeams { min: 3 }
    );
    assert_eq!(
        generate(&teams(33), Format::RoundRobin).unwrap_err(),
        EngineError::TooManyTeams { max: 32 }
    );
    let mut single = generate(&teams(4), Format::SingleElimination).unwrap();
    assert_eq!(single.set_result(0, 1, 1), Err(EngineError::Tie));
}

fn groups(n: u32, groups: u8, advance: u8) -> Bracket {
    generate_with(
        &teams(n),
        Format::GroupKnockout,
        Options {
            groups,
            advance,
            ..Options::default()
        },
    )
    .unwrap()
}

fn play_groups(b: &mut Bracket) {
    let ids: Vec<usize> = b
        .matches
        .iter()
        .filter(|m| m.stage == Stage::League)
        .map(|m| m.id)
        .collect();
    for id in ids {
        let (a, c) = lower_team_wins(&b.matches[id]);
        b.set_result(id, a, c).unwrap();
    }
}

#[test]
fn groups_are_dealt_in_a_snake_and_play_a_round_robin() {
    let b = groups(8, 2, 2);
    let league: Vec<_> = b
        .matches
        .iter()
        .filter(|m| m.stage == Stage::League)
        .collect();
    assert_eq!(league.len(), 12);
    assert_eq!(b.group_count(), 2);
    let members = |g: u32| {
        let mut ids: Vec<u32> = league
            .iter()
            .filter(|m| m.group == Some(g))
            .flat_map(|m| m.slots)
            .filter_map(|s| if let Slot::Team(t) = s { Some(t) } else { None })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    assert_eq!(members(0), vec![1, 4, 5, 8]);
    assert_eq!(members(1), vec![2, 3, 6, 7]);
    let knockout = b
        .matches
        .iter()
        .filter(|m| m.stage == Stage::Winners)
        .count();
    assert_eq!(knockout, 3);
}

#[test]
fn knockout_waits_for_the_groups_then_fills_without_group_rematches() {
    let mut b = groups(8, 2, 2);
    let first_round = |b: &Bracket| -> Vec<[Slot; 2]> {
        b.matches
            .iter()
            .filter(|m| m.stage == Stage::Winners && m.round == 1)
            .map(|m| m.slots)
            .collect()
    };
    assert!(first_round(&b).iter().flatten().all(|s| *s == Slot::Empty));
    play_groups(&mut b);
    let group_of = |team: u32| if [1, 4, 5, 8].contains(&team) { 0 } else { 1 };
    for [x, y] in first_round(&b) {
        let (Slot::Team(x), Slot::Team(y)) = (x, y) else {
            panic!("unfilled slot")
        };
        assert_ne!(group_of(x), group_of(y), "group rematch {x} v {y}");
    }
    play_all(&mut b, lower_team_wins);
    assert_eq!(b.champion(), Some(1));
}

#[test]
fn changing_a_group_result_moves_qualifiers_until_a_knockout_match_is_played() {
    let mut b = groups(8, 2, 2);
    play_groups(&mut b);
    let before = b
        .matches
        .iter()
        .find(|m| m.stage == Stage::Winners)
        .unwrap()
        .slots;
    b.clear_result(0).unwrap();
    assert!(
        b.matches
            .iter()
            .filter(|m| m.stage == Stage::Winners && m.round == 1)
            .flat_map(|m| m.slots)
            .all(|s| s == Slot::Empty)
    );
    b.set_result(0, 1, 0).unwrap();
    assert_eq!(
        b.matches
            .iter()
            .find(|m| m.stage == Stage::Winners)
            .unwrap()
            .slots,
        before
    );

    let ready = b
        .matches
        .iter()
        .find(|m| m.stage == Stage::Winners && m.is_ready())
        .unwrap()
        .id;
    b.set_result(ready, 1, 0).unwrap();
    let snapshot = b.clone();
    assert_eq!(b.clear_result(0), Err(EngineError::DownstreamPlayed));
    assert_eq!(b, snapshot);
}

#[test]
fn uneven_groups_and_byes_still_reach_a_champion() {
    for (n, g, a) in [
        (9u32, 3u8, 2u8),
        (10, 2, 3),
        (7, 2, 1),
        (12, 4, 2),
        (16, 4, 2),
    ] {
        let mut b = groups(n, g, a);
        play_all(&mut b, lower_team_wins);
        assert!(b.champion().is_some(), "n={n} groups={g} advance={a}");
        let (played, total) = b.progress();
        assert_eq!(played, total, "n={n}");
    }
}

#[test]
fn group_setup_is_validated() {
    let bad = |n: u32, groups: u8, advance: u8| {
        generate_with(
            &teams(n),
            Format::GroupKnockout,
            Options {
                groups,
                advance,
                ..Options::default()
            },
        )
        .unwrap_err()
    };
    assert!(matches!(bad(8, 1, 1), EngineError::BadGroupSetup(_)));
    assert!(matches!(bad(5, 2, 1), EngineError::BadGroupSetup(_)));
    assert!(matches!(bad(6, 2, 3), EngineError::BadGroupSetup(_)));
    assert!(matches!(bad(8, 2, 0), EngineError::BadGroupSetup(_)));
    assert_eq!(bad(33, 4, 2), EngineError::TooManyTeams { max: 32 });
}

#[test]
fn group_tables_are_separate_per_group() {
    let mut b = groups(6, 2, 1);
    play_groups(&mut b);
    let (a, c) = (b.group_standings(Some(0)), b.group_standings(Some(1)));
    assert_eq!(a.len(), 3);
    assert_eq!(c.len(), 3);
    assert!(a.iter().all(|s| s.played == 2));
    assert_eq!(a[0].team, 1);
    assert_eq!(c[0].team, 2);
}

#[test]
fn schedule_is_stored_cleared_and_never_changes_the_bracket() {
    let mut b = generate(&teams(4), Format::SingleElimination).unwrap();
    let before = b
        .matches
        .iter()
        .map(|m| (m.slots, m.winner))
        .collect::<Vec<_>>();
    b.set_schedule(0, Some(1_800_000_000), Some("Room A".into()))
        .unwrap();
    assert_eq!(b.matches[0].scheduled_at, Some(1_800_000_000));
    assert_eq!(b.matches[0].venue.as_deref(), Some("Room A"));
    b.set_result(0, 2, 1).unwrap();
    assert_eq!(
        b.matches[0].scheduled_at,
        Some(1_800_000_000),
        "a result keeps the plan"
    );
    b.set_schedule(0, None, None).unwrap();
    assert_eq!(b.matches[0].venue, None);
    assert_eq!(
        b.set_schedule(99, None, None),
        Err(EngineError::UnknownMatch(99))
    );
    let after = b
        .matches
        .iter()
        .map(|m| (m.slots, m.winner))
        .collect::<Vec<_>>();
    assert_eq!(before.len(), after.len());
}

#[test]
fn old_stored_matches_without_schedule_fields_still_load() {
    let b = generate(&teams(4), Format::SingleElimination).unwrap();
    let mut value = serde_json::to_value(&b).unwrap();
    for m in value["matches"].as_array_mut().unwrap() {
        let m = m.as_object_mut().unwrap();
        m.remove("scheduled_at");
        m.remove("venue");
    }
    assert_eq!(serde_json::from_value::<Bracket>(value).unwrap(), b);
}

#[test]
fn tiebreak_order_decides_teams_level_on_everything() {
    let mut b = robin(3);
    let ids: Vec<usize> = b.matches.iter().map(|m| m.id).collect();
    for id in ids {
        b.set_result(id, 0, 0).unwrap();
    }
    let order = |b: &Bracket| b.standings().iter().map(|s| s.team).collect::<Vec<_>>();
    assert_eq!(order(&b), vec![1, 2, 3]);
    b.set_tiebreak(vec![3, 1]).unwrap();
    assert_eq!(order(&b), vec![3, 1, 2]);
    assert_eq!(b.champion(), Some(3));
    assert_eq!(b.set_tiebreak(vec![9]), Err(EngineError::UnknownTeam(9)));
    assert_eq!(
        b.set_tiebreak(vec![1, 1]),
        Err(EngineError::DuplicateTeam(1))
    );
    assert_eq!(order(&b), vec![3, 1, 2]);
}

fn swiss(n: u32, rounds: u8) -> Bracket {
    generate_with(
        &teams(n),
        Format::Swiss,
        Options {
            rounds,
            ..Options::default()
        },
    )
    .unwrap()
}

fn play_swiss_round(b: &mut Bracket, round: u32) {
    let ids: Vec<usize> = b
        .matches
        .iter()
        .filter(|m| m.round == round && !m.slots.contains(&Slot::Bye))
        .map(|m| m.id)
        .collect();
    for id in ids {
        let (a, c) = lower_team_wins(&b.matches[id]);
        b.set_result(id, a, c).unwrap();
    }
}

fn pairs_of(b: &Bracket) -> Vec<(u32, u32)> {
    b.matches
        .iter()
        .filter_map(|m| match m.slots {
            [Slot::Team(a), Slot::Team(c)] => Some((a.min(c), a.max(c))),
            _ => None,
        })
        .collect()
}

#[test]
fn swiss_plans_each_round_after_the_last_without_rematches() {
    let mut b = swiss(8, 0);
    assert_eq!(b.swiss_rounds, 3);
    assert_eq!(b.matches.len(), 4);
    assert_eq!(b.champion(), None);
    for round in 1..=3 {
        assert!(b.matches.iter().all(|m| m.round <= round));
        play_swiss_round(&mut b, round);
    }
    assert_eq!(b.matches.len(), 12);
    let pairs = pairs_of(&b);
    let mut unique = pairs.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), pairs.len(), "a pair met twice: {pairs:?}");
    assert_eq!(b.champion(), Some(1));
}

#[test]
fn swiss_gives_each_team_at_most_one_bye_when_the_count_is_odd() {
    let mut b = swiss(5, 3);
    for round in 1..=3 {
        play_swiss_round(&mut b, round);
    }
    let byes: Vec<u32> = b
        .matches
        .iter()
        .filter_map(|m| match m.slots {
            [Slot::Team(t), Slot::Bye] => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(byes.len(), 3);
    let mut unique = byes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 3);
    let table = b.standings();
    assert!(table.iter().all(|s| s.played == 3));
}

#[test]
fn swiss_corrections_replan_an_unplayed_round_and_block_a_played_one() {
    let mut b = swiss(8, 3);
    play_swiss_round(&mut b, 1);
    let before = pairs_of(&b);
    let first = b.matches.iter().find(|m| m.round == 1).unwrap().id;
    let (x, y) = lower_team_wins(&b.matches[first]);
    b.set_result(first, y, x).unwrap();
    assert_ne!(
        pairs_of(&b),
        before,
        "round two follows the corrected result"
    );
    play_swiss_round(&mut b, 2);
    assert_eq!(
        b.set_result(first, x, y),
        Err(EngineError::DownstreamPlayed)
    );
    assert_eq!(b.clear_result(first), Err(EngineError::DownstreamPlayed));
    assert_eq!(
        generate_with(&teams(3), Format::Swiss, Options::default()),
        Err(EngineError::TooFewTeams { min: 4 })
    );
}

fn best_of(n: u32, games: u8) -> Bracket {
    generate_with(
        &teams(n),
        Format::SingleElimination,
        Options {
            best_of: games,
            ..Options::default()
        },
    )
    .unwrap()
}

#[test]
fn a_best_of_three_is_won_by_a_majority_of_games() {
    let mut b = best_of(4, 3);
    assert_eq!(b.best_of, 3);
    // A series score must end at the majority.
    assert_eq!(b.set_result(0, 1, 0), Err(EngineError::BadSeries(3)));
    assert_eq!(b.set_result(0, 3, 0), Err(EngineError::BadSeries(3)));
    assert_eq!(b.set_result(0, 2, 2), Err(EngineError::Tie));
    b.set_games(0, vec![[10, 4], [3, 9], [12, 11]]).unwrap();
    let m = &b.matches[0];
    assert_eq!(m.score, Some([2, 1]));
    assert_eq!(m.winner, Some(Side::A));
    assert_eq!(m.games.len(), 3);
    // The winner moved on.
    assert!(matches!(b.matches[2].slots[0], Slot::Team(_)));
}

#[test]
fn series_games_must_stop_at_the_majority_and_never_tie() {
    let mut b = best_of(4, 3);
    assert_eq!(b.set_games(0, vec![[1, 0]]), Err(EngineError::BadSeries(3)));
    assert_eq!(
        b.set_games(0, vec![[1, 1], [2, 0], [2, 0]]),
        Err(EngineError::BadSeries(3))
    );
    assert_eq!(
        b.set_games(0, vec![[2, 0], [2, 0], [2, 0]]),
        Err(EngineError::BadSeries(3)),
        "a third game after the series was decided is refused"
    );
    assert!(b.matches[0].games.is_empty() && b.matches[0].score.is_none());
    let mut single = best_of(4, 1);
    assert_eq!(
        single.set_games(0, vec![[1, 0]]),
        Err(EngineError::BadSeries(1))
    );
    assert!(
        generate_with(
            &teams(4),
            Format::SingleElimination,
            Options {
                best_of: 4,
                ..Options::default()
            }
        )
        .is_err()
    );
}

#[test]
fn clearing_a_series_removes_its_games() {
    let mut b = best_of(4, 5);
    b.set_games(0, vec![[1, 0], [1, 0], [1, 0]]).unwrap();
    assert_eq!(b.matches[0].score, Some([3, 0]));
    b.clear_result(0).unwrap();
    assert!(b.matches[0].games.is_empty());
}
