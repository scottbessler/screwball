//! Word definitions: parsed from the bundled NWL2023 word list, which includes
//! a short definition and part of speech for every valid word.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
pub struct Definition {
    pub pos: String,
    pub text: String,
}

/// Cache of `WORD -> definition` built from the bundled word list.
pub struct DefinitionCache {
    entries: RwLock<HashMap<String, Definition>>,
}

impl Default for DefinitionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DefinitionCache {
    /// Non-persistent, empty cache (used by tests).
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Build a cache from a newline-separated word list that may also carry
    /// NWL2023-style definitions.
    pub fn from_words(content: &str) -> Self {
        Self {
            entries: RwLock::new(parse_definitions(content)),
        }
    }

    /// Look up `word`. Words are normalized to uppercase ASCII.
    pub async fn lookup(&self, word: &str) -> Option<Definition> {
        let key = word.to_ascii_uppercase();
        self.entries.read().await.get(&key).cloned()
    }

    /// Pre-fetch definitions for `words`. With the bundled list this is a no-op,
    /// because every definition is already loaded.
    pub async fn warm(&self, _words: &[String]) {}
}

#[derive(Clone)]
struct Sense {
    pos: String,
    text: String,
    xref: Option<String>,
}

fn parse_definitions(content: &str) -> HashMap<String, Definition> {
    let mut raw: HashMap<String, Vec<Sense>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((word, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let word = word.to_ascii_uppercase();
        if word.len() < 2 {
            continue;
        }
        let senses = parse_senses(rest.trim_start());
        if !senses.is_empty() {
            raw.insert(word, senses);
        }
    }

    let mut memo: HashMap<String, Definition> = HashMap::with_capacity(raw.len());
    for word in raw.keys().cloned().collect::<Vec<_>>() {
        resolve_word(&word, &raw, &mut memo);
    }

    memo
}

fn parse_senses(rest: &str) -> Vec<Sense> {
    rest.split(" / ")
        .map(|s| s.split_once(':').map(|(a, _)| a).unwrap_or(s))
        .filter_map(parse_sense)
        .collect()
}

fn parse_sense(s: &str) -> Option<Sense> {
    let s = s.trim();
    let bracket = s.find('[')?;
    let text = s[..bracket].trim();
    let after = &s[bracket + 1..];
    let pos_end = after
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after.len());
    let pos = expand_pos(&after[..pos_end]).to_string();
    let (text, xref) = parse_xref(text);
    let text = if xref.is_none() {
        expand_markup(&text)
    } else {
        String::new()
    };
    Some(Sense { pos, text, xref })
}

fn parse_xref(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('<') {
        return (text.to_string(), None);
    }

    let after_lt = &trimmed[1..].trim_start();

    // "< WORD, definition" references a specific sense of another word.
    if let Some((prefix, rest)) = after_lt.split_once(", ")
        && prefix.chars().all(|c| c.is_ascii_alphabetic())
    {
        return (rest.trim_start().to_string(), None);
    }

    // "<word=pos>" is a cross-reference to another entry.
    if let Some(gt) = trimmed.find('>') {
        let inside = &trimmed[1..gt];
        if let Some((word, _pos)) = inside.split_once('=') {
            let word = word.trim();
            if word.chars().all(|c| c.is_ascii_alphabetic()) {
                return (String::new(), Some(word.to_ascii_uppercase()));
            }
        }
    }

    (text.to_string(), None)
}

fn expand_pos(pos: &str) -> &str {
    if pos.eq_ignore_ascii_case("n") {
        "noun"
    } else if pos.eq_ignore_ascii_case("v") {
        "verb"
    } else if pos.eq_ignore_ascii_case("adj") {
        "adjective"
    } else if pos.eq_ignore_ascii_case("adv") {
        "adverb"
    } else if pos.eq_ignore_ascii_case("interj") {
        "interjection"
    } else if pos.eq_ignore_ascii_case("pron") {
        "pronoun"
    } else if pos.eq_ignore_ascii_case("prep") {
        "preposition"
    } else if pos.eq_ignore_ascii_case("conj") {
        "conjunction"
    } else if pos.eq_ignore_ascii_case("article") {
        "article"
    } else {
        pos
    }
}

