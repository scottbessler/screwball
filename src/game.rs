use std::collections::HashSet;
use std::fmt;

use chrono::Utc;
use rand::Rng;
use uuid::Uuid;

use crate::bag;
use crate::board::collect_run;
use crate::dict::Dictionary;
use crate::models::{
    BINGO_BONUS, Board, CENTER, Game, GameStatus, Move, MoveKind, PlacedTile, Placement, Position,
    Premium, RACK_SIZE, SCORELESS_LIMIT, Seat, SeatKind, Tile,
};

/// Specification for one seat at table-creation time.
#[derive(Clone, Debug)]
pub struct SeatSpec {
    pub kind: SeatKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveError {
    GameNotActive,
    NotYourTurn,
    EmptyPlay,
    OutOfBounds,
    DuplicatePosition,
    SquareOccupied,
    TilesNotInRack,
    NotInLine,
    NotContiguous,
    FirstMoveMustCoverCenter,
    NotConnected,
    NoWordFormed,
    InvalidWords(Vec<String>),
    WordsTooShort(Vec<String>),
    CannotExchange,
    ExchangeTilesNotInRack,
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::GameNotActive => write!(f, "the game is not active"),
            MoveError::NotYourTurn => write!(f, "it is not your turn"),
            MoveError::EmptyPlay => write!(f, "a play must place at least one tile"),
            MoveError::OutOfBounds => write!(f, "a tile was placed off the board"),
            MoveError::DuplicatePosition => write!(f, "two tiles were placed on the same square"),
            MoveError::SquareOccupied => write!(f, "a tile was placed on an occupied square"),
            MoveError::TilesNotInRack => write!(f, "you do not have those tiles"),
            MoveError::NotInLine => write!(f, "tiles must be placed in a single row or column"),
            MoveError::NotContiguous => write!(f, "placed tiles must form an unbroken line"),
            MoveError::FirstMoveMustCoverCenter => {
                write!(f, "the first move must cover the center square")
            }
            MoveError::NotConnected => write!(f, "the play must connect to existing tiles"),
            MoveError::NoWordFormed => {
                write!(f, "the play must form a word of two or more letters")
            }
            MoveError::InvalidWords(words) => write!(f, "not in dictionary: {}", words.join(", ")),
            MoveError::WordsTooShort(words) => {
                write!(f, "too short for John Mode: {}", words.join(", "))
            }
            MoveError::CannotExchange => {
                write!(f, "cannot exchange when fewer than seven tiles remain")
            }
            MoveError::ExchangeTilesNotInRack => {
                write!(f, "you do not have those tiles to exchange")
            }
        }
    }
}

impl std::error::Error for MoveError {}

/// A formed word: the cells it spans, the resulting string, and its score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoredWord {
    pub word: String,
    pub points: u32,
}

/// Result of validating a [`MoveKind::Play`] without mutating game state.
#[derive(Clone, Debug)]
pub struct ScoredPlay {
    pub placed: Vec<(Position, PlacedTile)>,
    pub words: Vec<ScoredWord>,
    pub points: u32,
}

pub fn new_game(
    seats: Vec<SeatSpec>,
    john_mode: bool,
    hints_allowed: u8,
    rng: &mut impl Rng,
) -> Game {
    let mut bag = bag::shuffled_bag(rng);
    let seats = seats
        .into_iter()
        .map(|spec| {
            let rack = bag::draw(&mut bag, RACK_SIZE);
            Seat {
                id: Uuid::new_v4(),
                kind: spec.kind,
                name: spec.name,
                rack,
                score: 0,
            }
        })
        .collect::<Vec<_>>();
    let seat_count = seats.len();
    Game {
        id: Uuid::new_v4(),
        status: GameStatus::Active,
        board: Board::new(),
        bag,
        seats,
        turn: 0,
        moves: Vec::new(),
        consecutive_scoreless: 0,
        created_at: Utc::now(),
        john_mode,
        hints_allowed,
        hints_used: vec![0; seat_count],
    }
}

