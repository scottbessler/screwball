use chrono::Utc;
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use screwball::bot;
use screwball::dict::Dictionary;
use screwball::game::validate_play;
use screwball::models::{Board, Difficulty, Game, GameStatus, MoveKind, Seat, SeatKind, Tile, WordRule};

fn dict() -> Dictionary {
    Dictionary::from_words(
        "AT\nAS\nTA\nTO\nOAT\nOATS\nCAT\nCATS\nCASA\nCOAT\nCOATS\nCOST\nCOTS\nSO\nOS\nTOE\nTOES\nSCAT\n",
    )
}

fn rack(letters: &str) -> Vec<Tile> {
    letters.chars().map(Tile::Letter).collect()
}

fn bot_game(racks: &[&str], difficulty: Difficulty) -> Game {
    let seats = racks
        .iter()
        .enumerate()
        .map(|(i, letters)| Seat {
            id: Uuid::new_v4(),
            kind: SeatKind::Bot { difficulty },
            name: format!("Bot {i}"),
            rack: rack(letters),
            score: 0,
        })
        .collect();
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
        hints_allowed: 0,
        hints_used: vec![0; racks.len()],
    }
}

#[test]
fn generates_first_move_through_center() {
    let board = Board::new();
    let plays = bot::scored_plays(&board, &rack("COATSXY"), &dict(), WordRule::Standard);
    assert!(!plays.is_empty());
    // Every first-move play must cover the center and re-validate cleanly.
    for (placements, _) in &plays {
        assert!(validate_play(&board, &rack("COATSXY"), &dict(), placements, WordRule::Standard).is_ok());
    }
}

#[test]
fn hard_bot_picks_a_scoring_play() {
    let game = bot_game(&["COATSXY"], Difficulty::Hard);
    let mut rng = StdRng::seed_from_u64(3);
    let kind = bot::choose_move(&game, &dict(), 0, &mut rng);
    match kind {
        MoveKind::Play { placements } => {
            let scored = validate_play(&game.board, &game.seats[0].rack, &dict(), &placements, WordRule::Standard)
                .expect("bot play is legal");
            assert!(scored.points > 0);
        }
        other => panic!("expected a play, got {other:?}"),
    }
}

#[test]
fn impossible_bot_picks_the_top_play() {
    let game = bot_game(&["COATSXY"], Difficulty::Impossible);
    let best = bot::scored_plays(&game.board, &game.seats[0].rack, &dict(), WordRule::Standard)
        .into_iter()
        .map(|(_, s)| s.points)
        .max()
        .expect("has plays");
    // Impossible is deterministic (index 0), so any seed yields the best.
    for seed in 0..5 {
        let mut rng = StdRng::seed_from_u64(seed);
        let kind = bot::choose_move(&game, &dict(), 0, &mut rng);
        let MoveKind::Play { placements } = kind else {
            panic!("expected a play");
        };
        let scored = validate_play(&game.board, &game.seats[0].rack, &dict(), &placements, WordRule::Standard)
            .expect("legal");
        assert_eq!(scored.points, best);
    }
}

#[test]
fn every_difficulty_produces_a_legal_play() {
    for difficulty in [
        Difficulty::Easy,
        Difficulty::Chill,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Impossible,
    ] {
        let game = bot_game(&["COATSXY"], difficulty);
        for seed in 0..8 {
            let mut rng = StdRng::seed_from_u64(seed);
            let kind = bot::choose_move(&game, &dict(), 0, &mut rng);
            let MoveKind::Play { placements } = kind else {
                panic!("expected a play for {difficulty:?}");
            };
            validate_play(&game.board, &game.seats[0].rack, &dict(), &placements, WordRule::Standard)
                .unwrap_or_else(|_| panic!("legal play for {difficulty:?}"));
        }
    }
}

#[test]
fn take_turn_applies_a_bot_move() {
    let mut game = bot_game(&["COATSXY", "ATOESCR"], Difficulty::Hard);
    let mut rng = StdRng::seed_from_u64(5);
    let result = bot::take_turn(&mut game, &dict(), &mut rng);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    // A successful play was recorded.
    assert_eq!(game.moves.len(), 1);
}

#[test]
fn bot_with_no_moves_passes_or_exchanges() {
    // Rack of letters that form no word in this dictionary, empty bag -> pass.
    let game = bot_game(&["BBBBBBB"], Difficulty::Medium);
    let mut rng = StdRng::seed_from_u64(1);
    let kind = bot::choose_move(&game, &dict(), 0, &mut rng);
    assert!(matches!(kind, MoveKind::Pass));
}

#[test]
fn bot_vs_bot_game_terminates() {
    let mut game = bot_game(&["COATSXY", "ATOESCR"], Difficulty::Hard);
    let mut rng = StdRng::seed_from_u64(11);
    let mut guard = 0;
    while game.status == GameStatus::Active {
        let played = bot::take_turn(&mut game, &dict(), &mut rng);
        assert!(played.is_some());
        played.unwrap().expect("bot move applies");
        guard += 1;
        assert!(guard < 100, "bot game failed to terminate");
    }
    assert_eq!(game.status, GameStatus::Finished);
}