fn expand_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        let close = match ch {
            '{' => '}',
            '<' => '>',
            _ => {
                out.push(ch);
                continue;
            }
        };

        let mut content = String::new();
        let mut found = false;
        for c in chars.by_ref() {
            if c == close {
                found = true;
                break;
            }
            content.push(c);
        }
        if !found {
            continue;
        }

        let word = content
            .split_once('=')
            .map(|(w, _)| w)
            .unwrap_or(&content)
            .trim();
        if word.eq_ignore_ascii_case("mdash") {
            out.push('\u{2014}');
        } else {
            out.push_str(word);
        }
    }

    out
}

fn resolve_word(
    word: &str,
    raw: &HashMap<String, Vec<Sense>>,
    memo: &mut HashMap<String, Definition>,
) -> Option<Definition> {
    if let Some(def) = memo.get(word) {
        return Some(def.clone());
    }

    resolve_word_inner(word, raw, memo, &mut HashSet::new())
}

fn resolve_word_inner(
    word: &str,
    raw: &HashMap<String, Vec<Sense>>,
    memo: &mut HashMap<String, Definition>,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<Definition> {
    if let Some(def) = memo.get(word) {
        return Some(def.clone());
    }

    let senses = raw.get(word)?;

    if !visiting.insert(word.to_string()) {
        return None;
    }

    let mut positions = Vec::new();
    let mut texts = Vec::new();

    for sense in senses {
        if let Some(target) = &sense.xref {
            if let Some(def) = resolve_word_inner(target, raw, memo, visiting) {
                positions.push(def.pos.clone());
                texts.push(def.text.clone());
            } else {
                positions.push(sense.pos.clone());
                texts.push(target.clone());
            }
            continue;
        }

        if !sense.text.is_empty() {
            positions.push(sense.pos.clone());
            texts.push(sense.text.clone());
        }
    }

    visiting.remove(word);

    if texts.is_empty() {
        return None;
    }

    let def = Definition {
        pos: dedupe_join(&positions),
        text: dedupe_join(&texts),
    };
    memo.insert(word.to_string(), def.clone());
    Some(def)
}

fn dedupe_join(items: &[String]) -> String {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.as_str()) {
            out.push(item.as_str());
        }
    }
    out.join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nwl2023_definitions_parse() {
        let cache = DefinitionCache::from_words(crate::dict::DEFAULT_WORDS);

        let qi = cache.lookup("QI").await.unwrap();
        assert_eq!(qi.pos, "noun");
        assert!(qi.text.contains("vital force"));

        let zzz = cache.lookup("ZZZ").await.unwrap();
        assert_eq!(zzz.pos, "interjection");
        assert!(zzz.text.contains("snoring"));

        let go = cache.lookup("GO").await.unwrap();
        assert!(go.pos.contains("noun"));
        assert!(go.pos.contains("verb"));

        let goes = cache.lookup("GOES").await.unwrap();
        assert_eq!(goes.pos, "verb");
        assert!(goes.text.contains("move along"));

        let has = cache.lookup("HAS").await.unwrap();
        assert!(has.pos.contains("noun"));
        assert!(has.pos.contains("verb"));

        assert!(cache.lookup("NOTAWORD").await.is_none());
    }

    #[test]
    fn expands_markup_and_pos() {
        assert_eq!(expand_markup("a {protein=n}"), "a protein");
        assert_eq!(
            expand_markup("a woman or girl {mdash} usually"),
            "a woman or girl \u{2014} usually"
        );
        assert_eq!(expand_pos("n"), "noun");
        assert_eq!(expand_pos("adj"), "adjective");
    }

    #[tokio::test]
    async fn empty_cache_returns_none() {
        let cache = DefinitionCache::new();
        assert!(cache.lookup("QI").await.is_none());
    }
}
