use std::sync::OnceLock;

use uuid::Uuid;

use crate::models::{BOARD_SIZE, Board, Game, GameStatus, SeatKind};
use crate::view::{GameSummary, GameView, PositionView, SquareView, premium_code};

/// Content-hash of the static assets, set once at startup for release builds.
/// Debug builds hash each asset at render time so local edits do not keep
/// pointing browsers at stale immutable URLs.
static ASSET_VERSION: OnceLock<String> = OnceLock::new();

pub fn set_asset_version(version: String) {
    let _ = ASSET_VERSION.set(version);
}

fn asset_version() -> &'static str {
    ASSET_VERSION.get().map(String::as_str).unwrap_or("dev")
}

/// A `/public/...` URL with the current asset version appended for cache-busting.
fn asset(path: &str) -> String {
    format!("{path}?v={}", asset_version_for(path))
}

#[cfg(debug_assertions)]
fn asset_version_for(path: &str) -> String {
    use std::hash::{Hash, Hasher};

    let file = path.trim_start_matches('/');
    if let Ok(bytes) = std::fs::read(file) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        return format!("{:x}", hasher.finish());
    }

    asset_version().to_string()
}

#[cfg(not(debug_assertions))]
fn asset_version_for(_path: &str) -> String {
    asset_version().to_string()
}

pub fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn layout(title: &str, body: &str) -> String {
    layout_with_head(title, body, "", "")
}

fn layout_with_head(title: &str, body: &str, head_extra: &str, body_class: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no,viewport-fit=cover">
  <meta name="apple-mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-status-bar-style" content="default">
  <title>{title}</title>
  <link rel="icon" href="/public/favicon.svg">
  <link rel="apple-touch-icon" href="/public/apple-touch-icon.png">
  <link rel="manifest" href="/public/manifest.webmanifest">
  <link rel="stylesheet" href="{css}">
  {head_extra}
</head>
<body class="{body_class}">
  {nav}
  <main class="page">
  {body}
  </main>
</body>
</html>"#,
        title = escape(title),
        css = asset("/public/app.css"),
        head_extra = head_extra,
        body_class = body_class,
        nav = nav(),
        body = body,
    )
}

pub fn nav() -> &'static str {
    r#"<nav class="nav">
  <a class="brand" href="/" aria-label="Screwball home">
    <img class="brand-icon" src="/public/favicon.svg" alt="">
  </a>
  <div class="nav-links"></div>
</nav>"#
}

pub fn error_page(title: &str, message: &str) -> String {
    layout(
        title,
        &format!(
            r#"<section class="card">
  <h1>{}</h1>
  <p>{}</p>
  <p><a href="/">Back home</a></p>
</section>"#,
            escape(title),
            escape(message)
        ),
    )
}

pub fn home_page(
    games: &[Game],
    joinable_games: &[Game],
    current: Option<Uuid>,
    display_name: Option<&str>,
) -> String {
    let Some(current) = current else {
        return layout_with_head(
            "Screwball",
            &signed_out_panel(),
            &format!(
                r#"<script type="module" src="{}" defer></script>"#,
                asset("/public/auth.js")
            ),
            "",
        );
    };
    let new_game = new_game_form();
    let list = if games.is_empty() {
        "<p class=\"muted\">No games yet. Create one to get started.</p>".to_string()
    } else {
        game_list(games, current)
    };
    let open_games = if joinable_games.is_empty() {
        String::new()
    } else {
        format!(
            r#"<section class="card">
  <h1>Open games</h1>
  <p class="muted">Games with open seats waiting for another player.</p>
  {}
</section>"#,
            open_game_list(joinable_games),
        )
    };
    let greeting = display_name
        .map(|name| format!("Signed in as <strong>{}</strong>", escape(name)))
        .unwrap_or_else(|| "Signed in".to_string());
    layout(
        "Screwball",
        &format!(
            r#"<section class="card account">
  <p class="muted">{greeting}</p>
  <form method="post" action="/auth/logout">
    <button type="submit" class="button button-secondary">Sign out</button>
  </form>
</section>
<section class="card">
  <h1>New game</h1>
  {new_game}
</section>
<section class="card">
  <h1>Your games</h1>
  {list}
  <p class="debug-link"><a href="/debug/notifications">Notification debug</a> | <a href="/debug/touch">Touch debug</a></p>
</section>
{open_games}"#
        ),
    )
}

