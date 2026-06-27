# Screwball — Spec (draft)

A web letter game in the style of Scrabble / Lexulous: 2–4 players build
crossword words from lettered tiles on a 15×15 board for points, against each
other and/or heuristic computer opponents. Same architectural shape as
**lisports** — Rust + Axum, server-rendered HTML templates, static assets in
`public/` — plus a small client-side island for the interactive board.

## 1. Goals / non-goals

- **Goal:** playable 2–4 player games (any mix of humans + bots) over a
  shareable URL; server is the source of truth for board, racks, bag, scores,
  and word validation.
- **Goal:** passkey-only (WebAuthn) sign-in — no passwords.
- **Goal:** heuristic computer opponent(s) with selectable difficulty.
- **Goal:** keep the frontend small and dependency-light, matching lisports'
  "SSR + sprinkles of JS" philosophy.
- **Non-goals (v1):** matchmaking/public lobby, chat, ratings/ELO, social
  features. Easy follow-ups once the core loop works.

## 2. Tech stack

- Rust 2024, Axum 0.8, Tokio, Serde, `tower-http` (static files), tracing —
  mirrors lisports' `Cargo.toml`.
- `webauthn-rs` for passkey registration/authentication; signed session cookie.
- SSR HTML via string templates (a `render.rs` analog).
- One interactive page (the board) hydrated by a small JS bundle.
- Deploy via Docker → Fly.io, same as lisports (`Dockerfile`, `fly.toml`,
  persistent volume for users + game state at `DATA_PATH`).

## 3. Frontend: recommendation

The board needs real interactivity (drag tiles from rack to squares, tentative
placement, live opponent moves) — more than lisports' vanilla sprinkles, but far
less than a full SPA. Options considered:

| Option | Size | Fit |
| --- | --- | --- |
| **Preact + `@preact/signals`** | ~6 kB | Component model + fine-grained reactivity; ideal for a board grid + rack with frequent local state updates. |
| Alpine.js | ~15 kB | Great for HTML-attribute sprinkles, awkward for a stateful grid + drag/drop. |
| htmx | ~14 kB | Server-driven; round-trips per interaction feel laggy for tile dragging. |
| Lit (web components) | ~6 kB | Solid, but more boilerplate than Preact for this. |
| Vanilla + Web Components | 0 kB | Doable, but we'd reinvent reactivity. |