/// Validate and score a play against the board and rack, without mutating.
pub fn validate_play(
    board: &Board,
    rack: &[Tile],
    dict: &Dictionary,
    placements: &[Placement],
    min_word_length: usize,
) -> Result<ScoredPlay, MoveError> {
    if placements.is_empty() {
        return Err(MoveError::EmptyPlay);
    }

    let mut seen = HashSet::new();
    for placement in placements {
        if !placement.position.in_bounds() {
            return Err(MoveError::OutOfBounds);
        }
        if !seen.insert(placement.position) {
            return Err(MoveError::DuplicatePosition);
        }
        if board.is_occupied(placement.position) {
            return Err(MoveError::SquareOccupied);
        }
    }

    check_rack_has_tiles(rack, placements)?;

    let same_row = placements
        .iter()
        .all(|p| p.position.row == placements[0].position.row);
    let same_col = placements
        .iter()
        .all(|p| p.position.col == placements[0].position.col);
    if !same_row && !same_col {
        return Err(MoveError::NotInLine);
    }

    let board_was_empty = board.is_empty();

    // Apply onto a scratch board to discover the words formed.
    let mut scratch = board.clone();
    for placement in placements {
        scratch.set_tile(
            placement.position,
            PlacedTile {
                letter: placement.position_letter(),
                is_blank: placement.is_blank,
            },
        );
    }

    let multi_col = placements
        .iter()
        .any(|p| p.position.col != placements[0].position.col);
    let multi_row = placements
        .iter()
        .any(|p| p.position.row != placements[0].position.row);
    let (main_dir, cross_dir): ((isize, isize), (isize, isize)) = if multi_col {
        ((0, 1), (1, 0))
    } else if multi_row {
        ((1, 0), (0, 1))
    } else {
        ((0, 1), (1, 0))
    };

    let placed_set: HashSet<Position> = placements.iter().map(|p| p.position).collect();

    // Contiguity: every placed tile must sit within the single main-axis run.
    let main_run = collect_run(&scratch, placements[0].position, main_dir);
    let main_cells: HashSet<Position> = main_run.iter().copied().collect();
    if !placed_set.iter().all(|pos| main_cells.contains(pos)) {
        return Err(MoveError::NotContiguous);
    }

    if board_was_empty && !placed_set.contains(&CENTER) {
        return Err(MoveError::FirstMoveMustCoverCenter);
    }

    let mut words: Vec<Vec<Position>> = Vec::new();
    if main_run.len() >= 2 {
        words.push(main_run);
    }
    for placement in placements {
        let cross = collect_run(&scratch, placement.position, cross_dir);
        if cross.len() >= 2 {
            words.push(cross);
        }
    }

    if words.is_empty() {
        return Err(MoveError::NoWordFormed);
    }

    if !board_was_empty {
        let connects = words
            .iter()
            .any(|cells| cells.iter().any(|pos| !placed_set.contains(pos)));
        if !connects {
            return Err(MoveError::NotConnected);
        }
    }

    let mut scored = Vec::with_capacity(words.len());
    let mut invalid = Vec::new();
    let mut too_short = Vec::new();
    let mut total = 0u32;
    for cells in &words {
        let word = word_string(&scratch, cells);
        if word.len() < min_word_length {
            too_short.push(word);
            continue;
        }
        if !dict.contains(&word) {
            invalid.push(word.clone());
            continue;
        }
        let points = score_word(&scratch, cells, &placed_set);
        total += points;
        scored.push(ScoredWord { word, points });
    }
    if !too_short.is_empty() {
        too_short.sort();
        too_short.dedup();
        return Err(MoveError::WordsTooShort(too_short));
    }
    if !invalid.is_empty() {
        invalid.sort();
        invalid.dedup();
        return Err(MoveError::InvalidWords(invalid));
    }

    if placements.len() == RACK_SIZE {
        total += BINGO_BONUS;
    }

    let placed = placements
        .iter()
        .map(|p| {
            (
                p.position,
                PlacedTile {
                    letter: p.position_letter(),
                    is_blank: p.is_blank,
                },
            )
        })
        .collect();

    Ok(ScoredPlay {
        placed,
        words: scored,
        points: total,
    })
}

fn check_rack_has_tiles(rack: &[Tile], placements: &[Placement]) -> Result<(), MoveError> {
    let mut available = rack.to_vec();
    for placement in placements {
        let needed = if placement.is_blank {
            Tile::Blank
        } else {
            Tile::Letter(placement.position_letter())
        };
        match available.iter().position(|&tile| tile == needed) {
            Some(index) => {
                available.swap_remove(index);
            }
            None => return Err(MoveError::TilesNotInRack),
        }
    }
    Ok(())
}

fn word_string(board: &Board, cells: &[Position]) -> String {
    cells
        .iter()
        .filter_map(|&pos| board.tile_at(pos).map(|tile| tile.letter))
        .collect()
}

fn score_word(board: &Board, cells: &[Position], placed: &HashSet<Position>) -> u32 {
    let mut word_score = 0u32;
    let mut word_multiplier = 1u32;
    for &pos in cells {
        let Some(tile) = board.tile_at(pos) else {
            continue;
        };
        let mut value = tile.points();
        if placed.contains(&pos) {
            match board.square(pos).premium {
                Premium::DoubleLetter => value *= 2,
                Premium::TripleLetter => value *= 3,
                Premium::DoubleWord => word_multiplier *= 2,
                Premium::TripleWord => word_multiplier *= 3,
                Premium::None => {}
            }
        }
        word_score += value;
    }
    word_score * word_multiplier
}

impl Placement {
    fn position_letter(&self) -> char {
        self.letter.to_ascii_uppercase()
    }
}

