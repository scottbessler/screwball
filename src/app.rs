use std::{env, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::FromRef,
    routing::{get, post},
};
use axum_extra::extract::cookie::Key;
use tower_http::{services::ServeDir, trace::TraceLayer};
use webauthn_rs::prelude::{Url, Webauthn, WebauthnBuilder};

use crate::{auth, dict::Dictionary, routes, store::GameStore, users::UserStore};

#[derive(Clone)]
pub struct AppState {
    pub dict: Arc<Dictionary>,
    pub store: Arc<GameStore>,
    pub users: Arc<UserStore>,
    pub webauthn: Arc<Webauthn>,
    pub key: Key,
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
        .route("/healthcheck", get(routes::healthcheck))
        .route("/demo", get(routes::demo))
        .route("/auth/register/begin", post(auth::register_begin))
        .route("/auth/register/finish", post(auth::register_finish))
        .route("/auth/login/begin", post(auth::login_begin))
        .route("/auth/login/finish", post(auth::login_finish))
        .route("/auth/logout", post(auth::logout))
        .route("/games", post(routes::create_game))
        .route("/api/my-games", get(routes::my_games))
        .route("/games/{id}", get(routes::game_page))
        .route("/games/{id}/join", post(routes::join_game))
        .route("/games/{id}/state", get(routes::game_state))
        .route("/games/{id}/move", post(routes::submit_move))
        .route("/games/{id}/hint", post(routes::hint))
        .nest_service("/public", ServeDir::new("public"))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let dict = Arc::new(Dictionary::load()?);
    tracing::info!("loaded dictionary with {} words", dict.word_count());

    let data_path = env::var("DATA_PATH").unwrap_or_else(|_| "data".to_string());
    let store = Arc::new(GameStore::load(&data_path).await?);
    let users = Arc::new(UserStore::load(&data_path).await?);
    tracing::info!("loaded {} registered users", users.count().await);

    let webauthn = Arc::new(build_webauthn()?);
    let key = load_key();

    let app = router(AppState {
        dict,
        store,
        users,
        webauthn,
        key,
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

/// Derive the cookie-signing key from `SESSION_SECRET`. Falls back to an
/// ephemeral key (with a warning) so local runs work out of the box.
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
            tracing::warn!(
                "SESSION_SECRET is not set; using an ephemeral key (sessions reset on restart)"
            );
            Key::generate()
        }
    }
}
