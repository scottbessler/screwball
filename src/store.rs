use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Utc;
use tokio::sync::{Mutex as AsyncMutex, broadcast};
use uuid::Uuid;

use crate::{error::AppError, models::Game};

/// One game plus the lock that serializes writes to it. The game is held behind
/// an `Arc` so reads clone a pointer (and the registry lock is released
/// immediately) instead of deep-cloning under contention.
struct Entry {
    game: Arc<Game>,
    write_lock: Arc<AsyncMutex<()>>,
}

/// In-memory registry of games backed by atomic per-game JSON files on disk.
///
/// The registry map is guarded by a short-lived std mutex held only long enough
/// to clone an `Arc` — never across `.await`, so polling reads (`get`/`list`)
/// never block on disk I/O or on a bot turn. Writes to a *single* game serialize
/// on that game's own `write_lock`; different games (and all reads) proceed
/// concurrently.
pub struct GameStore {
    games: Mutex<HashMap<Uuid, Entry>>,
    dir: PathBuf,
    /// Fires the id of a game whenever it changes, so SSE subscribers can push
    /// fresh state to clients instead of having them poll.
    changes: broadcast::Sender<Uuid>,
}

impl GameStore {
    /// Load all persisted games under `<data_root>/games`, creating the
    /// directory if needed. Invalid files are skipped with a warning.
    pub async fn load(data_root: impl Into<PathBuf>) -> Result<Self, AppError> {
        let dir = data_root.into().join("games");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(AppError::internal)?;

        let mut games = HashMap::new();
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(AppError::internal)?;
        while let Some(entry) = entries.next_entry().await.map_err(AppError::internal)? {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(text) => match serde_json::from_str::<Game>(&text) {
                    Ok(game) => {
                        games.insert(game.id, Entry::new(game));
                    }
                    Err(err) => {
                        tracing::warn!(path = %path.display(), error = %err, "skipping invalid game file")
                    }
                },
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "could not read game file")
                }
            }
        }

        Ok(Self {
            games: Mutex::new(games),
            dir,
            changes: broadcast::channel(256).0,
        })
    }

    /// Subscribe to game-change notifications (the changed game's id).
    pub fn subscribe(&self) -> broadcast::Receiver<Uuid> {
        self.changes.subscribe()
    }

    pub async fn insert(&self, game: Game) -> Result<(), AppError> {
        let id = game.id;
        self.persist(&game).await?;
        self.games.lock().unwrap().insert(id, Entry::new(game));
        let _ = self.changes.send(id);
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Option<Game> {
        self.games
            .lock()
            .unwrap()
            .get(&id)
            .map(|entry| (*entry.game).clone())
    }

    /// Games sorted newest-first, for listing.
    pub async fn list(&self) -> Vec<Game> {
        let mut games: Vec<Game> = {
            let map = self.games.lock().unwrap();
            map.values().map(|entry| (*entry.game).clone()).collect()
        };
        games.sort_by_key(|game| std::cmp::Reverse(game.created_at));
        games
    }

    /// Mutate a game, persisting the result before it becomes visible in memory.
    ///
    /// The closure (which may run a bot turn) and the disk write happen while
    /// holding only this game's `write_lock` — not the registry lock — so other
    /// games and all reads are unaffected. The closure runs on a fresh clone and
    /// the new state is committed back only after a successful write, so a failed
    /// write leaves disk and memory on the previous state (no divergence), and
    /// concurrent writes to the same game serialize.
    pub async fn update<R>(&self, id: Uuid, f: impl FnOnce(&mut Game) -> R) -> Result<R, AppError> {
        let write_lock = self
            .games
            .lock()
            .unwrap()
            .get(&id)
            .map(|entry| entry.write_lock.clone())
            .ok_or_else(|| AppError::not_found("game not found"))?;
        // Serialize writes to this game; held across the closure and the persist.
        let _guard = write_lock.lock().await;

        // Read the current state only after taking the write lock, so we never
        // build on a snapshot another writer is about to replace.
        let current = self
            .games
            .lock()
            .unwrap()
            .get(&id)
            .map(|entry| entry.game.clone())
            .ok_or_else(|| AppError::not_found("game not found"))?;

        let mut working = (*current).clone();
        let outcome = f(&mut working);
        if working != *current {
            working.updated_at = Utc::now();
        }
        self.persist(&working).await?;
        if let Some(entry) = self.games.lock().unwrap().get_mut(&id) {
            entry.game = Arc::new(working);
        }
        let _ = self.changes.send(id);
        Ok(outcome)
    }

    async fn persist(&self, game: &Game) -> Result<(), AppError> {
        let path = self.dir.join(format!("{}.json", game.id));
        let tmp = temp_path(&path);
        let bytes = serde_json::to_vec_pretty(game).map_err(AppError::internal)?;
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(AppError::internal)?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}

impl Entry {
    fn new(game: Game) -> Self {
        Self {
            game: Arc::new(game),
            write_lock: Arc::new(AsyncMutex::new(())),
        }
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    path.with_extension(format!("json.tmp-{nanos}"))
}