pub fn notification_debug_page() -> String {
    layout_with_head(
        "Notification debug",
        r#"<section class="card notification-debug">
  <h1>Notification debug</h1>
  <p class="muted">Use this to separate browser permission, service worker, subscription storage, and server push delivery.</p>

  <div class="debug-grid">
    <section class="debug-panel">
      <h2>Browser</h2>
      <dl class="debug-status">
        <dt>Support</dt><dd id="debug-support">checking...</dd>
        <dt>Permission</dt><dd id="debug-permission">checking...</dd>
        <dt>Service worker</dt><dd id="debug-service-worker">checking...</dd>
        <dt>Push subscription</dt><dd id="debug-subscription">checking...</dd>
      </dl>
    </section>
    <section class="debug-panel">
      <h2>Server</h2>
      <dl class="debug-status">
        <dt>Web push</dt><dd id="debug-server-configured">checking...</dd>
        <dt>Stored subscriptions</dt><dd id="debug-server-subscriptions">checking...</dd>
        <dt>VAPID public key</dt><dd id="debug-server-key">checking...</dd>
      </dl>
    </section>
  </div>

  <div class="debug-actions">
    <button type="button" class="button" id="debug-enable">Enable/register</button>
    <button type="button" class="button button-secondary" id="debug-local-test">Local notification</button>
    <button type="button" class="button button-secondary" id="debug-server-test">Send server test</button>
    <button type="button" class="button ghost" id="debug-unsubscribe">Unsubscribe</button>
  </div>

  <pre class="debug-output" id="debug-output" aria-live="polite"></pre>
</section>"#,
        &format!(
            r#"<script type="module" src="{}" defer></script>"#,
            asset("/public/notification-debug.js")
        ),
        "",
    )
}

pub fn touch_debug_page() -> String {
    layout_with_head(
        "Touch debug",
        r#"<section class="touch-debug" aria-labelledby="touch-debug-title">
  <div class="touch-debug-board-panel">
    <h1 id="touch-debug-title">Touch debug</h1>
    <p class="touch-debug-turn">Your turn</p>
    <div id="touch-debug-board" class="board touch-debug-board" role="grid" aria-label="Debug board"></div>
    <div id="touch-debug-rack" class="rack touch-debug-rack" aria-label="Debug rack"></div>
  </div>

  <form id="touch-debug-controls" class="touch-debug-controls">
    <h2>Offsets</h2>
    <fieldset>
      <legend>Drop point</legend>
      <div class="touch-debug-offset" data-offset-key="dropX">
        <label for="touch-debug-drop-x">X <output id="touch-debug-drop-x-output" for="touch-debug-drop-x">0 px</output></label>
        <div class="touch-debug-slider-row">
          <button type="button" class="button button-secondary" data-nudge="-10" aria-label="Decrease drop X">-10</button>
          <input id="touch-debug-drop-x" type="range" min="-140" max="140" step="2" value="0">
          <button type="button" class="button button-secondary" data-nudge="10" aria-label="Increase drop X">+10</button>
        </div>
      </div>
      <div class="touch-debug-offset" data-offset-key="dropY">
        <label for="touch-debug-drop-y">Y <output id="touch-debug-drop-y-output" for="touch-debug-drop-y">0 px</output></label>
        <div class="touch-debug-slider-row">
          <button type="button" class="button button-secondary" data-nudge="-10" aria-label="Decrease drop Y">-10</button>
          <input id="touch-debug-drop-y" type="range" min="-140" max="140" step="2" value="0">
          <button type="button" class="button button-secondary" data-nudge="10" aria-label="Increase drop Y">+10</button>
        </div>
      </div>
    </fieldset>

    <fieldset>
      <legend>Dragged tile</legend>
      <div class="touch-debug-offset" data-offset-key="tileX">
        <label for="touch-debug-tile-x">X <output id="touch-debug-tile-x-output" for="touch-debug-tile-x">0 px</output></label>
        <div class="touch-debug-slider-row">
          <button type="button" class="button button-secondary" data-nudge="-10" aria-label="Decrease dragged tile X">-10</button>
          <input id="touch-debug-tile-x" type="range" min="-140" max="140" step="2" value="0">
          <button type="button" class="button button-secondary" data-nudge="10" aria-label="Increase dragged tile X">+10</button>
        </div>
      </div>
      <div class="touch-debug-offset" data-offset-key="tileY">
        <label for="touch-debug-tile-y">Y <output id="touch-debug-tile-y-output" for="touch-debug-tile-y">0 px</output></label>
        <div class="touch-debug-slider-row">
          <button type="button" class="button button-secondary" data-nudge="-10" aria-label="Decrease dragged tile Y">-10</button>
          <input id="touch-debug-tile-y" type="range" min="-140" max="140" step="2" value="0">
          <button type="button" class="button button-secondary" data-nudge="10" aria-label="Increase dragged tile Y">+10</button>
        </div>
      </div>
    </fieldset>

    <div class="touch-debug-presets">
      <button type="button" class="button" id="touch-debug-preset-proposed">Proposed</button>
      <button type="button" class="button button-secondary" id="touch-debug-preset-old">Old target</button>
      <button type="button" class="button ghost" id="touch-debug-reset-board">Reset tiles</button>
    </div>

    <dl class="touch-debug-readout">
      <dt>Touch</dt><dd id="touch-debug-touch-readout">-</dd>
      <dt>Dragged</dt><dd id="touch-debug-drag-readout">-</dd>
      <dt>Drop</dt><dd id="touch-debug-drop-readout">-</dd>
      <dt>Cell</dt><dd id="touch-debug-cell-readout">-</dd>
    </dl>
  </form>
</section>"#,
        &format!(
            r#"<script type="module" src="{}" defer></script>"#,
            asset("/public/touch-debug.js")
        ),
        "touch-debug-page",
    )
}