**Recommendation: Preact + `@preact/signals`**, bundled with **Vite** (run via
`bun`, which is already pinned in lisports' `.mise.toml`). Rationale:

- Smallest mainstream reactive option (~6 kB) with a real component model.
- Signals give clean, fast board/rack state without a heavy store.
- Build emits a single hashed JS/CSS bundle into `public/build/`; Rust serves it
  as a static asset. Only the `/games/{id}` page loads it — every other page
  stays pure SSR.
- TS + JSX, lints with `oxlint` (already configured in lisports).

The board's initial state is SSR'd into the page (so it's visible without JS)
and the bundle hydrates it. Drag/drop via native pointer events (no extra dnd
library). Passkey ceremonies use the browser's native WebAuthn API directly
(`navigator.credentials`), no library needed.

## 4. Game rules (classic Scrabble, 2–4 players)

- 15×15 board with standard premium squares (DL/TL/DW/TW), center star is DW.
- 100-tile bag, classic English letter distribution + point values, 2 blanks.
- Rack of 7 tiles per player; refill to 7 after each turn from the bag.
- Turn order rotates through 2–4 seats. A turn is one of: **play** (place ≥1
  tile forming valid words), **exchange** (swap tiles, only if ≥7 in bag), or
  **pass**.
- First move must cover the center; later moves must connect to existing tiles.
- Placed tiles must be in a single row or column and contiguous (with existing
  tiles). Every main + cross word formed must be in the dictionary.
- Scoring: letter/word multipliers (premiums only score the turn they're
  covered), +50 bingo bonus for using all 7 tiles.
- Game ends when the bag is empty and a player empties their rack, or after
  6 consecutive scoreless turns. End-game subtracts each player's remaining
  rack value; a player who went out gains the sum of others' remaining tiles.

## 5. Dictionary & move generation

- Default word list: TWL06
  (`https://github.com/kamilmielnik/scrabble-dictionaries`, `english/twl06.txt`),
  bundled at build time. **Configurable** via env (`DICTIONARY_PATH` / a named
  set) so SOWPODS/ENABLE/other languages can be dropped in.
- Built at startup into a **DAWG/GADDAG** rather than a plain `HashSet`: this
  gives O(word) validation *and* efficient legal-move enumeration for the bot
  (a `HashSet` can validate but can't generate moves cheaply). GADDAG is the
  standard structure for fast cross-checked move generation on a Scrabble board.

## 6. Computer opponent (heuristic)

- A seat can be a bot with a difficulty level. On its turn the engine:
  1. Generates all legal plays for the rack against the current board (GADDAG +
     per-square cross-checks — the classic Appel–Jacobson algorithm).
  2. Scores each, then picks by a difficulty heuristic:
     - **Easy:** random among low/mid-scoring plays; prefers short words.
     - **Medium:** highest raw score most turns, occasional suboptimal.
     - **Hard:** highest score with light leave/board heuristics (rack leave
       value, avoid opening premium squares for opponents).
- Bots run server-side in the same engine; their "turn" is just a move the
  server makes and broadcasts. No special client support needed.

## 7. Auth (passkey-only)

- `webauthn-rs` relying party. Registration: user picks a display name →
  WebAuthn create ceremony → store credential. Login: WebAuthn get ceremony.
- Signed, HTTP-only session cookie identifies the user; no passwords ever.
- Users + credentials persisted to disk under `DATA_PATH`.
- Minimal: a user is `{ id, display_name, credentials, created_at }`.

## 8. Data model (server, in-memory + persisted)

```
User { id, display_name, credentials: Vec<Passkey>, created_at }
Game { id, status: Lobby|Active|Finished, board: [Square; 225],
       bag: Vec<Tile>, seats: Vec<Seat> (2..=4), turn: usize,
       moves: Vec<Move>, consecutive_scoreless: u8, created_at }
Seat { id, kind: Human{ user_id } | Bot{ difficulty },
       name, rack: Vec<Tile>, score }
Square { premium: None|DL|TL|DW|TW, tile: Option<PlacedTile> }
Tile = letter (A–Z) or Blank; PlacedTile remembers the blank's chosen letter
Move { seat, kind: Play{placements}|Exchange{n}|Pass, words, points }
```

- State held in `Arc<RwLock<HashMap<GameId, Game>>>`.
- Persisted to disk (`DATA_PATH`, like lisports' cache) on each move so a
  restart resumes in-progress games; periodic prune of finished/stale games.
- A player only ever receives their own rack; opponent/bot racks are redacted.

## 9. Routes

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/` | Home: sign in, create a game, list your games |
| POST | `/auth/register/begin` · `/finish` | WebAuthn registration ceremony |
| POST | `/auth/login/begin` · `/finish` | WebAuthn login ceremony |
| POST | `/auth/logout` | Clear session |
| POST | `/games` | Create game (player count, which seats are bots + difficulty) → redirect to `/games/{id}` |
| GET | `/games/{id}` | SSR game page (board, your rack, scores) + hydration bundle |
| POST | `/games/{id}/join` | Join an open human seat |
| GET | `/games/{id}/state` | JSON state for the requesting user (initial hydrate + poll fallback) |
| POST | `/games/{id}/move` | Submit play / exchange / pass; server validates + scores |
| GET | `/games/{id}/ws` | WebSocket: pushes new state to seated players on each move |
| GET | `/healthcheck` | `OK` |
| GET | `/public/*` | Static assets (built bundle, CSS, favicon) |

Invalid IDs/params → 400/404; unauthenticated actions → 401; illegal moves →
422 with a reason the UI shows.

## 10. Real-time

WebSocket per game (Axum has built-in WS) is the primary channel: on a valid
move (human or bot) the server broadcasts the updated, per-player-redacted state
to all seated connections. Polling `/games/{id}/state` is the no-JS / reconnect
fallback (reuse lisports' `data-refresh-at` pattern).

## 11. Project layout (mirrors lisports)

```
src/  app.rs (router/state) · routes.rs (handlers) · auth.rs (webauthn/session)
      game.rs (rules/engine) · board.rs · bag.rs · dict.rs (DAWG/GADDAG)
      bot.rs (move gen + heuristics) · models.rs · render.rs · error.rs · main.rs
web/  Preact source (board, rack, drag/drop, auth ceremonies) → public/build/
public/  app.css, favicon, manifest, built bundle
data/   persisted users + games (gitignored)
tests/  engine unit tests (scoring, validation, move gen) + route integration tests
```

## 12. Milestones

1. Skeleton: Axum server, SSR home + static board render, `/healthcheck`, Docker/Fly.
2. Engine: bag, racks, placement validation, DAWG/GADDAG dictionary, scoring,
   end-game, 2–4 turn rotation (unit-tested, no UI yet).
3. Bot: legal-move generation + difficulty heuristics over the engine.
4. Auth: passkey register/login, sessions, user persistence.
5. Routes + persistence: create/join/move/state, disk save, per-user redaction.
6. Frontend island: Preact board + rack, drag/drop, submit move, show errors,
   passkey ceremonies.
7. Real-time: WebSocket broadcast; polling fallback.
8. Polish: game setup UI (player count + bots), end-game screen, exchange/pass
   UI, mobile layout, blank-tile picker.

## 13. Tasks


* [x] Tweak the difficulties a bit:
Easy - bottom 50% still
Chill - middle 50%
Medium - top 25% 
Hard - top 10%
Impossible - top (current hard)
* [x] Change "John Mode" to actually show the valid 2-letter words that include the selected/hovered-over letter (but not disallow them)
* [x] Add "Grandpa Mode" to disallow almost all 2-letter words (allow the super common ones like am, an, me, hi)
* [x] Game start screen doesnt work on mobile (buttons/controls are lost to overflow)
* [x] Touch drag+drop on mobile should have the dragged item be above where the touch is so it isnt hidden under your finger
* [x] Touch drag+drop drop locations are not forgiving enough (i.e. go a little to far to the left/right/up/down 
* [x] Touch drag+drop should animate letter-movement more smoothly 
* [x] Touching on letters to play them in typing mode should not move the other letters so you can type faster
* [x] Find and fix an obvious UI bug — new-game form checkbox labels rendered with checkbox centered above text
* [x] Find and fix an obvious UI bug — undefined `--border` CSS var (missing mobile sidebar separator)
* [x] Find and fix an obvious UI bug — SSR/demo board center square showed "DW" instead of ★
* [x] formatting still broken when using the PWA — root cause was stale cached CSS; fixed via versioned asset URLs + no-cache HTML (see PWA caching task)
* [x] i should be able to drag letters back and forth on mobile while my finger is below the actual rack. the letter being dragged properly shows up above my finger but the drop targets need to extend lower. — reorder band now extends ~120px below the rack
* [x] while dragging letters on the rack, we should be previewing the result better — tiles reorder live toward the nearest tile centre, showing where the dragged tile lands
* [x] while dragging letters on the rack, the changes to the preview should be more smoothly animated — FLIP animation slides tiles to new positions
* [x] while dragging letters on the rack, there should not be flickering of the preview — nearest-centre targeting only changes at midpoints (no edge flip-flop); manual reorder-pop removed
* [x] should be able to drag a placed letter from 1 spot on board to another — pending board tiles are draggable (mouse + touch) to any empty square via movePending
* [x] Find and fix an obvious UI bug — buttons outside `.form` (sign-out, join, Play word) had default UA borders; reset moved to base `.button`
* [x] Find and fix an obvious UI bug — scoreboard showed unfilled open seat as "human" instead of "open"
* [x] Find and fix an obvious UI bug — SSR scoreboard showed a bot as "bot" while hydrated client showed "Easy bot"
* [x] add and apply strict rust linting / compiling — Cargo.toml denies warnings + clippy::all; `mise run lint:rust` runs fmt + clippy; tree is clean
* [x] add and apply strict js linting — oxlint (`.oxlintrc.json`, vendor ignored) via `bun run lint --deny-warnings`; `mise run lint:js`
* [x] add precommit hooks that enforce/fix formatting/linting — `.githooks/pre-commit` runs fmt+clippy+oxlint; enable with `mise run setup`
* [x] formatting of game options is poorly aligned and ugly — checkbox labels now left-aligned rows; hint on its own indented line
* [x] pwa on ios seems to cache css/js very aggressively — asset URLs now carry a content-hash `?v=`; `/public` is cached immutable, HTML/JSON are no-cache so new asset links are always picked up
* [x] show how many remaining hints each player has in the score summary if hints are enabled — 💡N badge per human seat in the scoreboard (SSR + client)
* [ ] consider using https://shopify.github.io/draggable or a similar library to ensure our drag+drop is robust
* [x] the grandpa list should include other very common 2 letter words as well — expanded to AM AN AS AT BE BY DO GO HE HI IF IN IS IT ME MY NO OF OH ON OR SO TO UP US WE
* [ ] in game log, show word definitions (there has to be some free api somewhere maybe https://dictionaryapi.dev)
```
