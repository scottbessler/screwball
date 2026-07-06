use std::{env, net::SocketAddr, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{FromRef, FromRequestParts, Request, State},
    http::{HeaderValue, header::CACHE_CONTROL},
    middleware::{Next, from_fn, from_fn_with_state},
    response::Response,
    routing::{get, post},
};
use axum_extra::extract::cookie::Key;
use tower_http::{services::ServeDir, trace::TraceLayer};
use webauthn_rs::prelude::{Url, Webauthn, WebauthnBuilder};

use crate::{
    auth, define::DefinitionCache, dict::Dictionary, push::PushService, routes, session::MaybeUser,
    store::GameStore, users::UserStore,
};

const LOCAL_DEV_SESSION_SECRET: &str =
    "screwball-local-development-session-secret-v1-keep-browser-sessions-across-restarts";

#[derive(Clone)]
pub struct AppState {
    pub dict: Arc<Dictionary>,
    pub defs: Arc<DefinitionCache>,
    pub store: Arc<GameStore>,
    pub users: Arc<UserStore>,
    pub webauthn: Arc<Webauthn>,
    pub key: Key,
    pub push: PushService,
    /// When set, auth skips the WebAuthn ceremony and trusts the username alone.
    /// Dev-only escape hatch — browsers dislike passkeys on localhost.
    pub passkey_disabled: bool,
}

/// Lets the signed-cookie extractors pull the signing key out of `AppState`.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::index))
        .route("/debug/notifications", get(routes::notification_debug_page))
        .route("/debug/touch", get(routes::touch_debug_page))
        .route("/sw.js", get(routes::service_worker))
        .route("/healthcheck", get(routes::healthcheck))
        .route("/auth/register/begin", post(auth::register_begin))
        .route("/auth/register/finish", post(auth::register_finish))
        .route("/auth/login/begin", post(auth::login_begin))
        .route("/auth/login/finish", post(auth::login_finish))
        .route("/auth/logout", post(auth::logout))
        .route("/games", post(routes::create_game))
        .route("/games/new", get(routes::new_game_page))
        .route("/api/my-games", get(routes::my_games))
        .route("/api/push/vapid-public-key", get(routes::push_public_key))
        .route("/api/push/debug", get(routes::push_debug_status))
        .route("/api/push/subscribe", post(routes::push_subscribe))
        .route("/api/push/unsubscribe", post(routes::push_unsubscribe))
        .route("/api/push/test", post(routes::push_test))
        .route("/games/{id}", get(routes::game_page))
        .route("/games/{id}/join", post(routes::join_game))
        .route("/games/{id}/state", get(routes::game_state))
        .route("/games/{id}/events", get(routes::game_events))
        .route("/games/{id}/move", post(routes::submit_move))
        .route("/games/{id}/abandon", post(routes::abandon_game))
        .route("/games/{id}/hint", post(routes::hint))
        .route("/api/define/{word}", get(routes::define))
        .nest_service("/public", ServeDir::new("public"))
        .layer(from_fn(cache_control))
        .layer(from_fn_with_state(state.clone(), log_request))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Log every request's method, path, status, signed-in username, and wall-clock
/// duration so prod latency spikes are visible. Static assets and the health
/// check are skipped to keep the log readable; anything slow is bumped to a
/// warning. Guests log as `-`.
async fn log_request(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    if path.starts_with("/public/") || path == "/healthcheck" {
        return next.run(request).await;
    }

    // Resolve the username from the signed session cookie before the body is
    // consumed; falls back to the user id, then `-` for guests.
    let (mut parts, body) = request.into_parts();
    let user_id = MaybeUser::from_request_parts(&mut parts, &state)
        .await
        .map(|m| m.0)
        .unwrap_or(None);
    let request = Request::from_parts(parts, body);

    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed_ms = start.elapsed().as_millis();
    let status = response.status().as_u16();

    let user = match user_id {
        Some(id) => state
            .users
            .get(id)
            .await
            .map(|u| u.username)
            .unwrap_or_else(|| id.to_string()),
        None => "-".to_string(),
    };

    if elapsed_ms >= 500 {
        tracing::warn!(%method, path, status, user, elapsed_ms, "slow request");
    } else {
        tracing::info!(%method, path, status, user, elapsed_ms, "request");
    }
    response
}

