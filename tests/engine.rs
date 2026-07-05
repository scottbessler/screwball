use chrono::Utc;
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use screwball::dict::Dictionary;
use screwball::game::{MoveError, SeatSpec, apply_move, new_game, validate_play};
use screwball::models::{
    Board, Game, GameStatus, MoveKind, Placement, Position, SeatKind, Tile, WordRule, is_jax_name,
    jax_names,
};

fn dict() -> Dictionary {
    Dictionary::from_words("AT\nAS\nTA\nTO\nOAT\nOATS\nCAT\nCATS\nHAT\nHATS\nHA\nAH\nAAAAAAA\n")
}

fn letter(c: char) -> Tile {
    Tile::Letter(c)
}

fn place(row: usize, col: usize, c: char) -> Placement {
    Placement {
        position: Position::new(row, col),
        letter: c,
        is_blank: false,
    }
}

#[test]
fn first_move_scores_center_double_word() {
    let board = Board::new();
    let rack = vec![letter('C'), letter('A'), letter('T'), letter('X')];
    let placements = vec![place(7, 6, 'C'), place(7, 7, 'A'), place(7, 8, 'T')];
    let scored =
        validate_play(&board, &rack, &dict(), &placements, WordRule::Standard).expect("valid play");
    assert_eq!(scored.words.len(), 1);
    assert_eq!(scored.words[0].word, "CAT");
    // C(3)+A(1)+T(1) = 5, doubled by the center star = 10.
    assert_eq!(scored.points, 10);
}

#[test]
fn first_move_must_cover_center() {
    let board = Board::new();
    let rack = vec![letter('C'), letter('A'), letter('T')];
    let placements = vec![place(0, 0, 'C'), place(0, 1, 'A'), place(0, 2, 'T')];
    let err = validate_play(&board, &rack, &dict(), &placements, WordRule::Standard).unwrap_err();
    assert_eq!(err, MoveError::FirstMoveMustCoverCenter);
}

#[test]
fn rejects_words_not_in_dictionary() {
    let board = Board::new();
    let rack = vec![letter('X'), letter('Y'), letter('Z')];
    let placements = vec![place(7, 6, 'X'), place(7, 7, 'Y'), place(7, 8, 'Z')];
    let err = validate_play(&board, &rack, &dict(), &placements, WordRule::Standard).unwrap_err();
    assert!(matches!(err, MoveError::InvalidWords(_)));
}

#[test]
fn grandpa_mode_rejects_uncommon_two_letter_words() {
    let board = Board::new();
    let rack = vec![letter('H'), letter('A')];
    let placements = vec![place(7, 7, 'H'), place(7, 8, 'A')];
    // "HA" is a valid dictionary word but not on Grandpa's allowlist.
    validate_play(&board, &rack, &dict(), &placements, WordRule::Standard)
        .expect("standard allows HA");
    let err = validate_play(&board, &rack, &dict(), &placements, WordRule::Grandpa).unwrap_err();
    assert_eq!(err, MoveError::DisallowedWords(vec!["HA".to_string()]));
}

#[test]
fn grandpa_mode_allows_common_two_letter_words() {
    let board = Board::new();
    let rack = vec![letter('T'), letter('O')];
    let placements = vec![place(7, 7, 'T'), place(7, 8, 'O')];
    // "TO" is common enough to survive Grandpa Mode.
    validate_play(&board, &rack, &dict(), &placements, WordRule::Grandpa)
        .expect("grandpa allows TO");
}

#[test]
fn jax_mode_allows_top_names_not_in_dictionary() {
    assert_eq!(jax_names().count(), 500);
    assert!(is_jax_name("JAX"));

    let names_without_olivia = Dictionary::from_words("AT\nTO\nCAT\n");
    let board = Board::new();
    let rack = vec![
        letter('O'),
        letter('L'),
        letter('I'),
        letter('V'),
        letter('I'),
        letter('A'),
    ];
    let placements = vec![
        place(7, 4, 'O'),
        place(7, 5, 'L'),
        place(7, 6, 'I'),
        place(7, 7, 'V'),
        place(7, 8, 'I'),
        place(7, 9, 'A'),
    ];

    let err = validate_play(
        &board,
        &rack,
        &names_without_olivia,
        &placements,
        WordRule::Standard,
    )
    .unwrap_err();
    assert_eq!(err, MoveError::InvalidWords(vec!["OLIVIA".to_string()]));

    validate_play(
        &board,
        &rack,
        &names_without_olivia,
        &placements,
        WordRule::Jax,
    )
    .expect("Jax Mode allows common names");
}