/// Apply a move for the seat whose turn it currently is, mutating the game.
pub fn apply_move(
    game: &mut Game,
    dict: &Dictionary,
    seat_index: usize,
    kind: MoveKind,
    rng: &mut impl Rng,
) -> Result<Move, MoveError> {
    if game.status != GameStatus::Active {
        return Err(MoveError::GameNotActive);
    }
    if seat_index != game.turn {
        return Err(MoveError::NotYourTurn);
    }

    let recorded = match kind {
        MoveKind::Play { placements } => {
            let scored = validate_play(
                &game.board,
                &game.seats[seat_index].rack,
                dict,
                &placements,
                game.min_word_length(),
            )?;
            for placement in &placements {
                let needed = if placement.is_blank {
                    Tile::Blank
                } else {
                    Tile::Letter(placement.letter.to_ascii_uppercase())
                };
                remove_tile(&mut game.seats[seat_index].rack, needed);
            }
            for &(pos, tile) in &scored.placed {
                game.board.set_tile(pos, tile);
            }
            refill_rack(&mut game.seats[seat_index].rack, &mut game.bag);
            game.seats[seat_index].score += scored.points as i32;
            Move {
                seat: seat_index,
                kind: MoveKind::Play { placements },
                words: scored.words.iter().map(|w| w.word.clone()).collect(),
                points: scored.points,
            }
        }
        MoveKind::Exchange { tiles } => {
            if game.bag.len() < RACK_SIZE {
                return Err(MoveError::CannotExchange);
            }
            let mut scratch = game.seats[seat_index].rack.clone();
            for tile in &tiles {
                match scratch.iter().position(|t| t == tile) {
                    Some(index) => {
                        scratch.swap_remove(index);
                    }
                    None => return Err(MoveError::ExchangeTilesNotInRack),
                }
            }
            for tile in &tiles {
                remove_tile(&mut game.seats[seat_index].rack, *tile);
            }
            let drawn = bag::draw(&mut game.bag, tiles.len());
            game.seats[seat_index].rack.extend(drawn);
            game.bag.extend(tiles.iter().copied());
            use rand::seq::SliceRandom;
            game.bag.shuffle(rng);
            Move {
                seat: seat_index,
                kind: MoveKind::Exchange { tiles },
                words: Vec::new(),
                points: 0,
            }
        }
        MoveKind::Pass => Move {
            seat: seat_index,
            kind: MoveKind::Pass,
            words: Vec::new(),
            points: 0,
        },
        // Settlement entries are produced only by `finish_game`, never submitted.
        MoveKind::EndAdjustment { .. } => unreachable!("end adjustments are engine-internal"),
    };

    if recorded.points == 0 {
        game.consecutive_scoreless = game.consecutive_scoreless.saturating_add(1);
    } else {
        game.consecutive_scoreless = 0;
    }
    game.moves.push(recorded.clone());

    let went_out = game.seats[seat_index].rack.is_empty() && game.bag.is_empty();
    if went_out || game.consecutive_scoreless >= SCORELESS_LIMIT {
        finish_game(game, if went_out { Some(seat_index) } else { None });
    } else {
        game.turn = (game.turn + 1) % game.seats.len();
    }

    Ok(recorded)
}

fn remove_tile(rack: &mut Vec<Tile>, tile: Tile) {
    if let Some(index) = rack.iter().position(|&t| t == tile) {
        rack.swap_remove(index);
    }
}

fn refill_rack(rack: &mut Vec<Tile>, bag: &mut Vec<Tile>) {
    let needed = RACK_SIZE.saturating_sub(rack.len());
    if needed > 0 {
        rack.extend(bag::draw(bag, needed));
    }
}

/// Settle final scores: each seat loses its remaining rack value; a seat that
/// emptied its rack gains the sum of everyone else's remaining tiles.
fn finish_game(game: &mut Game, went_out: Option<usize>) {
    let remaining: Vec<i32> = game
        .seats
        .iter()
        .map(|seat| seat.rack.iter().map(|tile| tile.points() as i32).sum())
        .collect();
    let total_remaining: i32 = remaining.iter().sum();
    let leftover: Vec<Vec<Tile>> = game.seats.iter().map(|seat| seat.rack.clone()).collect();
    for (index, seat) in game.seats.iter_mut().enumerate() {
        seat.score -= remaining[index];
        if Some(index) == went_out {
            seat.score += total_remaining;
        }
    }
    // Record the settlement in the move log so the UI can show each penalty
    // and the going-out bonus.
    for index in 0..game.seats.len() {
        let mut delta = -remaining[index];
        if Some(index) == went_out {
            delta += total_remaining;
        }
        if delta != 0 {
            game.moves.push(Move {
                seat: index,
                kind: MoveKind::EndAdjustment {
                    delta,
                    tiles: leftover[index].clone(),
                },
                words: Vec::new(),
                points: 0,
            });
        }
    }
    game.status = GameStatus::Finished;
}
