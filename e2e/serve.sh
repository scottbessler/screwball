#!/usr/bin/env bash
# Starts the server for the Playwright snapshot tests on a throwaway copy of
# the committed fixture data. Expects the debug binary to be built already
# (cargo build).
set -euo pipefail
cd "$(dirname "$0")/.."

DATA_DIR=$(mktemp -d)
trap 'rm -rf "$DATA_DIR"' EXIT
cp -r e2e/fixture-data/. "$DATA_DIR"

DATA_PATH="$DATA_DIR" PASSKEY_DISABLED=1 PORT="${PORT:-8123}" exec ./target/debug/screwball
