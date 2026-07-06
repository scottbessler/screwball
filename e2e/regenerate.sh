#!/usr/bin/env bash
# Regenerates e2e/fixture-data from scratch by seeding a temporary server.
# Screenshots depend on this data; after regenerating, re-approve them with
#   bun run test:e2e -- --update-snapshots
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build
rm -rf e2e/fixture-data
mkdir -p e2e/fixture-data

DATA_PATH=e2e/fixture-data PASSKEY_DISABLED=1 PORT=8123 ./target/debug/screwball &
SERVER_PID=$!
trap 'kill "$SERVER_PID"' EXIT

for _ in $(seq 1 50); do
  curl -fsS http://localhost:8123/healthcheck >/dev/null 2>&1 && break
  sleep 0.2
done

SEED_OUT=$(node e2e/seed.mjs)
echo "$SEED_OUT"

# Mark the third game finished so the home page shows a Finished section.
FINISHED_ID=$(echo "$SEED_OUT" | node -e 'const d=JSON.parse(require("fs").readFileSync(0,"utf8"));console.log(d.finished.replace("/games/",""))')
kill "$SERVER_PID"
trap - EXIT
wait "$SERVER_PID" 2>/dev/null || true
node -e '
const fs = require("fs");
const path = `e2e/fixture-data/games/${process.argv[1]}.json`;
const game = JSON.parse(fs.readFileSync(path, "utf8"));
game.status = "Finished";
fs.writeFileSync(path, JSON.stringify(game));
' "$FINISHED_ID"

# The definitions cache fills up lazily in the background; drop it so the
# fixture stays minimal and deterministic.
rm -rf e2e/fixture-data/definitions*
echo "fixture written to e2e/fixture-data"
