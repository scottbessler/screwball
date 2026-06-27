use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const BOARD_SIZE: usize = 15;
pub const BOARD_CELLS: usize = BOARD_SIZE * BOARD_SIZE;
pub const RACK_SIZE: usize = 7;
pub const CENTER: Position = Position { row: 7, col: 7 };
pub const BINGO_BONUS: u32 = 50;
/// Number of consecutive scoreless turns (passes / exchanges) that ends a game.
pub const SCORELESS_LIMIT: u8 = 6;

/// A tile as it exists in a bag or on a rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    /// A lettered tile, always uppercase `A`..=`Z`.
    Letter(char),
    /// A blank tile; its letter is chosen when it is played.
    Blank,
}

impl Tile {
    /// Scrabble point value of the tile when drawn from the bag.
    pub fn points(self) -> u32 {
        match self {
            Tile::Blank => 0,
            Tile::Letter(letter) => letter_points(letter),
        }
    }
}

/// Point value of a played letter (blanks resolve to 0 via [`PlacedTile`]).
pub fn letter_points(letter: char) -> u32 {
    match letter.to_ascii_uppercase() {
        'A' | 'E' | 'I' | 'O' | 'U' | 'L' | 'N' | 'S' | 'T' | 'R' => 1,
        'D' | 'G' => 2,
        'B' | 'C' | 'M' | 'P' => 3,
        'F' | 'H' | 'V' | 'W' | 'Y' => 4,
        'K' => 5,
        'J' | 'X' => 8,
        'Q' | 'Z' => 10,
        _ => 0,
    }
}

/// A tile resolved onto the board. A blank remembers the letter it represents
/// but always scores zero points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedTile {
    pub letter: char,
    pub is_blank: bool,
}

impl PlacedTile {
    pub fn points(self) -> u32 {
        if self.is_blank {
            0
        } else {
            letter_points(self.letter)
        }
    }
}

/// Premium multiplier on a board square.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Premium {
    None,
    DoubleLetter,
    TripleLetter,
    DoubleWord,
    TripleWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    pub fn in_bounds(self) -> bool {
        self.row < BOARD_SIZE && self.col < BOARD_SIZE
    }

    pub fn index(self) -> usize {
        self.row * BOARD_SIZE + self.col
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Square {
    pub premium: Premium,
    pub tile: Option<PlacedTile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub squares: Vec<Square>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Chill,
    Medium,
    Hard,
    Impossible,
}

/// Who occupies a seat at the table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeatKind {
    Human { user_id: Option<Uuid> },
    Bot { difficulty: Difficulty },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub id: Uuid,
    pub kind: SeatKind,
    pub name: String,
    pub rack: Vec<Tile>,
    pub score: i32,
}

impl Seat {
    pub fn is_bot(&self) -> bool {
        matches!(self.kind, SeatKind::Bot { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameStatus {
    Lobby,
    Active,
    Finished,
}

/// A single tile placement within a play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Placement {
    pub position: Position,
    /// The letter that lands on the board.
    pub letter: char,
    /// Whether the placement was made with a blank tile.
    pub is_blank: bool,
}

/// What a seat did on its turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveKind {
    Play {
        placements: Vec<Placement>,
    },
    Exchange {
        tiles: Vec<Tile>,
    },
    Pass,
    /// End-of-game score settlement: a seat loses the value of its leftover
    /// `tiles` (negative `delta`), or the seat that went out gains the sum of
    /// everyone else's leftovers (positive `delta`, empty `tiles`).
    EndAdjustment {
        delta: i32,
        tiles: Vec<Tile>,
    },
}

/// A completed, scored move recorded in the game log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    pub seat: usize,
    pub kind: MoveKind,
    pub words: Vec<String>,
    pub points: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: Uuid,
    pub status: GameStatus,
    pub board: Board,
    pub bag: Vec<Tile>,
    pub seats: Vec<Seat>,
    pub turn: usize,
    pub moves: Vec<Move>,
    pub consecutive_scoreless: u8,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: DateTime<Utc>,
    /// John Mode: a display helper that surfaces valid 2-letter words; it does
    /// not restrict play. Kept for backward-compatible game state.
    #[serde(default)]
    pub john_mode: bool,
    /// Grandpa Mode: disallow 2-letter words except a few common ones.
    #[serde(default)]
    pub grandpa_mode: bool,
    #[serde(default)]
    pub hints_allowed: u8,
    #[serde(default)]
    pub hints_used: Vec<u8>,
}

/// 2-letter words still allowed in Grandpa Mode: the everyday ones any
/// non-Scrabble-nerd would recognise. Excludes the obscure dump tiles
/// (XU, QI, ZA, AA, JO, …) that Grandpa Mode is meant to ban.
pub const GRANDPA_TWO_LETTER: &[&str] = &[
    "AM", "AN", "AS", "AT", "BE", "BY", "DO", "GO", "HE", "HI", "IF", "IN", "IS", "IT", "ME", "MY",
    "NO", "OF", "OH", "ON", "OR", "SO", "TO", "UP", "US", "WE",
];

/// Which words a play may form, beyond being in the dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordRule {
    /// Any dictionary word (2+ letters).
    Standard,
    /// Grandpa Mode: 2-letter words only if in [`GRANDPA_TWO_LETTER`].
    Grandpa,
}

impl WordRule {
    /// Whether `word` (uppercase) is permitted by this rule.
    pub fn allows(&self, word: &str) -> bool {
        match self {
            WordRule::Standard => true,
            WordRule::Grandpa => word.len() != 2 || GRANDPA_TWO_LETTER.contains(&word),
        }
    }
}

impl Game {
    pub fn word_rule(&self) -> WordRule {
        if self.grandpa_mode {
            WordRule::Grandpa
        } else {
            WordRule::Standard
        }
    }
}
