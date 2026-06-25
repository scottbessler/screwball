use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use axum_extra::extract::cookie::Key;
use cookie::{Cookie as RawCookie, CookieJar};
use screwball::{app, dict::Dictionary, store::GameStore, users::UserStore};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

/// A test harness bundling the router with the cookie-signing key, so tests can
/// mint authenticated `sid` cookies without running a real passkey ceremony.
struct TestApp {
    router: Router,
    key: Key,
}

async fn test_app() -> TestApp {
    let dict = Arc::new(Dictionary::from_words("CAT\nCATS\nAT\nHE\nHEY\n"));
    let dir = std::env::temp_dir().join(format!(
        "screwball-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Arc::new(GameStore::load(&dir).await.unwrap());
    let users = Arc::new(UserStore::load(&dir).await.unwrap());
    let webauthn = Arc::new(app::build_webauthn().unwrap());
    let key = Key::generate();
    let state = app::AppState {
        dict,
        store,
        users,
        webauthn,
        key: key.clone(),
    };
    TestApp {
        router: app::router(state),
        key,
    }
}

impl TestApp {
    fn router(&self) -> Router {
        self.router.clone()
    }

    /// Mint a signed session cookie for a brand-new user id.
    fn new_session(&self) -> (Uuid, String) {
        let user = Uuid::new_v4();
        (user, self.cookie_for(user))
    }

    /// Build the signed `sid=...` cookie header value for a given user.
    fn cookie_for(&self, user: Uuid) -> String {
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key)
            .add(RawCookie::new("sid", user.to_string()));
        let value = jar.get("sid").unwrap().value().to_string();
        format!("sid={value}")
    }
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn post_form(uri: &str, cookie: Option<&str>, body: &'static str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(body)).unwrap()
}

fn post_json(uri: &str, cookie: Option<&str>, body: String) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(body)).unwrap()
}

