use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use screwball::{app, dict::Dictionary};
use tower::ServiceExt;

fn router() -> axum::Router {
    let dict = Arc::new(Dictionary::from_words("CAT\nCATS\nAT\n"));
    app::router(app::AppState { dict })
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn healthcheck_returns_ok() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/healthcheck")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "OK");
}

#[tokio::test]
async fn home_page_renders() {
    let response = router()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_string(response).await.contains("Screwball"));
}

#[tokio::test]
async fn demo_page_renders_board() {
    let response = router()
        .oneshot(Request::builder().uri("/demo").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("class=\"board\""));
    // 225 cells on the grid.
    assert_eq!(html.matches("class=\"cell").count(), 225);
}
