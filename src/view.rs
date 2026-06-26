use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::models::{
    Board, Difficulty, Game, GameStatus, Move, MoveKind, Premium, Seat, SeatKind, Square, Tile,
};

/// A game serialized for a specific viewer: other players' racks and the bag
/// contents are redacted to tile counts only.
#[derive(Serialize)]
pub struct GameView {
    pub id: Uuid,
    pub status: GameStatus,
    pub turn: usize,
    pub your_seat: Option<usize>,
    pub your_rack: Option<Vec<TileView>>,
    pub seats: Vec<SeatView>,
    pub board: Vec<SquareView>,
    pub bag_count: usize,
    pub moves: Vec<MoveView>,
    pub winners: Vec<usize>,
    pub john_mode: bool,
    pub hints_allowed: u8,
    pub hints_remaining: u8,
    pub last_play: Vec<PositionView>,
}

#[derive(Serialize)]
pub struct PositionView {
    pub row: usize,
    pub col: usize,
}

#[derive(Serialize)]
pub struct SeatView {
    pub index: usize,
    pub name: String,
    pub kind: &'static str,
    pub difficulty: Option<Difficulty>,
    pub score: i32,
    pub rack_count: usize,
    pub on_turn: bool,
    pub is_you: bool,
    pub open: bool,
}

#[derive(Serialize)]
pub struct TileView {
    pub letter: Option<char>,
    pub is_blank: bool,
}

#[derive(Serialize)]
pub struct SquareView {
    pub premium: &'static str,
    pub letter: Option<char>,
    pub is_blank: bool,
}

#[derive(Serialize)]
pub struct MoveView {
    pub seat: usize,
    pub kind: &'static str,
    pub words: Vec<String>,
    pub points: u32,
    /// Signed score change for end-game settlement entries; `0` otherwise.
    pub delta: i32,
}

impl GameView {
    pub fn for_viewer(game: &Game, viewer: Uuid) -> Self {
        let your_seat = game
            .seats
            .iter()
            .position(|seat| seat_user(seat) == Some(viewer));
        let your_rack =
            your_seat.map(|index| game.seats[index].rack.iter().map(tile_view).collect());

        let seats = game
            .seats
            .iter()
            .enumerate()
            .map(|(index, seat)| SeatView {
                index,
                name: seat.name.clone(),
                kind: match seat.kind {
                    SeatKind::Bot { .. } => "bot",
                    SeatKind::Human { .. } => "human",
                },
                difficulty: match seat.kind {
                    SeatKind::Bot { difficulty } => Some(difficulty),
                    SeatKind::Human { .. } => None,
                },
                score: seat.score,
                rack_count: seat.rack.len(),
                on_turn: game.status == GameStatus::Active && game.turn == index,
                is_you: Some(index) == your_seat,
                open: matches!(seat.kind, SeatKind::Human { user_id: None }),
            })
            .collect();

        let hints_remaining = match your_seat {
            Some(i) if game.hints_allowed > 0 => game
                .hints_allowed
                .saturating_sub(game.hints_used.get(i).copied().unwrap_or(0)),
            _ => 0,
        };

        let last_play = last_play_positions(game);

        Self {
            id: game.id,
            status: game.status,
            turn: game.turn,
            your_seat,
            your_rack,
            seats,
            board: board_view(&game.board),
            bag_count: game.bag.len(),
            moves: game.moves.iter().map(move_view).collect(),
            winners: winners(game),
            john_mode: game.john_mode,
            hints_allowed: game.hints_allowed,
            hints_remaining,
            last_play,
        }
    }
}

/// A compact summary of a game the viewer is seated in, for the "your other
/// games" panel. `your_turn` highlights games waiting on the viewer.
#[derive(Serialize)]
pub struct GameSummary {
    pub id: Uuid,
    pub players: Vec<String>,
    pub status: &'static str,
    pub your_turn: bool,
    #[serde(skip)]
    pub is_active: bool,
    #[serde(skip)]
    pub effective_updated_at: DateTime<Utc>,
}

impl GameSummary {
    /// Build a summary for `viewer`, or `None` when they aren't seated.
    pub fn for_viewer(game: &Game, viewer: Uuid) -> Option<Self> {
        let your_seat = game
            .seats
            .iter()
            .position(|seat| seat_user(seat) == Some(viewer))?;
        let is_active = game.status != GameStatus::Finished;
        let effective_updated_at = if game.updated_at == DateTime::<Utc>::default() {
            game.created_at
        } else {
            game.updated_at
        };
        Some(Self {
            id: game.id,
            players: game.seats.iter().map(|seat| seat.name.clone()).collect(),
            status: match game.status {
                GameStatus::Lobby => "lobby",
                GameStatus::Active => "active",
                GameStatus::Finished => "finished",
            },
            your_turn: game.status == GameStatus::Active && game.turn == your_seat,
            is_active,
            effective_updated_at,
        })
    }
}

fn seat_user(seat: &Seat) -> Option<Uuid> {
    match seat.kind {
        SeatKind::Human { user_id } => user_id,
        SeatKind::Bot { .. } => None,
    }
}

fn tile_view(tile: &Tile) -> TileView {
    match tile {
        Tile::Letter(letter) => TileView {
            letter: Some(*letter),
            is_blank: false,
        },
        Tile::Blank => TileView {
            letter: None,
            is_blank: true,
        },
    }
}

fn board_view(board: &Board) -> Vec<SquareView> {
    board.squares.iter().map(square_view).collect()
}

fn square_view(square: &Square) -> SquareView {
    SquareView {
        premium: premium_code(square.premium),
        letter: square.tile.map(|tile| tile.letter),
        is_blank: square.tile.is_some_and(|tile| tile.is_blank),
    }
}

pub fn premium_code(premium: Premium) -> &'static str {
    match premium {
        Premium::None => "none",
        Premium::DoubleLetter => "dl",
        Premium::TripleLetter => "tl",
        Premium::DoubleWord => "dw",
        Premium::TripleWord => "tw",
    }
}

fn move_view(mv: &Move) -> MoveView {
    let (kind, words, delta) = match &mv.kind {
        MoveKind::Play { .. } => ("play", mv.words.clone(), 0),
        MoveKind::Exchange { .. } => ("exchange", mv.words.clone(), 0),
        MoveKind::Pass => ("pass", mv.words.clone(), 0),
        MoveKind::EndAdjustment { delta, tiles } => {
            ("adjustment", tiles.iter().map(tile_label).collect(), *delta)
        }
    };
    MoveView {
        seat: mv.seat,
        kind,
        words,
        points: mv.points,
        delta,
    }
}

fn tile_label(tile: &Tile) -> String {
    match tile {
        Tile::Letter(letter) => letter.to_string(),
        Tile::Blank => "?".to_string(),
    }
}

fn last_play_positions(game: &Game) -> Vec<PositionView> {
    game.moves
        .iter()
        .rev()
        .find_map(|mv| match &mv.kind {
            MoveKind::Play { placements } => Some(
                placements
                    .iter()
                    .map(|p| PositionView {
                        row: p.position.row,
                        col: p.position.col,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// Indices of the highest-scoring seat(s) once a game is finished.
fn winners(game: &Game) -> Vec<usize> {
    if game.status != GameStatus::Finished {
        return Vec::new();
    }
    let best = game.seats.iter().map(|seat| seat.score).max().unwrap_or(0);
    game.seats
        .iter()
        .enumerate()
        .filter(|(_, seat)| seat.score == best)
        .map(|(index, _)| index)
        .collect()
}