/// Cache policy: release `/public` asset URLs (`?v=hash`) are content-addressed
/// so cache them immutably. Debug builds use no-cache because local files can
/// change without restarting the server, and stale JS/CSS is painful to debug.
async fn cache_control(request: Request, next: Next) -> Response {
    let is_asset = request.uri().path().starts_with("/public/");
    let mut response = next.run(request).await;
    let value = if is_asset && !cfg!(debug_assertions) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            // Default to info so request-duration logs show up in prod even when
            // RUST_LOG is unset.
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    crate::render::set_asset_version(asset_version());

    let dict = Arc::new(Dictionary::load()?);
    tracing::info!("loaded dictionary with {} words", dict.word_count());

    let data_path = env::var("DATA_PATH").unwrap_or_else(|_| "data".to_string());
    let defs = Arc::new(DefinitionCache::load(&data_path).await);
    let store = Arc::new(GameStore::load(&data_path).await?);
    let users = Arc::new(UserStore::load(&data_path).await?);
    tracing::info!("loaded {} registered users", users.count().await);

    let webauthn = Arc::new(build_webauthn()?);
    let key = load_key();
    let push = PushService::from_env().context("failed to configure web push")?;
    if push.is_enabled() {
        tracing::info!("web push notifications enabled");
    } else {
        tracing::warn!("VAPID_PRIVATE_KEY is not set; web push notifications are disabled");
    }
    let passkey_disabled = env_flag("PASSKEY_DISABLED");
    if passkey_disabled {
        tracing::warn!("PASSKEY_DISABLED set; auth trusts username only (dev mode)");
    }

    let app = router(AppState {
        dict,
        defs,
        store,
        users,
        webauthn,
        key,
        push,
        passkey_disabled,
    });

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("listening on port {}", bound.port());
    tracing::info!("listening on http://{bound}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the WebAuthn relying party from `RP_ID` / `RP_ORIGIN` (defaults suit
/// local development on `http://localhost:8080`).
pub fn build_webauthn() -> Result<Webauthn> {
    let rp_id = env::var("RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let rp_origin_raw =
        env::var("RP_ORIGIN").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let rp_origin = Url::parse(&rp_origin_raw).context("RP_ORIGIN must be a valid URL")?;
    WebauthnBuilder::new(&rp_id, &rp_origin)
        .context("invalid WebAuthn relying-party configuration")?
        .rp_name("Screwball")
        .build()
        .context("failed to build WebAuthn relying party")
}

/// A short hash of the static assets, used to cache-bust their URLs. Stable for
/// identical content across restarts; changes when any asset changes.
fn asset_version() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in [
        "public/app.css",
        "public/game.js",
        "public/auth.js",
        "public/touch-debug.js",
        "public/sw.js",
    ] {
        if let Ok(bytes) = std::fs::read(file) {
            bytes.hash(&mut hasher);
        }
    }
    format!("{:x}", hasher.finish())
}

/// Read a boolean env flag (`1`/`true`, case-insensitive); absent or anything
/// else is false.
fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("True")
    )
}

/// Derive the cookie-signing key from `SESSION_SECRET`. Debug builds use a
/// stable local-dev fallback so `cargo run` restarts keep browser sessions;
/// release builds keep the safer ephemeral fallback when the secret is unset.
fn load_key() -> Key {
    match env::var("SESSION_SECRET") {
        Ok(secret) if secret.len() >= 64 => Key::from(secret.as_bytes()),
        Ok(_) => {
            tracing::warn!(
                "SESSION_SECRET is shorter than 64 bytes; using an ephemeral key (sessions reset on restart)"
            );
            Key::generate()
        }
        Err(_) => {
            if cfg!(debug_assertions) {
                tracing::info!("SESSION_SECRET is not set; using stable local-dev session key");
                Key::from(LOCAL_DEV_SESSION_SECRET.as_bytes())
            } else {
                tracing::warn!(
                    "SESSION_SECRET is not set; using an ephemeral key (sessions reset on restart)"
                );
                Key::generate()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cookie::{Cookie as RawCookie, CookieJar};

    use super::*;

    fn signed_sid_value(key: &Key) -> String {
        let mut jar = CookieJar::new();
        jar.signed_mut(key)
            .add(RawCookie::new("sid", "local-dev-user"));
        jar.get("sid").unwrap().value().to_string()
    }

    #[test]
    fn local_dev_session_secret_is_stable() {
        let first = Key::from(LOCAL_DEV_SESSION_SECRET.as_bytes());
        let second = Key::from(LOCAL_DEV_SESSION_SECRET.as_bytes());

        assert_eq!(signed_sid_value(&first), signed_sid_value(&second));
    }

    #[test]
    fn local_dev_session_secret_is_long_enough_for_cookie_key() {
        assert!(LOCAL_DEV_SESSION_SECRET.len() >= 64);
    }
}
