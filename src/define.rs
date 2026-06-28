//! Word definitions: a cache backed by two free upstream APIs and a JSON
//! snapshot on disk.
//!
//! dictionaryapi.dev is the primary source; Wiktionary's REST API is the
//! fallback because it covers the obscure TWL06 Scrabble words (EUOI, VOZHD,
//! QOPH, AALII, ...) that dictionaryapi.dev is missing.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use isahc::AsyncReadResponseExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, Serialize, Deserialize)]
pub struct Definition {
    pub pos: String,
    pub text: String,
}

/// Cache of `WORD -> definition`. `None` is a negative result (looked up, no
/// definition) so we don't re-hit upstream within a run. Positive results are
/// snapshotted to `<data>/definitions.json`; negatives are not, so they retry
/// after a restart (self-healing if upstream later gains the word).
pub struct DefinitionCache {
    entries: RwLock<HashMap<String, Option<Definition>>>,
    /// On-disk snapshot path, or `None` to disable persistence (tests).
    path: Option<PathBuf>,
    /// Serializes snapshot writes so concurrent saves don't clobber each other.
    write_lock: Mutex<()>,
}

impl Default for DefinitionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DefinitionCache {
    /// Non-persistent cache (used by tests).
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            path: None,
            write_lock: Mutex::new(()),
        }
    }

    /// Load the snapshot under `<data_root>/definitions.json`, creating the
    /// directory if needed. A missing or invalid file starts an empty cache.
    pub async fn load(data_root: impl Into<PathBuf>) -> Self {
        let root = data_root.into();
        let _ = tokio::fs::create_dir_all(&root).await;
        let path = root.join("definitions.json");
        let entries = match tokio::fs::read_to_string(&path).await {
            Ok(text) => serde_json::from_str::<HashMap<String, Definition>>(&text)
                .map(|m| m.into_iter().map(|(k, v)| (k, Some(v))).collect())
                .unwrap_or_else(|err| {
                    tracing::warn!(error = %err, "ignoring invalid definitions cache");
                    HashMap::new()
                }),
            Err(_) => HashMap::new(),
        };
        Self {
            entries: RwLock::new(entries),
            path: Some(path),
            write_lock: Mutex::new(()),
        }
    }

    /// Look up `word`, fetching and caching on a miss. Words must be ASCII
    /// alphabetic (callers validate); the upstream URL needs no escaping then.
    pub async fn lookup(&self, word: &str) -> Option<Definition> {
        let key = word.to_ascii_uppercase();
        if let Some(hit) = self.entries.read().await.get(&key) {
            return hit.clone();
        }
        // ponytail: concurrent misses for the same word may both fetch; the
        // lookups are idempotent so the last write just wins. Add a per-key
        // lock only if upstream rate limits bite.
        let fetched = fetch_upstream(&key.to_ascii_lowercase()).await;
        self.entries.write().await.insert(key, fetched.clone());
        if fetched.is_some() {
            self.snapshot().await;
        }
        fetched
    }

    /// Pre-fetch definitions for `words`, skipping any already cached. Run from
    /// a background task after a move so the client's first lookup is a cache hit.
    pub async fn warm(&self, words: &[String]) {
        let mut added = false;
        for word in words {
            let key = word.to_ascii_uppercase();
            if self.entries.read().await.contains_key(&key) {
                continue;
            }
            let fetched = fetch_upstream(&key.to_ascii_lowercase()).await;
            added |= fetched.is_some();
            self.entries.write().await.insert(key, fetched);
        }
        if added {
            self.snapshot().await;
        }
    }

    /// Atomically write the positive entries to disk. Failures are logged, not
    /// propagated — a cache write must never break gameplay.
    /// ponytail: rewrites the whole file per new word. Fine at game scale;
    /// switch to an append log if the cache ever grows into the thousands.
    async fn snapshot(&self) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let _guard = self.write_lock.lock().await;
        let positives: HashMap<String, Definition> = {
            let entries = self.entries.read().await;
            entries
                .iter()
                .filter_map(|(k, v)| v.as_ref().map(|d| (k.clone(), d.clone())))
                .collect()
        };
        if let Err(err) = write_atomic(path, &positives).await {
            tracing::warn!(error = %err, "could not snapshot definitions cache");
        }
    }
}