fn location_of(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn healthcheck_returns_ok() {
    let app = test_app().await;
    let response = app
        .router()
        .oneshot(get("/healthcheck", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "OK");
}

#[tokio::test]
async fn home_page_logged_out_shows_signin() {
    let app = test_app().await;
    let response = app.router().oneshot(get("/", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Screwball"));
    assert!(html.contains("Sign in with passkey"));
    assert!(html.contains("/public/auth.js"));
}

#[tokio::test]
async fn home_page_logged_in_shows_new_game() {
    let app = test_app().await;
    let (_user, cookie) = app.new_session();
    let response = app.router().oneshot(get("/", Some(&cookie))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("New game"));
    assert!(html.contains("Sign out"));
}

#[tokio::test]
async fn demo_page_renders_board() {
    let app = test_app().await;
    let response = app.router().oneshot(get("/demo", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("class=\"board\""));
    assert_eq!(html.matches("class=\"cell").count(), 225);
}

#[tokio::test]
async fn create_game_requires_auth() {
    let app = test_app().await;
    let response = app
        .router()
        .oneshot(post_form("/games", None, "your_name=Tester&seat2=hard"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_join_and_play_flow() {
    let app = test_app().await;
    let (_user, cookie) = app.new_session();

    // Create a game: us + a hard bot.
    let create = app
        .router()
        .oneshot(post_form(
            "/games",
            Some(&cookie),
            "your_name=Tester&seat2=hard",
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::SEE_OTHER);
    let location = location_of(&create);
    assert!(location.starts_with("/games/"));

    // Our view shows us seated at seat 0 with a full rack; the bot's rack is hidden.
    let state = app
        .router()
        .oneshot(get(&format!("{location}/state"), Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(state.status(), StatusCode::OK);
    let view: Value = serde_json::from_str(&body_string(state).await).unwrap();
    assert_eq!(view["your_seat"], json!(0));
    assert_eq!(view["your_rack"].as_array().unwrap().len(), 7);
    assert_eq!(view["seats"].as_array().unwrap().len(), 2);
    assert_eq!(view["seats"][1]["kind"], "bot");

    // A stranger (no session) sees no rack and no seat.
    let stranger = app
        .router()
        .oneshot(get(&format!("{location}/state"), None))
        .await
        .unwrap();
    let stranger_view: Value = serde_json::from_str(&body_string(stranger).await).unwrap();
    assert!(stranger_view["your_seat"].is_null());
    assert!(stranger_view["your_rack"].is_null());

    // We pass; the hard bot then takes its turn automatically.
    let mv = app
        .router()
        .oneshot(post_json(
            &format!("{location}/move"),
            Some(&cookie),
            json!({ "kind": "pass" }).to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(mv.status(), StatusCode::OK);
    let after: Value = serde_json::from_str(&body_string(mv).await).unwrap();
    // It should be our turn again (seat 0) after the bot moved.
    assert_eq!(after["turn"], json!(0));
    assert!(!after["moves"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn game_page_escapes_script_in_embedded_state() {
    let app = test_app().await;
    let (_user, cookie) = app.new_session();

    // A player name containing "</script>" must not break out of the embedded
    // JSON <script> element on the game page.
    let create = app
        .router()
        .oneshot(post_form(
            "/games",
            Some(&cookie),
            "your_name=%3C%2Fscript%3E&seat2=easy",
        ))
        .await
        .unwrap();
    let location = location_of(&create);

    let page = app
        .router()
        .oneshot(get(&location, Some(&cookie)))
        .await
        .unwrap();
    let html = body_string(page).await;

    let state_block = html
        .split(r#"<script id="game-state" type="application/json">"#)
        .nth(1)
        .expect("game-state script present");
    let json_text = state_block
        .split("</script>")
        .next()
        .expect("game-state script closes");
    assert!(!json_text.contains("</script>"));
    assert!(json_text.contains("<\\/script>"));
}

#[tokio::test]
async fn move_without_auth_is_unauthorized() {
    let app = test_app().await;
    let (_user, cookie) = app.new_session();

    let create = app
        .router()
        .oneshot(post_form("/games", Some(&cookie), "seat2=easy"))
        .await
        .unwrap();
    let location = location_of(&create);

    // No session cookie => the auth extractor rejects with 401.
    let mv = app
        .router()
        .oneshot(post_json(
            &format!("{location}/move"),
            None,
            json!({ "kind": "pass" }).to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(mv.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn move_by_non_seated_user_is_forbidden() {
    let app = test_app().await;
    let (_host, host_cookie) = app.new_session();
    let (_other, other_cookie) = app.new_session();

    let create = app
        .router()
        .oneshot(post_form("/games", Some(&host_cookie), "seat2=easy"))
        .await
        .unwrap();
    let location = location_of(&create);

    // A different authenticated user who is not seated cannot move.
    let mv = app
        .router()
        .oneshot(post_json(
            &format!("{location}/move"),
            Some(&other_cookie),
            json!({ "kind": "pass" }).to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(mv.status(), StatusCode::FORBIDDEN);
    let err: Value = serde_json::from_str(&body_string(mv).await).unwrap();
    assert!(err.get("error").is_some());
}

#[tokio::test]
async fn join_requires_auth() {
    let app = test_app().await;
    let (_host, host_cookie) = app.new_session();
    let create = app
        .router()
        .oneshot(post_form(
            "/games",
            Some(&host_cookie),
            "your_name=Host&seat2=open",
        ))
        .await
        .unwrap();
    let location = location_of(&create);

    let join = app
        .router()
        .oneshot(post_form(&format!("{location}/join"), None, "name=Guest"))
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn join_sets_display_name_on_open_seat() {
    let app = test_app().await;
    let (_host, host_cookie) = app.new_session();
    let (_guest, guest_cookie) = app.new_session();

    // Host creates a game with one open human seat.
    let create = app
        .router()
        .oneshot(post_form(
            "/games",
            Some(&host_cookie),
            "your_name=Host&seat2=open",
        ))
        .await
        .unwrap();
    let location = location_of(&create);

    // A second visitor joins the open seat with a chosen name.
    let join = app
        .router()
        .oneshot(post_form(
            &format!("{location}/join"),
            Some(&guest_cookie),
            "name=Guest",
        ))
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::SEE_OTHER);

    // The guest is now seated at seat 1 with their chosen name.
    let state = app
        .router()
        .oneshot(get(&format!("{location}/state"), Some(&guest_cookie)))
        .await
        .unwrap();
    let view: Value = serde_json::from_str(&body_string(state).await).unwrap();
    assert_eq!(view["your_seat"], json!(1));
    assert_eq!(view["seats"][1]["name"], "Guest");
    assert_eq!(view["seats"][1]["open"], json!(false));
}
