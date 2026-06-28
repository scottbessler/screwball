use axum::{
    Json,
    extract::{Form, Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    app::AppState,
    bot,
    error::AppError,
    game::{self, MoveError, SeatSpec},
    models::{Board, Difficulty, Game, GameStatus, MoveKind, Placement, Position, SeatKind, Tile},
    render,
    session::{ApiAuthUser, AuthUser, MaybeUser},
    users::{PushSubscription, PushSubscriptionKeys},
    view::{GameSummary, GameView},
};

const SERVICE_WORKER_JS: &str = include_str!("../public/sw.js");

/// The viewer id used for redaction; logged-out visitors get the nil UUID,
/// which never matches a real (v4) seat owner.
fn viewer_id(user: Option<Uuid>) -> Uuid {
    user.unwrap_or_else(Uuid::nil)
}

pub async fn index(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Html<String> {
    let Some(user) = user else {
        return Html(render::home_page(&[], None, None));
    };
    let games = state.store.list().await;
    let mine: Vec<Game> = games
        .into_iter()
        .filter(|game| {
            game.seats.iter().any(|seat| match seat.kind {
                SeatKind::Human { user_id } => user_id == Some(user),
                SeatKind::Bot { .. } => false,
            })
        })
        .collect();
    let display_name = state.users.get(user).await.map(|u| u.display_name);
    Html(render::home_page(
        &mine,
        Some(user),
        display_name.as_deref(),
    ))
}

pub async fn healthcheck() -> &'static str {
    "OK"
}

pub async fn service_worker() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        SERVICE_WORKER_JS,
    )
        .into_response()
}

pub async fn demo() -> Html<String> {
    Html(render::demo_page(&Board::new()))
}

#[derive(Deserialize)]
pub struct CreateForm {
    seat2: Option<String>,
    seat3: Option<String>,
    seat4: Option<String>,
    #[serde(default)]
    john_mode: Option<String>,
    #[serde(default)]
    grandpa_mode: Option<String>,
    #[serde(default)]
    hints: Option<u8>,
}

pub async fn create_game(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<CreateForm>,
) -> Result<Redirect, AppError> {
    let your_name = state
        .users
        .get(user)
        .await
        .map(|u| u.display_name)
        .unwrap_or_else(|| "You".to_string());

    let mut specs = vec![SeatSpec {
        kind: SeatKind::Human {
            user_id: Some(user),
        },
        name: your_name,
    }];
    for raw in [form.seat2, form.seat3, form.seat4].into_iter().flatten() {
        if let Some(spec) = seat_spec(&raw) {
            specs.push(spec);
        }
    }
    if specs.len() < 2 {
        return Err(AppError::bad_request("a game needs at least two seats"));
    }
    if specs.len() > 4 {
        return Err(AppError::bad_request("a game supports at most four seats"));
    }

    let john_mode = form.john_mode.is_some();
    let grandpa_mode = form.grandpa_mode.is_some();
    let hints_allowed = form.hints.unwrap_or(0).min(3);
    let game = game::new_game(
        specs,
        john_mode,
        grandpa_mode,
        hints_allowed,
        &mut rand::thread_rng(),
    );
    let id = game.id;
    state.store.insert(game).await?;

    // If the opening seat is a bot, let it (and any following bots) move.
    let dict = state.dict.clone();
    state.store.update(id, |game| run_bots(game, &dict)).await?;

    Ok(Redirect::to(&format!("/games/{id}")))
}

fn seat_spec(raw: &str) -> Option<SeatSpec> {
    match raw {
        "open" => Some(SeatSpec {
            kind: SeatKind::Human { user_id: None },
            name: "Open seat".to_string(),
        }),
        "easy" => Some(bot_spec(Difficulty::Easy, "Easy bot")),
        "chill" => Some(bot_spec(Difficulty::Chill, "Chill bot")),
        "medium" => Some(bot_spec(Difficulty::Medium, "Medium bot")),
        "hard" => Some(bot_spec(Difficulty::Hard, "Hard bot")),
        "impossible" => Some(bot_spec(Difficulty::Impossible, "Impossible bot")),
        _ => None,
    }
}

fn bot_spec(difficulty: Difficulty, name: &str) -> SeatSpec {
    SeatSpec {
        kind: SeatKind::Bot { difficulty },
        name: name.to_string(),
    }
}