#[test]
fn rejects_tiles_not_in_rack() {
    let board = Board::new();
    let rack = vec![letter('C'), letter('A')];
    let placements = vec![place(7, 6, 'C'), place(7, 7, 'A'), place(7, 8, 'T')];
    let err = validate_play(&board, &rack, &dict(), &placements, WordRule::Standard).unwrap_err();
    assert_eq!(err, MoveError::TilesNotInRack);
}

#[test]
fn rejects_non_contiguous_placement() {
    let board = Board::new();
    let rack = vec![letter('C'), letter('A'), letter('T')];
    // Gap at (7,7): tiles at cols 6 and 8 with nothing between.
    let placements = vec![place(7, 6, 'C'), place(7, 8, 'T')];
    let err = validate_play(&board, &rack, &dict(), &placements, WordRule::Standard).unwrap_err();
    assert_eq!(err, MoveError::NotContiguous);
}

#[test]
fn second_move_must_connect() {
    // Seat 0 plays CAT across the center; the board is then non-empty.
    let mut game = game_with(vec![
        (vec![letter('C'), letter('A'), letter('T'), letter('S')], 0),
        (vec![letter('O')], 0),
    ]);
    play_first_cat(&mut game);

    // A HAT placed far away, touching nothing, must be rejected.
    let rack = vec![letter('H'), letter('A'), letter('T')];
    let placements = vec![place(0, 0, 'H'), place(0, 1, 'A'), place(0, 2, 'T')];
    let err =
        validate_play(&game.board, &rack, &dict(), &placements, WordRule::Standard).unwrap_err();
    assert_eq!(err, MoveError::NotConnected);
}

#[test]
fn cross_word_play_scores_both_words() {
    // Place CAT across center, then hang an S to make CATS + a cross word.
    let mut game = game_with(vec![
        (vec![letter('C'), letter('A'), letter('T'), letter('S')], 0),
        (vec![letter('S'), letter('A'), letter('O')], 0),
    ]);
    play_first_cat(&mut game);
    // Seat 1 hangs an S under the center A to form "AS" vertically.
    let placements = vec![place(8, 7, 'S')];
    let scored = validate_play(
        &game.board,
        &game.seats[1].rack,
        &dict(),
        &placements,
        WordRule::Standard,
    )
    .expect("connecting play");
    assert!(scored.words.iter().any(|w| w.word == "AS"));
}

#[test]
fn bingo_awards_fifty_point_bonus() {
    let board = Board::new();
    let rack = vec![
        letter('A'),
        letter('A'),
        letter('A'),
        letter('A'),
        letter('A'),
        letter('A'),
        letter('A'),
    ];
    let placements: Vec<Placement> = (4..=10).map(|col| place(7, col, 'A')).collect();
    let scored = validate_play(&board, &rack, &dict(), &placements, WordRule::Standard)
        .expect("seven-tile play");
    // 7 x A(1) = 7, doubled by center = 14, plus 50 bingo bonus = 64.
    assert_eq!(scored.points, 64);
}

#[test]
fn pass_increments_scoreless_counter() {
    let mut game = game_with(vec![
        (vec![letter('A'), letter('T')], 0),
        (vec![letter('O')], 0),
    ]);
    let mut rng = StdRng::seed_from_u64(1);
    apply_move(&mut game, &dict(), 0, MoveKind::Pass, &mut rng).unwrap();
    assert_eq!(game.consecutive_scoreless, 1);
    assert_eq!(game.turn, 1);
}

#[test]
fn cannot_play_out_of_turn() {
    let mut game = game_with(vec![
        (vec![letter('A'), letter('T')], 0),
        (vec![letter('O')], 0),
    ]);
    let mut rng = StdRng::seed_from_u64(1);
    let err = apply_move(&mut game, &dict(), 1, MoveKind::Pass, &mut rng).unwrap_err();
    assert_eq!(err, MoveError::NotYourTurn);
}

