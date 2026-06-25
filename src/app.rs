use std::{env, net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::Router;
use tower_http::{services::ServeDir, trace::TraceLayer};

use crate::{dict::Dictionary, routes};

#[derive(Clone)]
pub struct AppState {
    pub dict: Arc<Dictionary>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(routes::index))
        .route("/healthcheck", axum::routing::get(routes::healthcheck))
        .route("/demo", axum::routing::get(routes::demo))
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
    let app = router(AppState { dict });

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("listening on port {}", bound.port());
    tracing::info!("listening on http://{bound}");
    axum::serve(listener, app).await?;
    Ok(())
}
