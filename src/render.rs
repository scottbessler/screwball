use crate::models::{Board, Position, Premium};

pub fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn layout(title: &str, body: &str) -> String {
    layout_with_scripts(title, body, "")
}

fn layout_with_scripts(title: &str, body: &str, scripts: &str) -> String {
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
</head>
<body>
  {nav}
  <main class="page">
  {body}
  </main>
  {scripts}
</body>
</html>"#,
        title = escape(title),
        nav = nav(),
        body = body,
        scripts = scripts,
    )
}

pub fn nav() -> &'static str {
    r#"<nav class="nav">
  <a class="brand" href="/">Screwball</a>
  <div class="nav-links">
    <a href="/">Home</a>
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

pub fn home_page() -> String {
    layout(
        "Screwball",
        r#"<section class="card">
  <h1>Screwball</h1>
  <p>A Scrabble-style letter game for 2&ndash;4 players, with heuristic computer
     opponents. Server-rendered with a small Preact board island.</p>
  <p>This is an early skeleton. The game engine (board, bag, dictionary,
     validation, scoring) is in place; play, accounts, and the interactive board
     land in upcoming milestones.</p>
  <p><a class="button" href="/demo">View a demo board</a></p>
</section>"#,
    )
}

pub fn demo_page(board: &Board) -> String {
    layout(
        "Demo board — Screwball",
        &format!(
            r#"<section class="card">
  <h1>Demo board</h1>
  <p>An empty 15&times;15 board with standard premium squares.</p>
  {}
</section>"#,
            render_board(board)
        ),
    )
}

/// Render a board as a static HTML grid. Premium squares carry a class so CSS
/// can color them; placed tiles render their letter and point value.
pub fn render_board(board: &Board) -> String {
    let mut cells = String::new();
    for row in 0..crate::models::BOARD_SIZE {
        for col in 0..crate::models::BOARD_SIZE {
            let pos = Position::new(row, col);
            let square = board.square(pos);
            let premium_class = premium_class(square.premium);
            match square.tile {
                Some(tile) => {
                    let blank_class = if tile.is_blank { " tile-blank" } else { "" };
                    cells.push_str(&format!(
                        r#"<div class="cell tile{blank}"><span class="tile-letter">{letter}</span><span class="tile-points">{points}</span></div>"#,
                        blank = blank_class,
                        letter = escape(&tile.letter.to_string()),
                        points = tile.points(),
                    ));
                }
                None => {
                    let label = premium_label(square.premium);
                    cells.push_str(&format!(
                        r#"<div class="cell {class}"><span class="premium-label">{label}</span></div>"#,
                        class = premium_class,
                        label = label,
                    ));
                }
            }
        }
    }
    format!(r#"<div class="board" role="grid">{cells}</div>"#)
}

fn premium_class(premium: Premium) -> &'static str {
    match premium {
        Premium::None => "premium-none",
        Premium::DoubleLetter => "premium-dl",
        Premium::TripleLetter => "premium-tl",
        Premium::DoubleWord => "premium-dw",
        Premium::TripleWord => "premium-tw",
    }
}

fn premium_label(premium: Premium) -> &'static str {
    match premium {
        Premium::None => "",
        Premium::DoubleLetter => "DL",
        Premium::TripleLetter => "TL",
        Premium::DoubleWord => "DW",
        Premium::TripleWord => "TW",
    }
}