/// The signed-out landing panel: passkey register + sign-in forms, wired up by
/// `public/auth.js`.
fn signed_out_panel() -> String {
    let body = r#"<section class="card">
  <h1>Welcome to Screwball</h1>
  <p class="muted">A classic Scrabble-style word game. Sign in with a passkey to play — no passwords.</p>
  <p id="auth-error" class="auth-error" role="alert" hidden></p>
  <div class="auth-grid">
    <form id="login-form" class="form auth-form">
      <h2>Sign in</h2>
      <label>Username
        <input type="text" name="username" autocomplete="username webauthn" maxlength="32" required>
      </label>
      <button type="submit" class="button">Sign in with passkey</button>
    </form>
    <form id="register-form" class="form auth-form">
      <h2>Create account</h2>
      <label>Username
        <input type="text" name="username" autocomplete="username" maxlength="32" required>
      </label>
      <label>Display name <span class="muted">(optional)</span>
        <input type="text" name="display_name" maxlength="48">
      </label>
      <button type="submit" class="button">Register a passkey</button>
    </form>
  </div>
</section>"#;
    body.to_string()
}

fn new_game_form() -> String {
    let options = |selected_off: bool| {
        let off = if selected_off { " selected" } else { "" };
        format!(
            r#"<option value="off"{off}>— none —</option>
<option value="open">Open seat (human)</option>
<option value="easy">Easy bot</option>
<option value="chill">Chill bot</option>
<option value="medium">Medium bot</option>
<option value="hard">Hard bot</option>
<option value="impossible">Impossible bot</option>"#
        )
    };
    format!(
        r#"<form method="post" action="/games" class="form new-game-form">
  <label>Seat 2
    <select name="seat2">{seat2}</select>
  </label>
  <label>Seat 3
    <select name="seat3">{seat3}</select>
  </label>
  <label>Seat 4
    <select name="seat4">{seat4}</select>
  </label>
  <div class="form-options">
    <div class="form-option-row">
      <label class="checkbox-label" for="john-mode">
        <input id="john-mode" type="checkbox" name="john_mode" value="on" />
        <span>John Mode</span>
      </label>
      <span class="info-tooltip" tabindex="0" aria-describedby="john-mode-help">i
        <span id="john-mode-help" class="tooltip-content" role="tooltip">Show valid 2-letter words.</span>
      </span>
    </div>
    <div class="form-option-row">
      <label class="checkbox-label" for="grandpa-mode">
        <input id="grandpa-mode" type="checkbox" name="grandpa_mode" value="on" />
        <span>Grandpa Mode</span>
      </label>
      <span class="info-tooltip" tabindex="0" aria-describedby="grandpa-mode-help">i
        <span id="grandpa-mode-help" class="tooltip-content" role="tooltip">Disallow obscure 2-letter words; keep common ones like at, in, go, to.</span>
      </span>
    </div>
    <div class="form-option-row">
      <label class="checkbox-label" for="jax-mode">
        <input id="jax-mode" type="checkbox" name="jax_mode" value="on" />
        <span>Jax Mode</span>
      </label>
      <span class="info-tooltip" tabindex="0" aria-describedby="jax-mode-help">i
        <span id="jax-mode-help" class="tooltip-content" role="tooltip">Unlimited hints and common names are valid words.</span>
      </span>
    </div>
    <div class="form-option-row">
      <label class="checkbox-label" for="august-mode">
        <input id="august-mode" type="checkbox" name="august_mode" value="on" />
        <span>August Mode</span>
      </label>
      <span class="info-tooltip" tabindex="0" aria-describedby="august-mode-help">i
        <span id="august-mode-help" class="tooltip-content" role="tooltip">Every tile is a letter from AUGUST (no blanks).</span>
      </span>
    </div>
    <label class="hints-label">Hints per player
      <select name="hints">
        <option value="0" selected>None</option>
        <option value="1">1</option>
        <option value="2">2</option>
        <option value="3">3</option>
      </select>
    </label>
  </div>
  <button type="submit" class="button">Create game</button>
</form>"#,
        seat2 = options(false).replace(
            "<option value=\"medium\">Medium bot</option>",
            "<option value=\"medium\" selected>Medium bot</option>"
        ),
        seat3 = options(true),
        seat4 = options(true),
    )
}