pub async fn game_page(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let game = state
        .store
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("game not found"))?;
    let view = GameView::for_viewer(&game, viewer_id(user));
    let initial = serde_json::to_string(&view).map_err(AppError::internal)?;
    let two_letter =
        serde_json::to_string(&state.dict.two_letter_words()).map_err(AppError::internal)?;
    let other_games = match user {
        Some(user) => my_game_summaries(&state, user, Some(id)).await,
        None => Vec::new(),
    };
    Ok(Html(render::game_page(
        &view,
        &initial,
        &two_letter,
        &other_games,
        user.is_some(),
    )))
}

/// The viewer's games sorted active-first then most-recently-updated-first,
/// optionally excluding one game id, each flagged with whether it is the
/// viewer's turn.
async fn my_game_summaries(
    state: &AppState,
    user: Uuid,
    exclude: Option<Uuid>,
) -> Vec<GameSummary> {
    let mut summaries: Vec<GameSummary> = state
        .store
        .list()
        .await
        .iter()
        .filter(|game| Some(game.id) != exclude)
        .filter_map(|game| GameSummary::for_viewer(game, user))
        .collect();
    summaries.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then(b.effective_updated_at.cmp(&a.effective_updated_at))
    });
    summaries
}

/// JSON list of the signed-in viewer's games, used by the game page to keep the
/// "your other games" panel (and its your-turn flags) fresh while you play.
pub async fn my_games(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> Json<Vec<GameSummary>> {
    let summaries = match user {
        Some(user) => my_game_summaries(&state, user, None).await,
        None => Vec::new(),
    };
    Json(summaries)
}

#[derive(Serialize)]
pub struct PushPublicKey {
    public_key: Option<String>,
}

pub async fn push_public_key(
    State(state): State<AppState>,
    ApiAuthUser(_user): ApiAuthUser,
) -> Json<PushPublicKey> {
    Json(PushPublicKey {
        public_key: state.push.public_key().map(str::to_string),
    })
}

#[derive(Deserialize)]
pub struct PushSubscriptionRequest {
    endpoint: String,
    keys: PushSubscriptionKeys,
}

pub async fn push_subscribe(
    State(state): State<AppState>,
    ApiAuthUser(user): ApiAuthUser,
    Json(subscription): Json<PushSubscriptionRequest>,
) -> Response {
    if state.push.public_key().is_none() {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "web push is not configured",
        );
    }
    if subscription.endpoint.trim().is_empty()
        || subscription.keys.p256dh.trim().is_empty()
        || subscription.keys.auth.trim().is_empty()
    {
        return api_error(StatusCode::BAD_REQUEST, "invalid push subscription");
    }

    let result = state
        .users
        .update(user, |stored| {
            let subscription = PushSubscription {
                endpoint: subscription.endpoint,
                keys: subscription.keys,
            };
            if let Some(existing) = stored
                .push_subscriptions
                .iter_mut()
                .find(|existing| existing.endpoint == subscription.endpoint)
            {
                *existing = subscription;
            } else {
                stored.push_subscriptions.push(subscription);
            }
            if stored.push_subscriptions.len() > 10 {
                let remove_count = stored.push_subscriptions.len() - 10;
                stored.push_subscriptions.drain(0..remove_count);
            }
        })
        .await;

    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(err.status_code(), err.detail()),
    }
}

#[derive(Deserialize)]
pub struct PushUnsubscribeRequest {
    endpoint: String,
}

pub async fn push_unsubscribe(
    State(state): State<AppState>,
    ApiAuthUser(user): ApiAuthUser,
    Json(body): Json<PushUnsubscribeRequest>,
) -> Response {
    let result = state
        .users
        .update(user, |stored| {
            stored
                .push_subscriptions
                .retain(|subscription| subscription.endpoint != body.endpoint);
        })
        .await;

    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(err.status_code(), err.detail()),
    }
}

