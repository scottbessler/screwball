use std::{env, net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{
    Router, middleware,
    routing::{get, post},
};
use tower_http::{services::ServeDir, trace::TraceLayer};

use crate::{dict::Dictionary, routes, session, store::GameStore};

#[derive(Clone)]
pub struct AppState {
    pub dict: Arc<Dictionary>,
    pub store: Arc<GameStore>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::index))
        .route("/healthcheck", get(routes::healthcheck))
        .route("/demo", get(routes::demo))
        .route("/games", post(routes::create_game))
        .route("/games/{id}", get(routes::game_page))
        .route("/games/{id}/join", post(routes::join_game))
        .route("/games/{id}/state", get(routes::game_state))
        .route("/games/{id}/move", post(routes::submit_move))
        .nest_service("/public", ServeDir::new("public"))
        .layer(middleware::from_fn(session::attach_session))
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

    let app = router(AppState { dict, store });

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("listening on port {}", bound.port());
    tracing::info!("listening on http://{bound}");
    axum::serve(listener, app).await?;
    Ok(())
}
