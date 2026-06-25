use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use screwball::{app, dict::Dictionary, store::GameStore};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn router() -> Router {
    let dict = Arc::new(Dictionary::from_words("CAT\nCATS\nAT\nHE\nHEY\n"));
    let dir = std::env::temp_dir().join(format!(
        "screwball-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Arc::new(GameStore::load(&dir).await.unwrap());
    app::router(app::AppState { dict, store })
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn sid_cookie(response: &axum::response::Response) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let value = value.to_str().ok()?;
            let first = value.split(';').next()?;
            first.starts_with("sid=").then(|| first.to_string())
        })
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn healthcheck_returns_ok() {
    let response = router()
        .await
        .oneshot(get("/healthcheck", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "OK");
}

#[tokio::test]
async fn home_page_renders() {
    let response = router().await.oneshot(get("/", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_string(response).await.contains("Screwball"));
}

#[tokio::test]
async fn demo_page_renders_board() {
    let response = router().await.oneshot(get("/demo", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("class=\"board\""));
    assert_eq!(html.matches("class=\"cell").count(), 225);
}

#[tokio::test]
async fn create_join_and_play_flow() {
    let app = router().await;

    // First request mints a session cookie for "us".
    let home = app.clone().oneshot(get("/", None)).await.unwrap();
    let cookie = sid_cookie(&home).expect("session cookie issued");

    // Create a game: us + a hard bot.
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/games")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("your_name=Tester&seat2=hard"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::SEE_OTHER);
    let location = create
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.starts_with("/games/"));

    // Our view shows us seated at seat 0 with a full rack; the bot's rack is hidden.
    let state = app
        .clone()
        .oneshot(get(&format!("{location}/state"), Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(state.status(), StatusCode::OK);
    let view: Value = serde_json::from_str(&body_string(state).await).unwrap();
    assert_eq!(view["your_seat"], json!(0));
    assert_eq!(view["your_rack"].as_array().unwrap().len(), 7);
    assert_eq!(view["seats"].as_array().unwrap().len(), 2);
    assert_eq!(view["seats"][1]["kind"], "bot");

    // A stranger sees no rack and no seat.
    let stranger = app
        .clone()
        .oneshot(get(&format!("{location}/state"), None))
        .await
        .unwrap();
    let stranger_view: Value = serde_json::from_str(&body_string(stranger).await).unwrap();
    assert!(stranger_view["your_seat"].is_null());
    assert!(stranger_view["your_rack"].is_null());

    // We pass; the hard bot then takes its turn automatically.
    let mv = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{location}/move"))
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "kind": "pass" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mv.status(), StatusCode::OK);
    let after: Value = serde_json::from_str(&body_string(mv).await).unwrap();
    // It should be our turn again (seat 0) after the bot moved.
    assert_eq!(after["turn"], json!(0));
    assert!(!after["moves"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn move_out_of_turn_is_rejected() {
    let app = router().await;
    let home = app.clone().oneshot(get("/", None)).await.unwrap();
    let cookie = sid_cookie(&home).unwrap();

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/games")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("seat2=easy"))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = create
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // A different visitor (no cookie => fresh user) is not seated.
    let mv = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{location}/move"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "kind": "pass" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mv.status(), StatusCode::FORBIDDEN);
    let err: Value = serde_json::from_str(&body_string(mv).await).unwrap();
    assert!(err.get("error").is_some());
}
