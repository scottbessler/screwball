use axum::{
    Extension, Json,
    extract::{Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    app::AppState,
    bot,
    error::AppError,
    game::{self, MoveError, SeatSpec},
    models::{Board, Difficulty, Game, GameStatus, MoveKind, Placement, Position, SeatKind, Tile},
    render,
    session::CurrentUser,
    view::GameView,
};

pub async fn index(
    State(state): State<AppState>,
    Extension(CurrentUser(user)): Extension<CurrentUser>,
) -> Html<String> {
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
    Html(render::home_page(&mine, user))
}

pub async fn healthcheck() -> &'static str {
    "OK"
}

pub async fn demo() -> Html<String> {
    Html(render::demo_page(&Board::new()))
}

#[derive(Deserialize)]
pub struct CreateForm {
    your_name: Option<String>,
    seat2: Option<String>,
    seat3: Option<String>,
    seat4: Option<String>,
}

pub async fn create_game(
    State(state): State<AppState>,
    Extension(CurrentUser(user)): Extension<CurrentUser>,
    Form(form): Form<CreateForm>,
) -> Result<Redirect, AppError> {
    let your_name = form
        .your_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
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

    let game = game::new_game(specs, &mut rand::thread_rng());
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
        "medium" => Some(bot_spec(Difficulty::Medium, "Medium bot")),
        "hard" => Some(bot_spec(Difficulty::Hard, "Hard bot")),
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
    Extension(CurrentUser(user)): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let game = state
        .store
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("game not found"))?;
    let view = GameView::for_viewer(&game, user);
    let initial = serde_json::to_string(&view).map_err(AppError::internal)?;
    Ok(Html(render::game_page(&view, &initial)))
}

pub async fn join_game(
    State(state): State<AppState>,
    Extension(CurrentUser(user)): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
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
            }
        })
        .await?;
    Ok(Redirect::to(&format!("/games/{id}")))
}

pub async fn game_state(
    State(state): State<AppState>,
    Extension(CurrentUser(user)): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<GameView>, AppError> {
    let game = state
        .store
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("game not found"))?;
    Ok(Json(GameView::for_viewer(&game, user)))
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
    Extension(CurrentUser(user)): Extension<CurrentUser>,
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
        Ok(Ok(view)) => (StatusCode::OK, Json(view)).into_response(),
        Ok(Err(api)) => move_error(api.status, &api.message),
        Err(err) => err.into_response(),
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

fn move_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