pub async fn join_game(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let name = state.users.get(user).await.map(|u| u.display_name);
    state
        .store
        .update(id, |game| {
            let already = game.seats.iter().any(|seat| match seat.kind {
                SeatKind::Human { user_id } => user_id == Some(user),
                SeatKind::Bot { .. } => false,
            });
            if !already
                && let Some(seat) = game
                    .seats
                    .iter_mut()
                    .find(|seat| matches!(seat.kind, SeatKind::Human { user_id: None }))
            {
                seat.kind = SeatKind::Human {
                    user_id: Some(user),
                };
                if let Some(name) = &name {
                    seat.name = name.clone();
                }
            }
        })
        .await?;
    Ok(Redirect::to(&format!("/games/{id}")))
}

pub async fn game_state(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<Uuid>,
) -> Result<Json<GameView>, AppError> {
    let game = state
        .store
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("game not found"))?;
    Ok(Json(GameView::for_viewer(&game, viewer_id(user))))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MoveRequest {
    Play { placements: Vec<PlacementReq> },
    Exchange { tiles: Vec<String> },
    Pass,
}

#[derive(Deserialize)]
pub struct PlacementReq {
    row: usize,
    col: usize,
    letter: String,
    #[serde(default)]
    is_blank: bool,
}

pub async fn submit_move(
    State(state): State<AppState>,
    ApiAuthUser(user): ApiAuthUser,
    Path(id): Path<Uuid>,
    Json(request): Json<MoveRequest>,
) -> Response {
    let kind = match to_move_kind(request) {
        Ok(kind) => kind,
        Err(message) => return move_error(StatusCode::BAD_REQUEST, &message),
    };

    let dict = state.dict.clone();
    let result = state
        .store
        .update(id, move |game| apply_player_move(game, &dict, user, kind))
        .await;

    match result {
        Ok(Ok(view)) => {
            if let Some(game) = state.store.get(id).await {
                let push = state.push.clone();
                let users = state.users.clone();
                // Warm the definition cache for every played word so the first
                // client lookup is a hit. Cached words are skipped, so scanning
                // the whole (short) move history each turn is cheap.
                let defs = state.defs.clone();
                let words: Vec<String> = game
                    .moves
                    .iter()
                    .flat_map(|mv| mv.words.iter().cloned())
                    .collect();
                tokio::spawn(async move {
                    push.notify_turn(&users, &game).await;
                });
                tokio::spawn(async move {
                    defs.warm(&words).await;
                });
            }
            (StatusCode::OK, Json(view)).into_response()
        }
        Ok(Err(api)) => move_error(api.status, &api.message),
        Err(err) => err.into_response(),
    }
}

/// Look up a word's definition via the server-side cache. Public (definitions
/// aren't game-private); restricted to short ASCII-alphabetic words so it can't
/// be used as an open URL proxy. 404 means "no definition found".
pub async fn define(State(state): State<AppState>, Path(word): Path<String>) -> Response {
    if word.len() < 2 || word.len() > 30 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return (StatusCode::BAD_REQUEST, "invalid word").into_response();
    }
    match state.defs.lookup(&word).await {
        Some(def) => (StatusCode::OK, Json(def)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

struct ApiMoveError {
    status: StatusCode,
    message: String,
}

fn apply_player_move(
    game: &mut Game,
    dict: &crate::dict::Dictionary,
    user: Uuid,
    kind: MoveKind,
) -> Result<GameView, ApiMoveError> {
    if game.status != GameStatus::Active {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "the game is not active".to_string(),
        });
    }
    let seat_index = game
        .seats
        .iter()
        .position(|seat| match seat.kind {
            SeatKind::Human { user_id } => user_id == Some(user),
            SeatKind::Bot { .. } => false,
        })
        .ok_or_else(|| ApiMoveError {
            status: StatusCode::FORBIDDEN,
            message: "you are not seated in this game".to_string(),
        })?;
    if seat_index != game.turn {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: MoveError::NotYourTurn.to_string(),
        });
    }

    let mut rng = rand::thread_rng();
    game::apply_move(game, dict, seat_index, kind, &mut rng).map_err(|err| ApiMoveError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        message: err.to_string(),
    })?;

    run_bots(game, dict);
    Ok(GameView::for_viewer(game, user))
}

