# Screwball

Screwball is a Scrabble-style web letter game for 2–4 players (humans and/or
heuristic computer opponents). It follows the same architecture as
[lisports](https://github.com/scottbessler/lisports): a Rust + Axum server that
renders HTML, with a small client-side island for the interactive board.

See [`SPEC.md`](./SPEC.md) for the full design and milestone plan.

## Status

Early skeleton + game engine. Implemented so far:

- Axum server, server-rendered home page, static assets, `/healthcheck`.
- Game engine: board with standard premium squares, tile bag + English
  distribution, dictionary (NWL2023, configurable), move validation (placement,
  contiguity, connectivity, word checks), scoring (premiums + bingo bonus),
  turn rotation for 2–4 seats, exchange/pass, and end-game scoring.

Still to come (see `SPEC.md`): heuristic computer opponent, passkey auth,
create/join/move HTTP routes, the Preact board island, and real-time updates.

## Routes

- `/` — home page
- `/demo` — renders an empty board with premium squares
- `/healthcheck` — returns `OK`
- `/public/*` — static assets

## Development

```sh
cargo run            # serves on http://localhost:8080
./dev.sh             # auto-restart on source/asset changes
PORT=3000 cargo run  # change the port
```

The dictionary defaults to the bundled NWL2023 word list, whose entries also
supply the word definitions shown in the move log. Override it with a path to a
newline-separated word list (definitions are optional, in the same NWL2023
format):

```sh
DICTIONARY_PATH=/path/to/words.txt cargo run
```

### Authentication

Players sign in with passkeys (WebAuthn) — no passwords. Configure the relying
party and session signing via environment variables:

| Variable         | Default                   | Purpose                                                            |
| ---------------- | ------------------------- | ------------------------------------------------------------------ |
| `RP_ID`          | `localhost`               | WebAuthn relying-party id (the registrable domain).                |
| `RP_ORIGIN`      | `http://localhost:8080`   | Full origin browsers connect from; must match `RP_ID`.             |
| `SESSION_SECRET` | stable in debug builds, ephemeral in release builds | ≥64-byte secret signing the session cookie. Unset in local `cargo run` uses a stable dev key; unset in release uses a random key and sessions reset on restart. |
| `VAPID_PRIVATE_KEY` | stable in debug builds, disabled in release builds | Base64url P-256 private key for Web Push notifications. Unset in local `cargo run` uses a stable dev key; unset in release disables push notifications. |
| `VAPID_SUBJECT` | `mailto:admin@example.com` | VAPID contact subject sent to push services. |

In production set `RP_ID`/`RP_ORIGIN` to your real domain (e.g. `RP_ID=play.example.com`,
`RP_ORIGIN=https://play.example.com`), a stable `SESSION_SECRET`, and a
`VAPID_PRIVATE_KEY` for browser push notifications.

## Checks

```sh
cargo fmt --check
cargo check --locked
cargo check --release --locked
cargo clippy --locked --all-targets --all-features
cargo test --locked
bun run lint
```

`mise run setup` enables the repo git hooks. The pre-push hook runs the full
check gate above automatically when pushing to `main`; `mise run check` runs the
same gate manually.

## Deployment

A Docker image builds the release binary and runs it directly; `fly.toml`
deploys to Fly.io with a persistent volume mounted at `/data` (matching the
production `DATA_PATH`). Deploying requires a Fly app named `screwball` and a
`FLY_SCREWBALL_DEPLOY_TOKEN` GitHub secret.
