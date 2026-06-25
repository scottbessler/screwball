use axum::response::Html;

use crate::{models::Board, render};

pub async fn index() -> Html<String> {
    Html(render::home_page())
}

pub async fn healthcheck() -> &'static str {
    "OK"
}

pub async fn demo() -> Html<String> {
    Html(render::demo_page(&Board::new()))
}