fn game_list(games: &[Game], current: Uuid) -> String {
    let mut rows = String::new();
    for game in games
        .iter()
        .filter(|game| game.status != GameStatus::Finished)
    {
        rows.push_str(&game_list_item(game, current));
    }
    if games.iter().any(|game| game.status == GameStatus::Finished) {
        rows.push_str(r#"<li class="game-list-divider"><span>Finished games</span></li>"#);
    }
    for game in games
        .iter()
        .filter(|game| game.status == GameStatus::Finished)
    {
        rows.push_str(&game_list_item(game, current));
    }
    format!("<ul class=\"game-list\">{rows}</ul>")
}

fn game_list_item(game: &Game, current: Uuid) -> String {
    let players: Vec<String> = game.seats.iter().map(|seat| escape(&seat.name)).collect();
    let status = match game.status {
        GameStatus::Lobby => "lobby",
        GameStatus::Active => "active",
        GameStatus::Finished => "finished",
    };
    let badge = if is_current_turn(game, current) {
        " <span class=\"badge badge-turn\">your turn</span>"
    } else {
        ""
    };
    let item_class = if game.status == GameStatus::Finished {
        "game-list-item is-finished"
    } else {
        "game-list-item"
    };
    format!(
        r#"<li class="{item_class}">
  <a href="/games/{id}">{players}</a>
  <span class="game-list-status muted">{status}</span>{badge}
</li>"#,
        id = game.id,
        players = players.join(" vs "),
    )
}

fn open_game_list(games: &[Game]) -> String {
    let rows: String = games.iter().map(open_game_list_item).collect();
    format!("<ul class=\"game-list open-game-list\">{rows}</ul>")
}

fn open_game_list_item(game: &Game) -> String {
    let players: Vec<String> = game.seats.iter().map(|seat| escape(&seat.name)).collect();
    let status = match game.status {
        GameStatus::Lobby => "lobby",
        GameStatus::Active => "active",
        GameStatus::Finished => "finished",
    };
    format!(
        r#"<li class="game-list-item open-game-list-item">
  <a href="/games/{id}">{players}</a>
  <span class="game-list-status muted">{status}</span>
  <form class="inline-join-form" method="post" action="/games/{id}/join">
    <button type="submit" class="button button-secondary">Join</button>
  </form>
</li>"#,
        id = game.id,
        players = players.join(" vs "),
    )
}

fn is_current_turn(game: &Game, current: Uuid) -> bool {
    if game.status != GameStatus::Active {
        return false;
    }
    game.seats.get(game.turn).is_some_and(
        |seat| matches!(seat.kind, SeatKind::Human { user_id } if user_id == Some(current)),
    )
}

pub fn game_page(
    view: &GameView,
    initial_json: &str,
    two_letter_json: &str,
    grandpa_two_letter_json: &str,
    other_games: &[GameSummary],
    logged_in: bool,
) -> String {
    let board = render_board_squares(&view.board, &view.last_play);
    let scoreboard = render_scoreboard(view);
    let log = render_move_log(view);
    let other = render_other_games(other_games);
    let status_banner = render_status_banner(view);
    let join = render_join_form(view);
    // Neutralize any "</..." sequence (e.g. a player name containing "</script>")
    // so embedded JSON can't break out of the surrounding <script> element.
    let initial_json = initial_json.replace("</", "<\\/");
    let initial_json = initial_json.as_str();
    let head = format!(
        r#"<script type="module" src="{}" defer></script>"#,
        asset("/public/game.js")
    );
    let head = head.as_str();
    let sign_in = if logged_in {
        String::new()
    } else {
        r#"<section class="card login-cta">
  <p class="muted">Sign in to join an open seat and play.</p>
  <a class="button" href="/">Sign in to play</a>
</section>"#
            .to_string()
    };
    let body = format!(
        r#"{sign_in}
<section class="card">
  <div id="ssr-fallback">
    {status_banner}
    <div class="game-layout">
      <div class="board-wrap">
        {board}
        {join}
      </div>
      <aside class="sidebar">
        {scoreboard}
        {log}
        {other}
      </aside>
    </div>
    <noscript><p class="muted">Enable JavaScript to place tiles and play.</p></noscript>
  </div>
  <div id="game-island" data-game-id="{id}"></div>
  <script id="game-state" type="application/json">{initial_json}</script>
  <script id="two-letter-words" type="application/json">{two_letter_json}</script>
  <script id="grandpa-two-letter-words" type="application/json">{grandpa_two_letter_json}</script>
</section>"#,
        id = view.id,
    );
    layout_with_head("Game — Screwball", &body, head, "game-page")
}

fn render_join_form(view: &GameView) -> String {
    let open_seat = view.seats.iter().any(|seat| seat.open);
    if view.your_seat.is_some() || !open_seat {
        return String::new();
    }
    format!(
        r#"<form class="join-form" method="post" action="/games/{id}/join">
  <p class="muted">An open seat is waiting. Join to play.</p>
  <button type="submit" class="button">Join game</button>
</form>"#,
        id = view.id,
    )
}

/// The "your other games" panel: every other game the viewer is seated in, with
/// games waiting on the viewer flagged "your turn".
fn render_other_games(games: &[GameSummary]) -> String {
    // Match the live OtherGames panel, which only lists in-progress games — else
    // finished games flash in on first paint and vanish once JS hydrates.
    let active: Vec<&GameSummary> = games.iter().filter(|game| game.is_active).collect();
    if active.is_empty() {
        return String::new();
    }
    let items: String = active
        .iter()
        .map(|game| {
            let players: Vec<String> = game.players.iter().map(|name| escape(name)).collect();
            let li_class = if game.your_turn {
                " class=\"your-turn\""
            } else {
                ""
            };
            let tag = if game.your_turn {
                "<span class=\"badge badge-turn\">your turn</span>".to_string()
            } else {
                format!("<span class=\"muted\">{}</span>", game.status)
            };
            format!(
                r#"<li{li_class}>
  <a href="/games/{id}">{players}</a>
  {tag}
</li>"#,
                id = game.id,
                players = players.join(" vs "),
            )
        })
        .collect();
    format!(
        r#"<div class="other-games">
  <h2>Your other games</h2>
  <ul class="other-games-list">{items}</ul>
</div>"#
    )
}

fn render_status_banner(view: &GameView) -> String {
    let text = match view.status {
        GameStatus::Lobby => "Waiting to start".to_string(),
        GameStatus::Active => {
            let seat = view.seats.get(view.turn);
            match seat {
                Some(seat) if seat.is_you => "Your turn".to_string(),
                Some(seat) => format!("{}'s turn", escape(&seat.name)),
                None => "In progress".to_string(),
            }
        }
        GameStatus::Finished => {
            let names: Vec<String> = view
                .winners
                .iter()
                .filter_map(|&i| view.seats.get(i))
                .map(|seat| escape(&seat.name))
                .collect();
            if names.is_empty() {
                "Game over".to_string()
            } else {
                format!("Game over — winner: {}", names.join(", "))
            }
        }
    };
    format!(r#"<h1 class="status">{text}</h1>"#)
}

fn render_scoreboard(view: &GameView) -> String {
    let rows: String = view
        .seats
        .iter()
        .map(|seat| {
            let turn = if seat.on_turn { " on-turn" } else { "" };
            let you = if seat.is_you {
                " <span class=\"badge\">you</span>"
            } else {
                ""
            };
            let kind = if seat.open {
                "open".to_string()
            } else if let Some(difficulty) = seat.difficulty {
                format!("{difficulty:?} bot")
            } else {
                "human".to_string()
            };
            let hints = if seat.hints_unlimited {
                r#" <span class="hint-count" title="unlimited hints">💡∞</span>"#.to_string()
            } else {
                match seat.hints_remaining {
                    Some(n) => {
                        format!(r#" <span class="hint-count" title="hints left">💡{n}</span>"#)
                    }
                    None => String::new(),
                }
            };
            format!(
                r#"<tr class="seat{turn}">
  <td>{name}{you}{hints}</td>
  <td class="muted">{kind}</td>
  <td class="score">{score}</td>
</tr>"#,
                name = escape(&seat.name),
                score = seat.score,
            )
        })
        .collect();
    format!(
        r#"<table class="scoreboard">
  <thead><tr><th>Player</th><th>Type</th><th>Score</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
<p class="muted">Tiles in bag: {bag}</p>"#,
        bag = view.bag_count,
    )
}

fn render_move_log(view: &GameView) -> String {
    if view.moves.is_empty() {
        return "<p class=\"muted\">No moves yet.</p>".to_string();
    }
    let items: String = view
        .moves
        .iter()
        .rev()
        .take(10)
        .map(|mv| {
            let name = view
                .seats
                .get(mv.seat)
                .map(|s| escape(&s.name))
                .unwrap_or_default();
            let detail = match mv.kind {
                "play" => format!("{} (+{})", mv.words.join(", "), mv.points),
                "exchange" => "exchanged tiles".to_string(),
                "adjustment" if mv.delta >= 0 => {
                    format!("out bonus (+{})", mv.delta)
                }
                "adjustment" => {
                    format!("leftover {} ({})", mv.words.join(""), mv.delta)
                }
                _ => "passed".to_string(),
            };
            format!("<li><strong>{name}</strong>: {detail}</li>")
        })
        .collect();
    format!("<ul class=\"move-log\">{items}</ul>")
}

fn render_board_squares(squares: &[SquareView], last_play: &[PositionView]) -> String {
    let mut cells = String::new();
    for (index, square) in squares.iter().enumerate() {
        let row = index / BOARD_SIZE;
        let col = index % BOARD_SIZE;
        match square.letter {
            Some(letter) => {
                let blank = if square.is_blank { " tile-blank" } else { "" };
                // Match the client, which rings the most recent play's tiles.
                let last = if last_play.iter().any(|p| p.row == row && p.col == col) {
                    " last-play"
                } else {
                    ""
                };
                let points = crate::models::letter_points(letter);
                let shown = if square.is_blank { 0 } else { points };
                cells.push_str(&format!(
                    r#"<div class="cell tile{blank}{last}" data-row="{row}" data-col="{col}"><span class="tile-letter">{letter}</span><span class="tile-points">{shown}</span></div>"#,
                ));
            }
            None => {
                let class = format!("premium-{}", square.premium);
                let center = row == BOARD_SIZE / 2 && col == BOARD_SIZE / 2;
                let label = if center {
                    "★"
                } else {
                    premium_label(square.premium)
                };
                cells.push_str(&format!(
                    r#"<div class="cell {class}" data-row="{row}" data-col="{col}"><span class="premium-label">{label}</span></div>"#,
                ));
            }
        }
    }
    format!(r#"<div class="board" role="grid">{cells}</div>"#)
}

fn premium_label(code: &str) -> &'static str {
    match code {
        "dl" => "DL",
        "tl" => "TL",
        "dw" => "DW",
        "tw" => "TW",
        _ => "",
    }
}

/// Low-level board rendering helper used by tests.
pub fn render_board(board: &Board) -> String {
    let squares: Vec<SquareView> = board
        .squares
        .iter()
        .map(|square| SquareView {
            premium: premium_code(square.premium),
            letter: square.tile.map(|t| t.letter),
            is_blank: square.tile.is_some_and(|t| t.is_blank),
        })
        .collect();
    render_board_squares(&squares, &[])
}