async fn write_atomic(path: &Path, value: &HashMap<String, Definition>) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp = path.with_extension(format!("json.tmp-{nanos}"));
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}

/// `word` is lowercase ASCII alphabetic, so it needs no URL escaping.
async fn fetch_upstream(word: &str) -> Option<Definition> {
    let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{word}");
    if let Some(def) = get_json(&url).await.as_ref().and_then(parse_dictapi) {
        return Some(def);
    }
    let url = format!("https://en.wiktionary.org/api/rest_v1/page/definition/{word}");
    get_json(&url).await.as_ref().and_then(parse_wiktionary)
}

async fn get_json(url: &str) -> Option<Value> {
    let mut resp = isahc::get_async(url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    serde_json::from_str(&resp.text().await.ok()?).ok()
}

fn parse_dictapi(v: &Value) -> Option<Definition> {
    let meaning = v.get(0)?.get("meanings")?.get(0)?;
    let text = meaning
        .get("definitions")?
        .get(0)?
        .get("definition")?
        .as_str()?;
    Some(Definition {
        pos: meaning
            .get("partOfSpeech")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        text: text.to_string(),
    })
}

fn parse_wiktionary(v: &Value) -> Option<Definition> {
    for entry in v.get("en")?.as_array()? {
        let raw = entry
            .get("definitions")
            .and_then(|d| d.get(0))
            .and_then(|d| d.get("definition"))
            .and_then(Value::as_str);
        let Some(raw) = raw else { continue };
        let text = strip_html(raw);
        if text.is_empty() {
            continue;
        }
        return Some(Definition {
            pos: entry
                .get("partOfSpeech")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase(),
            text,
        });
    }
    None
}

/// Strip HTML tags and decode the few entities Wiktionary emits.
/// ponytail: naive char scan, not a real HTML parser — fine for tags + basic
/// entities; upgrade only if upstream markup gets richer.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        let html = "A <a href=\"/x\">Soviet</a> leader &amp; chief.\n";
        assert_eq!(strip_html(html), "A Soviet leader & chief.");
    }

    #[test]
    fn parses_dictapi_shape() {
        let v: Value = serde_json::from_str(
            r#"[{"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"a test"}]}]}]"#,
        )
        .unwrap();
        let def = parse_dictapi(&v).unwrap();
        assert_eq!(def.pos, "noun");
        assert_eq!(def.text, "a test");
    }

    #[test]
    fn parses_wiktionary_shape() {
        let v: Value = serde_json::from_str(
            r#"{"en":[{"partOfSpeech":"Noun","definitions":[{"definition":"A <a>Soviet</a> leader."}]}]}"#,
        )
        .unwrap();
        let def = parse_wiktionary(&v).unwrap();
        assert_eq!(def.pos, "noun");
        assert_eq!(def.text, "A Soviet leader.");
    }

    #[tokio::test]
    async fn snapshot_persists_positives_and_reloads() {
        let dir = std::env::temp_dir().join(format!(
            "screwball-defs-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = DefinitionCache::load(&dir).await;
        cache.entries.write().await.insert(
            "QI".to_string(),
            Some(Definition {
                pos: "noun".into(),
                text: "life force".into(),
            }),
        );
        // Negatives must not be written.
        cache.entries.write().await.insert("ZZ".to_string(), None);
        cache.snapshot().await;

        let reloaded = DefinitionCache::load(&dir).await;
        let entries = reloaded.entries.read().await;
        assert_eq!(
            entries.get("QI").unwrap().as_ref().unwrap().text,
            "life force"
        );
        assert!(!entries.contains_key("ZZ"));
    }
}
