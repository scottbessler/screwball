use std::collections::HashSet;

use rand::Rng;

use crate::board::step;
use crate::dict::Dictionary;
use crate::game::MoveError;
use crate::game::{ScoredPlay, apply_move, validate_play};
use crate::models::{
    BOARD_SIZE, Board, CENTER, Difficulty, Game, GameStatus, Move, MoveKind, Placement, Position,
    RACK_SIZE, SeatKind, Tile, WordRule,
};

const ALL_LETTERS: u32 = (1 << 26) - 1;
/// Shortest word the board can form. The rule (e.g. Grandpa Mode) is applied
/// later in validation, not during generation.
const MIN_WORD_LEN: usize = 2;

fn letter_index(letter: char) -> usize {
    (letter as u8 - b'A') as usize
}

fn letter_at(index: usize) -> char {
    (b'A' + index as u8) as char
}

/// Multiset of a rack as letter counts plus a blank count.
#[derive(Clone)]
struct RackCounts {
    counts: [u8; 26],
    blanks: u8,
}

impl RackCounts {
    fn from_rack(rack: &[Tile]) -> Self {
        let mut counts = [0u8; 26];
        let mut blanks = 0u8;
        for tile in rack {
            match tile {
                Tile::Letter(letter) => counts[letter_index(*letter)] += 1,
                Tile::Blank => blanks += 1,
            }
        }
        Self { counts, blanks }
    }

    fn total(&self) -> usize {
        self.counts.iter().map(|&c| c as usize).sum::<usize>() + self.blanks as usize
    }
}

fn position(transposed: bool, line: usize, idx: usize) -> Position {
    if transposed {
        Position::new(idx, line)
    } else {
        Position::new(line, idx)
    }
}

/// For each empty square, the bitmask of letters that form a valid word in the
/// cross (perpendicular) direction given its neighbors. Squares with no
/// perpendicular neighbor allow every letter.
fn cross_masks(board: &Board, dict: &Dictionary, cross_dir: (isize, isize)) -> Vec<u32> {
    let (d_row, d_col) = cross_dir;
    let mut masks = vec![0u32; BOARD_SIZE * BOARD_SIZE];
    for row in 0..BOARD_SIZE {
        for col in 0..BOARD_SIZE {
            let pos = Position::new(row, col);
            if board.is_occupied(pos) {
                continue;
            }
            let prefix = collect_letters(board, pos, (-d_row, -d_col), true);
            let suffix = collect_letters(board, pos, (d_row, d_col), false);
            if prefix.is_empty() && suffix.is_empty() {
                masks[pos.index()] = ALL_LETTERS;
                continue;
            }
            if prefix.len() + 1 + suffix.len() < MIN_WORD_LEN {
                masks[pos.index()] = 0;
                continue;
            }
            let mut mask = 0u32;
            for index in 0..26 {
                let candidate: String = format!("{prefix}{}{suffix}", letter_at(index));
                if dict.contains(&candidate) {
                    mask |= 1 << index;
                }
            }
            masks[pos.index()] = mask;
        }
    }
    masks
}

/// Walk from `pos` along `dir` collecting contiguous occupied letters. When
/// `reverse` is set the result is reversed so it reads left-to-right.
fn collect_letters(board: &Board, pos: Position, dir: (isize, isize), reverse: bool) -> String {
    let mut letters = Vec::new();
    let mut cursor = step(pos, dir.0, dir.1);
    while let Some(p) = cursor {
        match board.tile_at(p) {
            Some(tile) => {
                letters.push(tile.letter);
                cursor = step(p, dir.0, dir.1);
            }
            None => break,
        }
    }
    if reverse {
        letters.reverse();
    }
    letters.into_iter().collect()
}

fn anchors(board: &Board) -> HashSet<Position> {
    if board.is_empty() {
        return HashSet::from([CENTER]);
    }
    let mut set = HashSet::new();
    for row in 0..BOARD_SIZE {
        for col in 0..BOARD_SIZE {
            let pos = Position::new(row, col);
            if board.is_occupied(pos) {
                continue;
            }
            let touches = [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|&(dr, dc)| {
                step(pos, dr, dc).is_some_and(|neighbor| board.is_occupied(neighbor))
            });
            if touches {
                set.insert(pos);
            }
        }
    }
    set
}

struct Generator<'a> {
    board: &'a Board,
    dict: &'a Dictionary,
    masks: Vec<u32>,
    anchors: &'a HashSet<Position>,
    transposed: bool,
    rack: RackCounts,
    rack_size: usize,
    placed: Vec<Placement>,
    out: &'a mut HashSet<Vec<Placement>>,
}

