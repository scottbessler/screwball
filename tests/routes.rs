use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use axum_extra::extract::cookie::Key;
use cookie::{Cookie as RawCookie, CookieJar};
use rand::{SeedableRng, rngs::StdRng};
use screwball::{
    app,
    dict::Dictionary,
    game::{SeatSpec, new_game},
    models::{Difficulty, Game, GameStatus, SeatKind},
    push::PushService,
    store::GameStore,
    users::{User, UserStore},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_VAPID_PRIVATE_KEY: &str = "GOUXfxqzqhlIXF7mcuoriQnHt7rmodZJQvRK1vD16Bc";

/// A test harness bundling the router with the cookie-signing key, so tests can
/// mint authenticated `sid` cookies without running a real passkey ceremony.
struct TestApp {
    router: Router,
    key: Key,
    store: Arc<GameStore>,
    users: Arc<UserStore>,
}

async fn test_app() -> TestApp {
    test_app_with_push(PushService::disabled()).await
}

async fn test_app_with_push(push: PushService) -> TestApp {
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
        defs: Arc::new(screwball::define::DefinitionCache::new()),
        store: store.clone(),
        users: users.clone(),
        webauthn,
        key: key.clone(),
        push,
        passkey_disabled: false,
    };
    TestApp {
        router: app::router(state),
        key,
        store,
        users,
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

    /// Register a user with the given display name and return their id plus a
    /// signed session cookie. Seat names now come from the account, so tests
    /// that care about a player's name register them first.
    async fn register(&self, username: &str, display_name: &str) -> (Uuid, String) {
        let user = User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            display_name: display_name.to_string(),
            credentials: Vec::new(),
            push_subscriptions: Vec::new(),
            created_at: chrono::Utc::now(),
        };
        let id = user.id;
        self.users.insert(user).await.unwrap();
        (id, self.cookie_for(id))
    }

    /// Build the signed `sid=...` cookie header value for a given user.
    fn cookie_for(&self, user: Uuid) -> String {
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key)
            .add(RawCookie::new("sid", user.to_string()));
        let value = jar.get("sid").unwrap().value().to_string();
        format!("sid={value}")
    }

    async fn insert_game(&self, game: Game) {
        self.store.insert(game).await.unwrap();
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
async fn service_worker_serves_push_notification_script() {
    let app = test_app().await;
    let response = app.router().oneshot(get("/sw.js", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(content_type.starts_with("text/javascript"));
    let js = body_string(response).await;
    assert!(js.contains("self.addEventListener(\"push\""));
    assert!(js.contains("showNotification"));
}

#[tokio::test]
async fn push_public_key_requires_auth() {
    let app = test_app().await;
    let response = app
        .router()
        .oneshot(get("/api/push/vapid-public-key", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(content_type.starts_with("application/json"));
}

#[tokio::test]
async fn push_subscription_is_persisted_for_user() {
    let push =
        PushService::from_private_key(TEST_VAPID_PRIVATE_KEY, "mailto:test@example.com").unwrap();
    let app = test_app_with_push(push).await;
    let (user, cookie) = app.register("push-user", "Push User").await;

    let key = app
        .router()
        .oneshot(get("/api/push/vapid-public-key", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(key.status(), StatusCode::OK);
    let key_body: Value = serde_json::from_str(&body_string(key).await).unwrap();
    assert!(
        key_body["public_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty())
    );

    let body = json!({
        "endpoint": "https://push.example.test/subscription/1",
        "keys": {
            "p256dh": "public-key",
            "auth": "auth-secret"
        }
    })
    .to_string();
    let response = app
        .router()
        .oneshot(post_json("/api/push/subscribe", Some(&cookie), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let user = app.users.get(user).await.unwrap();
    assert_eq!(user.push_subscriptions.len(), 1);
    assert_eq!(
        user.push_subscriptions[0].endpoint,
        "https://push.example.test/subscription/1"
    );
}

#[tokio::test]
async fn public_assets_are_not_immutable_in_debug_builds() {
    let app = test_app().await;
    let response = app
        .router()
        .oneshot(get("/public/game.js?v=stale-version", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(cache_control, "no-cache");
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
    assert!(html.contains("class=\"form new-game-form\""));
    assert!(html.contains("class=\"form-option-row\""));
    assert!(html.contains("role=\"tooltip\""));
    assert!(html.contains("name=\"jax_mode\""));
    assert!(!html.contains("href=\"/demo\""));
    assert!(!html.contains("Demo board"));
}

#[tokio::test]
async fn home_page_badges_only_games_waiting_on_you_and_separates_finished() {
    let app = test_app().await;
    let (user, cookie) = app.new_session();
    let mut rng = StdRng::seed_from_u64(7);

    let mut your_turn = new_game(
        vec![
            SeatSpec {
                kind: SeatKind::Human {
                    user_id: Some(user),
                },
                name: "Scott".to_string(),
            },
            SeatSpec {
                kind: SeatKind::Bot {
                    difficulty: Difficulty::Medium,
                },
                name: "Medium bot".to_string(),
            },
        ],
        false,
        false,
        false,
        0,
        &mut rng,
    );
    your_turn.turn = 0;

    let mut waiting_on_bot = new_game(
        vec![
            SeatSpec {
                kind: SeatKind::Human {
                    user_id: Some(user),
                },
                name: "Scott".to_string(),
            },
            SeatSpec {
                kind: SeatKind::Bot {
                    difficulty: Difficulty::Hard,
                },
                name: "Hard bot".to_string(),
            },
        ],
        false,
        false,
        false,
        0,
        &mut rng,
    );
    waiting_on_bot.turn = 1;

    let mut finished = new_game(
        vec![
            SeatSpec {
                kind: SeatKind::Human {
                    user_id: Some(user),
                },
                name: "Scott".to_string(),
            },
            SeatSpec {
                kind: SeatKind::Bot {
                    difficulty: Difficulty::Chill,
                },
                name: "Chill bot".to_string(),
            },
        ],
        false,
        false,
        false,
        0,
        &mut rng,
    );
    finished.status = GameStatus::Finished;

    app.insert_game(your_turn).await;
    app.insert_game(waiting_on_bot).await;
    app.insert_game(finished).await;

    let response = app.router().oneshot(get("/", Some(&cookie))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;

    assert_eq!(html.matches("badge badge-turn\">your turn").count(), 1);
    assert!(!html.contains(">you</span>"));
    assert!(html.contains("class=\"game-list-divider\"><span>Finished games</span>"));
    assert!(html.contains("class=\"game-list-item is-finished\""));

    let divider = html.find("Finished games").unwrap();
    let finished_game = html.find("Scott vs Chill bot").unwrap();
    assert!(divider < finished_game);
}

#[tokio::test]
async fn demo_route_is_removed() {
    let app = test_app().await;
    let response = app.router().oneshot(get("/demo", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_game_requires_auth() {
    let app = test_app().await;
    let response = app
        .router()
        .oneshot(post_form("/games", None, "seat2=hard"))
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
        .oneshot(post_form("/games", Some(&cookie), "seat2=hard"))
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
async fn jax_mode_hints_are_unlimited_even_when_hint_setting_is_none() {
    let app = test_app().await;
    let (_user, cookie) = app.new_session();

    let create = app
        .router()
        .oneshot(post_form(
            "/games",
            Some(&cookie),
            "seat2=open&jax_mode=on&hints=0",
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::SEE_OTHER);
    let location = location_of(&create);
    let id: Uuid = location.trim_start_matches("/games/").parse().unwrap();

    let state = app
        .router()
        .oneshot(get(&format!("{location}/state"), Some(&cookie)))
        .await
        .unwrap();
    let view: Value = serde_json::from_str(&body_string(state).await).unwrap();
    assert_eq!(view["jax_mode"], json!(true));
    assert_eq!(view["hints_allowed"], json!(0));
    assert_eq!(view["hints_unlimited"], json!(true));
    assert_eq!(view["seats"][0]["hints_unlimited"], json!(true));

    let hint = app
        .router()
        .oneshot(post_form(&format!("{location}/hint"), Some(&cookie), ""))
        .await
        .unwrap();
    assert_eq!(hint.status(), StatusCode::OK);
    let body: Value = serde_json::from_str(&body_string(hint).await).unwrap();
    assert_eq!(body["unlimited"], json!(true));
    assert!(body["remaining"].is_null());

    let game = app.store.get(id).await.unwrap();
    assert_eq!(game.hints_used[0], 0);
}

#[tokio::test]
async fn game_page_escapes_script_in_embedded_state() {
    let app = test_app().await;
    // The seat name comes from the account display name, which here contains
    // "</script>" and must not break out of the embedded JSON <script> element.
    let (_user, cookie) = app.register("scripter", "</script>").await;

    let create = app
        .router()
        .oneshot(post_form("/games", Some(&cookie), "seat2=easy"))
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
    assert!(html.contains(r#"<script id="grandpa-two-letter-words" type="application/json">"#));
    assert!(html.contains(r#""AM""#));
}

#[tokio::test]
async fn game_page_does_not_render_literal_div_text_lines() {
    let app = test_app().await;
    let (_user, cookie) = app.register("player", "Player").await;

    let create = app
        .router()
        .oneshot(post_form("/games", Some(&cookie), "seat2=medium"))
        .await
        .unwrap();
    let location = location_of(&create);

    let page = app
        .router()
        .oneshot(get(&location, Some(&cookie)))
        .await
        .unwrap();
    let html = body_string(page).await;
    assert!(
        !html.lines().any(|line| line.trim() == "div"),
        "game page should not emit standalone literal `div` text nodes"
    );
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
    // The move endpoint is consumed by fetch(), so the rejection must be JSON
    // (with an `error` field) rather than an HTML error page.
    let content_type = mv
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "expected JSON error, got content-type {content_type:?}"
    );
    let body: Value = serde_json::from_str(&body_string(mv).await).unwrap();
    assert!(body.get("error").and_then(Value::as_str).is_some());
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
        .oneshot(post_form("/games", Some(&host_cookie), "seat2=open"))
        .await
        .unwrap();
    let location = location_of(&create);

    let join = app
        .router()
        .oneshot(post_form(&format!("{location}/join"), None, ""))
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn join_sets_display_name_on_open_seat() {
    let app = test_app().await;
    let (_host, host_cookie) = app.register("host", "Host").await;
    let (_guest, guest_cookie) = app.register("guest", "Guest").await;

    // Host creates a game with one open human seat.
    let create = app
        .router()
        .oneshot(post_form("/games", Some(&host_cookie), "seat2=open"))
        .await
        .unwrap();
    let location = location_of(&create);

    // A second visitor joins the open seat; their account display name fills it.
    let join = app
        .router()
        .oneshot(post_form(
            &format!("{location}/join"),
            Some(&guest_cookie),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(join.status(), StatusCode::SEE_OTHER);

    // The guest is now seated at seat 1 with their account display name.
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
