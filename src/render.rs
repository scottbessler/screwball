use uuid::Uuid;

use crate::models::{BOARD_SIZE, Board, Game, GameStatus, SeatKind};
use crate::view::{GameView, SquareView, premium_code};

pub fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn layout(title: &str, body: &str) -> String {
    layout_with_head(title, body, "")
}

fn layout_with_head(title: &str, body: &str, head_extra: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>{title}</title>
  <link rel="icon" href="/public/favicon.svg">
  <link rel="manifest" href="/public/manifest.webmanifest">
  <link rel="stylesheet" href="/public/app.css">
  {head_extra}
</head>
<body>
  {nav}
  <main class="page">
  {body}
  </main>
</body>
</html>"#,
        title = escape(title),
        head_extra = head_extra,
        nav = nav(),
        body = body,
    )
}

pub fn nav() -> &'static str {
    r#"<nav class="nav">
  <a class="brand" href="/">Screwball</a>
  <div class="nav-links">
    <a href="/">Games</a>
    <a href="/demo">Demo board</a>
  </div>
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

pub fn home_page(games: &[Game], current: Uuid) -> String {
    let new_game = new_game_form();
    let list = if games.is_empty() {
        "<p class=\"muted\">No games yet. Create one to get started.</p>".to_string()
    } else {
        let rows: String = games
            .iter()
            .map(|game| game_list_item(game, current))
            .collect();
        format!("<ul class=\"game-list\">{rows}</ul>")
    };
    layout(
        "Screwball",
        &format!(
            r#"<section class="card">
  <h1>New game</h1>
  {new_game}
</section>
<section class="card">
  <h1>Your games</h1>
  {list}
</section>"#
        ),
    )
}

fn new_game_form() -> String {
    let options = |selected_off: bool| {
        let off = if selected_off { " selected" } else { "" };
        format!(
            r#"<option value="off"{off}>— none —</option>
<option value="open">Open seat (human)</option>
<option value="easy">Easy bot</option>
<option value="medium">Medium bot</option>
<option value="hard">Hard bot</option>"#
        )
    };
    format!(
        r#"<form method="post" action="/games" class="form">
  <label>Your name
    <input type="text" name="your_name" maxlength="24" placeholder="You">
  </label>
  <label>Seat 2
    <select name="seat2">{seat2}</select>
  </label>
  <label>Seat 3
    <select name="seat3">{seat3}</select>
  </label>
  <label>Seat 4
    <select name="seat4">{seat4}</select>
  </label>
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

fn game_list_item(game: &Game, current: Uuid) -> String {
    let seated = game.seats.iter().any(|seat| match seat.kind {
        SeatKind::Human { user_id } => user_id == Some(current),
        SeatKind::Bot { .. } => false,
    });
    let players: Vec<String> = game.seats.iter().map(|seat| escape(&seat.name)).collect();
    let status = match game.status {
        GameStatus::Lobby => "lobby",
        GameStatus::Active => "active",
        GameStatus::Finished => "finished",
    };
    let badge = if seated {
        " <span class=\"badge\">you</span>"
    } else {
        ""
    };
    format!(
        r#"<li>
  <a href="/games/{id}">{players}</a>
  <span class="muted">{status}</span>{badge}
</li>"#,
        id = game.id,
        players = players.join(" vs "),
    )
}

pub fn game_page(view: &GameView, initial_json: &str) -> String {
    let board = render_board_squares(&view.board);
    let scoreboard = render_scoreboard(view);
    let log = render_move_log(view);
    let status_banner = render_status_banner(view);
    let join = render_join_form(view);
    // Neutralize any "</..." sequence (e.g. a player name containing "</script>")
    // so embedded JSON can't break out of the surrounding <script> element.
    let initial_json = initial_json.replace("</", "<\\/");
    let initial_json = initial_json.as_str();
    let head = r#"<script type="module" src="/public/game.js" defer></script>"#;
    let body = format!(
        r#"<section class="card">
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
      </aside>
    </div>
    <noscript><p class="muted">Enable JavaScript to place tiles and play.</p></noscript>
  </div>
  <div id="game-island" data-game-id="{id}"></div>
  <script id="game-state" type="application/json">{initial_json}</script>
</section>"#,
        id = view.id,
    );
    layout_with_head("Game — Screwball", &body, head)
}

fn render_join_form(view: &GameView) -> String {
    let open_seat = view.seats.iter().any(|seat| seat.open);
    if view.your_seat.is_some() || !open_seat {
        return String::new();
    }
    format!(
        r#"<form class="join-form" method="post" action="/games/{id}/join">
  <p class="muted">An open seat is waiting. Join to play.</p>
  <label>Your name
    <input type="text" name="name" maxlength="24" placeholder="You">
  </label>
  <button type="submit" class="button">Join game</button>
</form>"#,
        id = view.id,
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
            format!(
                r#"<tr class="seat{turn}">
  <td>{name}{you}</td>
  <td class="muted">{kind}</td>
  <td class="score">{score}</td>
</tr>"#,
                name = escape(&seat.name),
                kind = seat.kind,
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

fn render_board_squares(squares: &[SquareView]) -> String {
    let mut cells = String::new();
    for (index, square) in squares.iter().enumerate() {
        let row = index / BOARD_SIZE;
        let col = index % BOARD_SIZE;
        match square.letter {
            Some(letter) => {
                let blank = if square.is_blank { " tile-blank" } else { "" };
                let points = crate::models::letter_points(letter);
                let shown = if square.is_blank { 0 } else { points };
                cells.push_str(&format!(
                    r#"<div class="cell tile{blank}" data-row="{row}" data-col="{col}"><span class="tile-letter">{letter}</span><span class="tile-points">{shown}</span></div>"#,
                ));
            }
            None => {
                let class = format!("premium-{}", square.premium);
                cells.push_str(&format!(
                    r#"<div class="cell {class}" data-row="{row}" data-col="{col}"><span class="premium-label">{label}</span></div>"#,
                    label = premium_label(square.premium),
                ));
            }
        }
    }
    format!(r#"<div class="board" role="grid">{cells}</div>"#)
}

pub fn demo_page(board: &Board) -> String {
    let squares: Vec<SquareView> = board
        .squares
        .iter()
        .map(|square| SquareView {
            premium: premium_code(square.premium),
            letter: square.tile.map(|t| t.letter),
            is_blank: square.tile.is_some_and(|t| t.is_blank),
        })
        .collect();
    layout(
        "Demo board — Screwball",
        &format!(
            r#"<section class="card">
  <h1>Demo board</h1>
  <p>An empty 15&times;15 board with standard premium squares.</p>
  {}
</section>"#,
            render_board_squares(&squares)
        ),
    )
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

/// Still used by tests and as a low-level helper for the demo board.
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
    render_board_squares(&squares)
}
