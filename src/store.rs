use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{error::AppError, models::Game};

/// In-memory registry of games backed by atomic per-game JSON files on disk.
pub struct GameStore {
    games: Mutex<HashMap<Uuid, Game>>,
    dir: PathBuf,
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
                        games.insert(game.id, game);
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
        })
    }

    pub async fn insert(&self, game: Game) -> Result<(), AppError> {
        self.persist(&game).await?;
        self.games.lock().await.insert(game.id, game);
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Option<Game> {
        self.games.lock().await.get(&id).cloned()
    }

    /// Games sorted newest-first, for listing.
    pub async fn list(&self) -> Vec<Game> {
        let mut games: Vec<Game> = self.games.lock().await.values().cloned().collect();
        games.sort_by_key(|game| std::cmp::Reverse(game.created_at));
        games
    }

    /// Mutate a game under the registry lock, persisting the result before it
    /// becomes visible in memory. The closure runs on a clone, which is written
    /// to disk while the lock is held; only on a successful write is the new
    /// state committed back to the map. A failed write therefore leaves both
    /// disk and memory on the previous state (no silent divergence), and the
    /// serialized persist/commit ordering prevents concurrent updates from
    /// racing. The closure is synchronous, so move application and bot turns run
    /// atomically with respect to other requests for the same game.
    pub async fn update<R>(&self, id: Uuid, f: impl FnOnce(&mut Game) -> R) -> Result<R, AppError> {
        let mut guard = self.games.lock().await;
        let mut working = guard
            .get(&id)
            .ok_or_else(|| AppError::not_found("game not found"))?
            .clone();
        let outcome = f(&mut working);
        working.updated_at = Utc::now();
        self.persist(&working).await?;
        guard.insert(id, working);
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

fn temp_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    path.with_extension(format!("json.tmp-{nanos}"))
}
