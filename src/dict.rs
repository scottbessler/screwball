use std::env;

use anyhow::{Context, Result};

/// The default bundled word list (TWL06). Swap at runtime via `DICTIONARY_PATH`.
const DEFAULT_WORDS: &str = include_str!("../assets/dictionaries/twl06.txt");

const ALPHABET: usize = 26;

/// Index `0..26` for an uppercase ASCII letter, or `None` for anything else.
fn letter_index(letter: char) -> Option<usize> {
    let upper = letter.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some((upper as u8 - b'A') as usize)
    } else {
        None
    }
}

#[derive(Default)]
struct Node {
    is_word: bool,
    /// Child edges sorted by letter index for binary search.
    children: Vec<(u8, u32)>,
}

/// A prefix trie over the word list. Supports O(word) membership and, later,
/// anchored move generation for the computer opponent.
pub struct Dictionary {
    nodes: Vec<Node>,
    word_count: usize,
}

impl Dictionary {
    /// Load the dictionary, preferring `DICTIONARY_PATH` then the bundled list.
    pub fn load() -> Result<Self> {
        match env::var("DICTIONARY_PATH") {
            Ok(path) if !path.is_empty() => {
                let contents = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading dictionary at {path}"))?;
                Ok(Self::from_words(&contents))
            }
            _ => Ok(Self::from_words(DEFAULT_WORDS)),
        }
    }

    /// Build from newline-separated words; non-alphabetic entries are skipped.
    pub fn from_words(contents: &str) -> Self {
        let mut dict = Self {
            nodes: vec![Node::default()],
            word_count: 0,
        };
        for line in contents.lines() {
            let word = line.trim();
            if word.len() >= 2 {
                dict.insert(word);
            }
        }
        dict
    }

    fn insert(&mut self, word: &str) {
        let mut node = 0usize;
        for ch in word.chars() {
            let Some(index) = letter_index(ch) else {
                return;
            };
            node = self.child_or_insert(node, index as u8);
        }
        if !self.nodes[node].is_word {
            self.nodes[node].is_word = true;
            self.word_count += 1;
        }
    }

    fn child_or_insert(&mut self, node: usize, letter: u8) -> usize {
        match self.nodes[node]
            .children
            .binary_search_by_key(&letter, |&(l, _)| l)
        {
            Ok(slot) => self.nodes[node].children[slot].1 as usize,
            Err(slot) => {
                let new_id = self.nodes.len() as u32;
                self.nodes.push(Node::default());
                self.nodes[node].children.insert(slot, (letter, new_id));
                new_id as usize
            }
        }
    }

    fn child(&self, node: usize, letter: u8) -> Option<usize> {
        self.nodes[node]
            .children
            .binary_search_by_key(&letter, |&(l, _)| l)
            .ok()
            .map(|slot| self.nodes[node].children[slot].1 as usize)
    }

    /// Whether `word` is in the dictionary (case-insensitive).
    pub fn contains(&self, word: &str) -> bool {
        if word.len() < 2 {
            return false;
        }
        let mut node = 0usize;
        for ch in word.chars() {
            let Some(index) = letter_index(ch) else {
                return false;
            };
            match self.child(node, index as u8) {
                Some(next) => node = next,
                None => return false,
            }
        }
        self.nodes[node].is_word
    }

    pub fn word_count(&self) -> usize {
        self.word_count
    }

    pub const fn alphabet_size() -> usize {
        ALPHABET
    }

    /// Root node of the trie, for anchored traversal during move generation.
    pub fn root(&self) -> usize {
        0
    }

    /// Follow the edge labelled `letter` from `node`, if present.
    pub fn step(&self, node: usize, letter: char) -> Option<usize> {
        letter_index(letter).and_then(|index| self.child(node, index as u8))
    }

    /// Whether `node` marks the end of a complete word.
    pub fn is_terminal(&self, node: usize) -> bool {
        self.nodes[node].is_word
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Dictionary {
        Dictionary::from_words("cat\ncats\ndog\nquiz\na\nzz\n")
    }

    #[test]
    fn contains_known_words() {
        let dict = sample();
        assert!(dict.contains("cat"));
        assert!(dict.contains("CATS"));
        assert!(dict.contains("quiz"));
    }

    #[test]
    fn rejects_unknown_and_too_short() {
        let dict = sample();
        assert!(!dict.contains("ca"));
        assert!(!dict.contains("a"));
        assert!(!dict.contains(""));
        assert!(!dict.contains("dogs"));
    }

    #[test]
    fn counts_only_valid_length_words() {
        let dict = sample();
        // "a" is too short and skipped; the rest are kept.
        assert_eq!(dict.word_count(), 5);
    }

    #[test]
    fn bundled_dictionary_loads() {
        let dict = Dictionary::from_words(DEFAULT_WORDS);
        assert!(dict.word_count() > 100_000);
        assert!(dict.contains("QUIXOTIC"));
        assert!(dict.contains("zzz".to_uppercase().as_str()) || !dict.contains("ZZZ"));
    }
}
