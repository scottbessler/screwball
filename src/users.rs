use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

use crate::error::AppError;

/// A registered player. Identity is established by passkeys only — there is no
/// password. `username` is the unique login handle; `display_name` is shown in
/// games and may be shared by multiple users.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub credentials: Vec<Passkey>,
    pub created_at: DateTime<Utc>,
}

/// Normalize a login handle so lookups are case- and whitespace-insensitive.
pub fn normalize_username(raw: &str) -> String {
    raw.trim().to_lowercase()
}

struct Index {
    by_id: HashMap<Uuid, User>,
    by_username: HashMap<String, Uuid>,
}

/// In-memory registry of users backed by atomic per-user JSON files on disk.
pub struct UserStore {
    index: Mutex<Index>,
    dir: PathBuf,
}

impl UserStore {
    /// Load all persisted users under `<data_root>/users`, creating the
    /// directory if needed. Invalid files are skipped with a warning.
    pub async fn load(data_root: impl Into<PathBuf>) -> Result<Self, AppError> {
        let dir = data_root.into().join("users");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(AppError::internal)?;

        let mut by_id = HashMap::new();
        let mut by_username = HashMap::new();
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(AppError::internal)?;
        while let Some(entry) = entries.next_entry().await.map_err(AppError::internal)? {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(text) => match serde_json::from_str::<User>(&text) {
                    Ok(user) => {
                        by_username.insert(normalize_username(&user.username), user.id);
                        by_id.insert(user.id, user);
                    }
                    Err(err) => {
                        tracing::warn!(path = %path.display(), error = %err, "skipping invalid user file")
                    }
                },
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "could not read user file")
                }
            }
        }

        Ok(Self {
            index: Mutex::new(Index { by_id, by_username }),
            dir,
        })
    }

    pub async fn get(&self, id: Uuid) -> Option<User> {
        self.index.lock().await.by_id.get(&id).cloned()
    }

    pub async fn count(&self) -> usize {
        self.index.lock().await.by_id.len()
    }

    pub async fn get_by_username(&self, username: &str) -> Option<User> {
        let key = normalize_username(username);
        let guard = self.index.lock().await;
        let id = guard.by_username.get(&key)?;
        guard.by_id.get(id).cloned()
    }

    pub async fn username_taken(&self, username: &str) -> bool {
        let key = normalize_username(username);
        self.index.lock().await.by_username.contains_key(&key)
    }

    /// Persist a new user, then commit it to memory and the username index.
    /// Fails if the username is already taken.
    pub async fn insert(&self, user: User) -> Result<(), AppError> {
        let key = normalize_username(&user.username);
        let mut guard = self.index.lock().await;
        if guard.by_username.contains_key(&key) {
            return Err(AppError::conflict("that username is already taken"));
        }
        self.persist(&user).await?;
        guard.by_username.insert(key, user.id);
        guard.by_id.insert(user.id, user);
        Ok(())
    }

    /// Mutate a user under the registry lock, persisting before the change
    /// becomes visible in memory (mirrors `GameStore::update`).
    pub async fn update<R>(&self, id: Uuid, f: impl FnOnce(&mut User) -> R) -> Result<R, AppError> {
        let mut guard = self.index.lock().await;
        let mut working = guard
            .by_id
            .get(&id)
            .ok_or_else(|| AppError::not_found("user not found"))?
            .clone();
        let outcome = f(&mut working);
        self.persist(&working).await?;
        guard.by_id.insert(id, working);
        Ok(outcome)
    }

    async fn persist(&self, user: &User) -> Result<(), AppError> {
        let path = self.dir.join(format!("{}.json", user.id));
        let tmp = temp_path(&path);
        let bytes = serde_json::to_vec_pretty(user).map_err(AppError::internal)?;
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
