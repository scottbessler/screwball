use rand::seq::SliceRandom;

use crate::models::Tile;

/// Standard English Scrabble tile distribution: `(letter, count)` plus blanks.
const DISTRIBUTION: &[(char, usize)] = &[
    ('A', 9),
    ('B', 2),
    ('C', 2),
    ('D', 4),
    ('E', 12),
    ('F', 2),
    ('G', 3),
    ('H', 2),
    ('I', 9),
    ('J', 1),
    ('K', 1),
    ('L', 4),
    ('M', 2),
    ('N', 6),
    ('O', 8),
    ('P', 2),
    ('Q', 1),
    ('R', 6),
    ('S', 4),
    ('T', 6),
    ('U', 4),
    ('V', 2),
    ('W', 2),
    ('X', 1),
    ('Y', 2),
    ('Z', 1),
];

const BLANK_COUNT: usize = 2;
const AUGUST_LETTERS: &[char] = &['A', 'U', 'G', 'U', 'S', 'T'];

/// Build a full, ordered (unshuffled) 100-tile bag.
pub fn full_bag() -> Vec<Tile> {
    let mut tiles = Vec::with_capacity(100);
    for &(letter, count) in DISTRIBUTION {
        for _ in 0..count {
            tiles.push(Tile::Letter(letter));
        }
    }
    for _ in 0..BLANK_COUNT {
        tiles.push(Tile::Blank);
    }
    tiles
}

/// Build a full bag and shuffle it with the provided RNG.
pub fn shuffled_bag(rng: &mut impl rand::Rng) -> Vec<Tile> {
    let mut tiles = full_bag();
    tiles.shuffle(rng);
    tiles
}

/// Build a full, ordered bag whose tiles repeat AUGUST in sequence.
pub fn august_bag() -> Vec<Tile> {
    let size = full_bag().len();
    AUGUST_LETTERS
        .iter()
        .copied()
        .cycle()
        .take(size)
        .map(Tile::Letter)
        .collect()
}

/// Build the August bag and shuffle it with the provided RNG.
pub fn shuffled_august_bag(rng: &mut impl rand::Rng) -> Vec<Tile> {
    let mut tiles = august_bag();
    tiles.shuffle(rng);
    tiles
}

/// Draw up to `count` tiles off the back of the bag.
pub fn draw(bag: &mut Vec<Tile>, count: usize) -> Vec<Tile> {
    let take = count.min(bag.len());
    bag.split_off(bag.len() - take)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_bag_has_100_tiles() {
        assert_eq!(full_bag().len(), 100);
    }

    #[test]
    fn full_bag_has_two_blanks() {
        let blanks = full_bag()
            .into_iter()
            .filter(|tile| matches!(tile, Tile::Blank))
            .count();
        assert_eq!(blanks, BLANK_COUNT);
    }

    #[test]
    fn august_bag_matches_full_bag_length() {
        assert_eq!(august_bag().len(), full_bag().len());
    }

    #[test]
    fn august_bag_has_no_blanks() {
        assert!(!august_bag().iter().any(|tile| matches!(tile, Tile::Blank)));
    }

    #[test]
    fn august_bag_only_contains_august_letters() {
        assert!(
            august_bag()
                .iter()
                .all(|tile| matches!(tile, Tile::Letter('A' | 'U' | 'G' | 'S' | 'T')))
        );
    }

    #[test]
    fn draw_takes_requested_count() {
        let mut bag = full_bag();
        let drawn = draw(&mut bag, 7);
        assert_eq!(drawn.len(), 7);
        assert_eq!(bag.len(), 93);
    }

    #[test]
    fn draw_is_clamped_to_bag_size() {
        let mut bag = vec![Tile::Blank, Tile::Letter('A')];
        let drawn = draw(&mut bag, 7);
        assert_eq!(drawn.len(), 2);
        assert!(bag.is_empty());
    }
}
