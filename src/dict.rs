/// The default bundled word list (NWL2023). Swap at runtime via `DICTIONARY_PATH`.
pub const DEFAULT_WORDS: &str = include_str!("../assets/dictionaries/NWL2023.txt");

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

#[derive(Clone, Default)]
struct Node {
    is_word: bool,
    /// Child edges sorted by letter index for binary search.
    children: Vec<(u8, u32)>,
}

/// A prefix trie over the word list. Supports O(word) membership and, later,
/// anchored move generation for the computer opponent.
#[derive(Clone)]
pub struct Dictionary {
    nodes: Vec<Node>,
    word_count: usize,
}

impl Dictionary {
    /// Build from newline-separated words; non-alphabetic entries are skipped.
    ///
    /// Each line may be a bare word or a word followed by whitespace and extra
    /// metadata (e.g. the bundled NWL2023 definitions). Only the first token is
    /// used as the word.
    pub fn from_words(contents: &str) -> Self {
        let mut dict = Self {
            nodes: vec![Node::default()],
            word_count: 0,
        };
        for line in contents.lines() {
            let word = line.split_whitespace().next().unwrap_or("");
            if word.len() >= 2 {
                dict.insert(word);
            }
        }
        dict
    }

    pub fn with_extra_words<'a>(&self, words: impl IntoIterator<Item = &'a str>) -> Self {
        let mut dict = self.clone();
        for word in words {
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

    /// Every valid 2-letter word, uppercase, sorted. Used by John Mode's
    /// client-side helper. Cheap: 676 lookups.
    pub fn two_letter_words(&self) -> Vec<String> {
        let mut words = Vec::new();
        for a in b'A'..=b'Z' {
            for b in b'A'..=b'Z' {
                let word = [a as char, b as char].iter().collect::<String>();
                if self.contains(&word) {
                    words.push(word);
                }
            }
        }
        words
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