#[test]
fn cannot_exchange_with_thin_bag() {
    let mut game = game_with(vec![
        (vec![letter('A'), letter('T')], 0),
        (vec![letter('O')], 0),
    ]);
    let mut rng = StdRng::seed_from_u64(1);
    let err = apply_move(
        &mut game,
        &dict(),
        0,
        MoveKind::Exchange {
            tiles: vec![letter('A')],
        },
        &mut rng,
    )
    .unwrap_err();
    assert_eq!(err, MoveError::CannotExchange);
}

#[test]
fn emptying_rack_with_empty_bag_finishes_game() {
    let mut game = game_with(vec![
        (vec![letter('A'), letter('T')], 0),
        (vec![letter('O')], 0),
    ]);
    let mut rng = StdRng::seed_from_u64(1);
    // Seat 0 plays AT across the center, emptying its rack with an empty bag.
    let placements = vec![place(7, 7, 'A'), place(7, 8, 'T')];
    apply_move(
        &mut game,
        &dict(),
        0,
        MoveKind::Play { placements },
        &mut rng,
    )
    .unwrap();
    assert_eq!(game.status, GameStatus::Finished);
    // AT: A on center DW = (1+1)*2 = 4, plus opponent's leftover O(1) = 5.
    assert_eq!(game.seats[0].score, 5);
    assert_eq!(game.seats[1].score, -1);

    // The settlement is recorded in the move log: a +1 out bonus for seat 0
    // and a -1 leftover penalty (the O tile) for seat 1.
    let adjustments: Vec<_> = game
        .moves
        .iter()
        .filter_map(|mv| match &mv.kind {
            MoveKind::EndAdjustment { delta, tiles } => Some((mv.seat, *delta, tiles.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        adjustments,
        vec![(0, 1, vec![]), (1, -1, vec![letter('O')])],
    );
}

#[test]
fn new_game_deals_full_racks() {
    let mut rng = StdRng::seed_from_u64(7);
    let specs = vec![
        SeatSpec {
            kind: SeatKind::Human { user_id: None },
            name: "Alice".into(),
        },
        SeatSpec {
            kind: SeatKind::Bot {
                difficulty: screwball::models::Difficulty::Medium,
            },
            name: "Bot".into(),
        },
    ];
    let game = new_game(specs, false, false, false, false, 0, &mut rng);
    assert_eq!(game.seats.len(), 2);
    assert_eq!(game.seats[0].rack.len(), 7);
    assert_eq!(game.seats[1].rack.len(), 7);
    assert_eq!(game.bag.len(), 100 - 14);
    assert_eq!(game.status, GameStatus::Active);
}

/// Build an active game with explicit racks and an empty bag.
fn game_with(seats: Vec<(Vec<Tile>, i32)>) -> Game {
    let seats = seats
        .into_iter()
        .enumerate()
        .map(|(i, (rack, score))| screwball::models::Seat {
            id: Uuid::new_v4(),
            kind: SeatKind::Human { user_id: None },
            name: format!("Seat {i}"),
            rack,
            score,
        })
        .collect::<Vec<_>>();
    let seat_count = seats.len();
    Game {
        id: Uuid::new_v4(),
        status: GameStatus::Active,
        board: Board::new(),
        bag: Vec::new(),
        seats,
        turn: 0,
        moves: Vec::new(),
        consecutive_scoreless: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        john_mode: false,
        grandpa_mode: false,
        jax_mode: false,
        august_mode: false,
        hints_allowed: 0,
        hints_used: vec![0; seat_count],
    }
}

/// Seat 0 plays CAT across the center (cols 6,7,8). Assumes seat 0 holds C,A,T.
fn play_first_cat(game: &mut Game) {
    let mut rng = StdRng::seed_from_u64(99);
    let placements = vec![place(7, 6, 'C'), place(7, 7, 'A'), place(7, 8, 'T')];
    apply_move(game, &dict(), 0, MoveKind::Play { placements }, &mut rng).expect("first CAT play");
}