impl Generator<'_> {
    fn record(&mut self) {
        if self.placed.is_empty() {
            return;
        }
        if !self
            .placed
            .iter()
            .any(|p| self.anchors.contains(&p.position))
        {
            return;
        }
        let mut key = self.placed.clone();
        key.sort_by_key(|p| (p.position.row, p.position.col));
        self.out.insert(key);
    }

    fn dfs(&mut self, line: usize, idx: usize, node: usize, new_count: usize) {
        if idx >= BOARD_SIZE {
            if self.dict.is_terminal(node) {
                self.record();
            }
            return;
        }
        let pos = position(self.transposed, line, idx);
        if let Some(tile) = self.board.tile_at(pos) {
            if let Some(next) = self.dict.step(node, tile.letter) {
                self.dfs(line, idx + 1, next, new_count);
            }
            return;
        }

        if self.dict.is_terminal(node) {
            self.record();
        }
        if new_count >= self.rack_size {
            return;
        }

        let mask = self.masks[pos.index()];
        for index in 0..26 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let letter = letter_at(index);
            let Some(next) = self.dict.step(node, letter) else {
                continue;
            };
            if self.rack.counts[index] > 0 {
                self.rack.counts[index] -= 1;
                self.placed.push(Placement {
                    position: pos,
                    letter,
                    is_blank: false,
                });
                self.dfs(line, idx + 1, next, new_count + 1);
                self.placed.pop();
                self.rack.counts[index] += 1;
            }
            if self.rack.blanks > 0 {
                self.rack.blanks -= 1;
                self.placed.push(Placement {
                    position: pos,
                    letter,
                    is_blank: true,
                });
                self.dfs(line, idx + 1, next, new_count + 1);
                self.placed.pop();
                self.rack.blanks += 1;
            }
        }
    }
}

/// Enumerate every legal play for `rack` against `board`, deduplicated.
pub fn generate_plays(board: &Board, rack: &[Tile], dict: &Dictionary) -> Vec<Vec<Placement>> {
    let rack_counts = RackCounts::from_rack(rack);
    let rack_size = rack_counts.total().min(RACK_SIZE);
    let anchors = anchors(board);
    let mut out: HashSet<Vec<Placement>> = HashSet::new();

    for transposed in [false, true] {
        let cross_dir = if transposed { (0, 1) } else { (1, 0) };
        let masks = cross_masks(board, dict, cross_dir);
        let mut generator = Generator {
            board,
            dict,
            masks,
            anchors: &anchors,
            transposed,
            rack: rack_counts.clone(),
            rack_size,
            placed: Vec::new(),
            out: &mut out,
        };
        for line in 0..BOARD_SIZE {
            for start in 0..BOARD_SIZE {
                if start > 0 {
                    let left = position(transposed, line, start - 1);
                    if board.is_occupied(left) {
                        continue;
                    }
                }
                generator.dfs(line, start, dict.root(), 0);
            }
        }
    }

    out.into_iter().collect()
}

/// A generated play validated and scored against the real board + rack.
pub fn scored_plays(
    board: &Board,
    rack: &[Tile],
    dict: &Dictionary,
    august_mode: bool,
    rule: WordRule,
) -> Vec<(Vec<Placement>, ScoredPlay)> {
    let augmented;
    let generator_dict = if rule.allows_name_words() {
        augmented = dict.with_extra_words(crate::models::jax_names());
        &augmented
    } else {
        dict
    };
    generate_plays(board, rack, generator_dict)
        .into_iter()
        .filter_map(|placements| {
            validate_play(board, rack, dict, &placements, august_mode, false, rule)
                .ok()
                .map(|scored| (placements, scored))
        })
        .collect()
}

/// Pick a move for the seat at `seat_index` according to its difficulty.
pub fn choose_move(
    game: &Game,
    dict: &Dictionary,
    seat_index: usize,
    rng: &mut impl Rng,
) -> MoveKind {
    let seat = &game.seats[seat_index];
    let mut plays = scored_plays(
        &game.board,
        &seat.rack,
        dict,
        game.august_mode,
        game.word_rule_for(seat.is_bot()),
    );
    if plays.is_empty() {
        if game.bag.len() >= RACK_SIZE {
            return MoveKind::Exchange {
                tiles: seat.rack.clone(),
            };
        }
        return MoveKind::Pass;
    }

    plays.sort_by_key(|play| std::cmp::Reverse(play.1.points));
    let difficulty = match seat.kind {
        SeatKind::Bot { difficulty } => difficulty,
        SeatKind::Human { .. } => Difficulty::Medium,
    };

    // plays sorted highest-score-first (index 0 = best).
    let n = plays.len();
    let index = match difficulty {
        Difficulty::Impossible => 0,
        Difficulty::Hard => rng.gen_range(0..(n / 10).max(1)),
        Difficulty::Medium => rng.gen_range(0..(n / 4).max(1)),
        Difficulty::Chill => {
            // Middle 50%: 25th–75th percentile.
            let lo = n / 4;
            let hi = (3 * n / 4).max(lo + 1).min(n);
            rng.gen_range(lo..hi)
        }
        Difficulty::Easy => {
            // Bottom 50%.
            let lo = n / 2;
            rng.gen_range(lo..n)
        }
    };

    MoveKind::Play {
        placements: plays.swap_remove(index).0,
    }
}

/// If the current seat is a bot, choose and apply its move.
pub fn take_turn(
    game: &mut Game,
    dict: &Dictionary,
    rng: &mut impl Rng,
) -> Option<Result<Move, MoveError>> {
    if game.status != GameStatus::Active {
        return None;
    }
    let seat_index = game.turn;
    if !game.seats[seat_index].is_bot() {
        return None;
    }
    let kind = choose_move(game, dict, seat_index, rng);
    Some(apply_move(game, dict, seat_index, kind, rng))
}