/// Drive any bot seats whose turn it is until a human is on turn or the game ends.
fn run_bots(game: &mut Game, dict: &crate::dict::Dictionary) {
    let mut rng = rand::thread_rng();
    while game.status == GameStatus::Active && game.seats[game.turn].is_bot() {
        match bot::take_turn(game, dict, &mut rng) {
            // A successful bot move advances the turn; keep going for the next seat.
            Some(Ok(_)) => {}
            // No move available, or the move failed without advancing the turn.
            // Stop rather than retry the same failing move forever.
            Some(Err(err)) => {
                tracing::warn!(error = %err, seat = game.turn, "bot move failed; stopping bot run");
                break;
            }
            None => break,
        }
    }
}

fn to_move_kind(request: MoveRequest) -> Result<MoveKind, String> {
    match request {
        MoveRequest::Pass => Ok(MoveKind::Pass),
        MoveRequest::Exchange { tiles } => {
            let tiles = tiles
                .iter()
                .map(|t| parse_tile(t))
                .collect::<Result<_, _>>()?;
            Ok(MoveKind::Exchange { tiles })
        }
        MoveRequest::Play { placements } => {
            let placements = placements
                .into_iter()
                .map(parse_placement)
                .collect::<Result<_, _>>()?;
            Ok(MoveKind::Play { placements })
        }
    }
}

fn parse_placement(req: PlacementReq) -> Result<Placement, String> {
    let letter = parse_letter(&req.letter)?;
    let position = Position::new(req.row, req.col);
    if !position.in_bounds() {
        return Err("placement is off the board".to_string());
    }
    Ok(Placement {
        position,
        letter,
        is_blank: req.is_blank,
    })
}

fn parse_letter(raw: &str) -> Result<char, String> {
    let mut chars = raw.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) if ch.is_ascii_alphabetic() => Ok(ch.to_ascii_uppercase()),
        _ => Err(format!("invalid letter: {raw:?}")),
    }
}

fn parse_tile(raw: &str) -> Result<Tile, String> {
    match raw {
        "?" | "_" | "" => Ok(Tile::Blank),
        other => Ok(Tile::Letter(parse_letter(other)?)),
    }
}

pub async fn hint(
    State(state): State<AppState>,
    ApiAuthUser(user): ApiAuthUser,
    Path(id): Path<Uuid>,
) -> Response {
    let dict = state.dict.clone();
    let result = state
        .store
        .update(id, move |game| apply_hint(game, &dict, user))
        .await;

    match result {
        Ok(Ok(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(Err(api)) => move_error(api.status, &api.message),
        Err(err) => err.into_response(),
    }
}

fn apply_hint(
    game: &mut Game,
    dict: &crate::dict::Dictionary,
    user: Uuid,
) -> Result<serde_json::Value, ApiMoveError> {
    if game.status != GameStatus::Active {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "the game is not active".into(),
        });
    }

    let seat_index = game
        .seats
        .iter()
        .position(|seat| matches!(seat.kind, SeatKind::Human { user_id } if user_id == Some(user)))
        .ok_or_else(|| ApiMoveError {
            status: StatusCode::FORBIDDEN,
            message: "you are not seated in this game".into(),
        })?;

    if seat_index != game.turn {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "it is not your turn".into(),
        });
    }

    if game.hints_allowed == 0 {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "hints are not enabled for this game".into(),
        });
    }

    let used = game.hints_used.get(seat_index).copied().unwrap_or(0);
    if used >= game.hints_allowed {
        return Err(ApiMoveError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "no hints remaining".into(),
        });
    }

    let plays = bot::scored_plays(
        &game.board,
        &game.seats[seat_index].rack,
        dict,
        game.word_rule(),
    );
    let best = plays.iter().max_by_key(|p| p.1.points);

    match best {
        Some((_, scored)) => {
            if game.hints_used.len() <= seat_index {
                game.hints_used.resize(game.seats.len(), 0);
            }
            game.hints_used[seat_index] += 1;
            let remaining = game.hints_allowed - game.hints_used[seat_index];
            let words: Vec<&str> = scored.words.iter().map(|w| w.word.as_str()).collect();
            Ok(json!({
                "words": words,
                "score": scored.points,
                "remaining": remaining
            }))
        }
        None => Ok(json!({
            "words": [],
            "score": 0,
            "remaining": game.hints_allowed - used,
            "message": "no plays available"
        })),
    }
}

fn move_error(status: StatusCode, message: &str) -> Response {
    api_error(status, message)
}

fn api_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
