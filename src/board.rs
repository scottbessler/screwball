use crate::models::{BOARD_CELLS, BOARD_SIZE, Board, PlacedTile, Position, Premium, Square};

/// Triple-word squares on a standard board.
const TRIPLE_WORD: &[(usize, usize)] = &[
    (0, 0),
    (0, 7),
    (0, 14),
    (7, 0),
    (7, 14),
    (14, 0),
    (14, 7),
    (14, 14),
];

/// Double-word squares (excluding the center, handled separately).
const DOUBLE_WORD: &[(usize, usize)] = &[
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 4),
    (1, 13),
    (2, 12),
    (3, 11),
    (4, 10),
    (10, 4),
    (11, 3),
    (12, 2),
    (13, 1),
    (10, 10),
    (11, 11),
    (12, 12),
    (13, 13),
];

const TRIPLE_LETTER: &[(usize, usize)] = &[
    (1, 5),
    (1, 9),
    (5, 1),
    (5, 5),
    (5, 9),
    (5, 13),
    (9, 1),
    (9, 5),
    (9, 9),
    (9, 13),
    (13, 5),
    (13, 9),
];

const DOUBLE_LETTER: &[(usize, usize)] = &[
    (0, 3),
    (0, 11),
    (2, 6),
    (2, 8),
    (3, 0),
    (3, 7),
    (3, 14),
    (6, 2),
    (6, 6),
    (6, 8),
    (6, 12),
    (7, 3),
    (7, 11),
    (8, 2),
    (8, 6),
    (8, 8),
    (8, 12),
    (11, 0),
    (11, 7),
    (11, 14),
    (12, 6),
    (12, 8),
    (14, 3),
    (14, 11),
];

fn premium_for(row: usize, col: usize) -> Premium {
    if row == 7 && col == 7 {
        return Premium::DoubleWord;
    }
    let cell = (row, col);
    if TRIPLE_WORD.contains(&cell) {
        Premium::TripleWord
    } else if DOUBLE_WORD.contains(&cell) {
        Premium::DoubleWord
    } else if TRIPLE_LETTER.contains(&cell) {
        Premium::TripleLetter
    } else if DOUBLE_LETTER.contains(&cell) {
        Premium::DoubleLetter
    } else {
        Premium::None
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        let mut squares = Vec::with_capacity(BOARD_CELLS);
        for row in 0..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                squares.push(Square {
                    premium: premium_for(row, col),
                    tile: None,
                });
            }
        }
        Self { squares }
    }

    pub fn square(&self, pos: Position) -> &Square {
        &self.squares[pos.index()]
    }

    pub fn tile_at(&self, pos: Position) -> Option<PlacedTile> {
        self.squares[pos.index()].tile
    }

    pub fn is_occupied(&self, pos: Position) -> bool {
        self.squares[pos.index()].tile.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.squares.iter().all(|square| square.tile.is_none())
    }

    pub fn set_tile(&mut self, pos: Position, tile: PlacedTile) {
        self.squares[pos.index()].tile = Some(tile);
    }
}

/// Step from `pos` by `(d_row, d_col)`, returning `None` when off the board.
pub fn step(pos: Position, d_row: isize, d_col: isize) -> Option<Position> {
    let row = pos.row as isize + d_row;
    let col = pos.col as isize + d_col;
    if row < 0 || col < 0 || row >= BOARD_SIZE as isize || col >= BOARD_SIZE as isize {
        None
    } else {
        Some(Position::new(row as usize, col as usize))
    }
}

/// Collect the unbroken run of occupied squares along `dir` that contains
/// `start`. Walks backward to the head of the run, then forward to its tail.
pub fn collect_run(board: &Board, start: Position, dir: (isize, isize)) -> Vec<Position> {
    let (d_row, d_col) = dir;
    let mut head = start;
    while let Some(prev) = step(head, -d_row, -d_col) {
        if board.is_occupied(prev) {
            head = prev;
        } else {
            break;
        }
    }

    let mut cells = Vec::new();
    let mut cursor = Some(head);
    while let Some(pos) = cursor {
        if board.is_occupied(pos) {
            cells.push(pos);
            cursor = step(pos, d_row, d_col);
        } else {
            break;
        }
    }
    cells
}
