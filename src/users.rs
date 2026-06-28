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
    #[serde(default)]
    pub push_subscriptions: Vec<PushSubscription>,
    pub created_at: DateTime<Utc>,
}

/// Browser Push API subscription persisted per user. This mirrors the
/// `PushSubscription.toJSON()` payload used by service workers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushSubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
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
    /// becomes visible in memory (mirrors `GameStore::update`). Keeps the
    /// username index consistent if the closure changes the username, and
    /// fails the update if the new username is already claimed by someone else.
    pub async fn update<R>(&self, id: Uuid, f: impl FnOnce(&mut User) -> R) -> Result<R, AppError> {
        let mut guard = self.index.lock().await;
        let mut working = guard
            .by_id
            .get(&id)
            .ok_or_else(|| AppError::not_found("user not found"))?
            .clone();
        let old_key = normalize_username(&working.username);
        let outcome = f(&mut working);
        let new_key = normalize_username(&working.username);
        if new_key != old_key
            && guard
                .by_username
                .get(&new_key)
                .is_some_and(|existing| *existing != id)
        {
            return Err(AppError::conflict("that username is already taken"));
        }
        self.persist(&working).await?;
        if new_key != old_key {
            guard.by_username.remove(&old_key);
            guard.by_username.insert(new_key, id);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("screwball-users-{nanos}-{}", Uuid::new_v4()))
    }

    fn sample(username: &str) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            display_name: username.to_string(),
            credentials: Vec::new(),
            push_subscriptions: Vec::new(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn update_keeps_username_index_consistent_on_rename() {
        let store = UserStore::load(scratch_dir()).await.unwrap();
        let user = sample("Alice");
        let id = user.id;
        store.insert(user).await.unwrap();

        store
            .update(id, |u| u.username = "Bob".to_string())
            .await
            .unwrap();

        assert!(store.get_by_username("alice").await.is_none());
        assert_eq!(store.get_by_username("BOB").await.unwrap().id, id);
        assert!(store.username_taken("bob").await);
    }

    #[tokio::test]
    async fn update_rejects_rename_to_taken_username() {
        let store = UserStore::load(scratch_dir()).await.unwrap();
        let alice = sample("alice");
        let alice_id = alice.id;
        store.insert(alice).await.unwrap();
        store.insert(sample("bob")).await.unwrap();

        let result = store
            .update(alice_id, |u| u.username = "BOB".to_string())
            .await;

        assert!(result.is_err());
        // The rejected rename leaves both index entries intact.
        assert_eq!(store.get_by_username("alice").await.unwrap().id, alice_id);
        assert!(store.username_taken("bob").await);
    }
}
