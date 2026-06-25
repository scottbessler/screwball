#!/bin/sh

set -eu

# Dependency-free dev runner for the Rust service. It restarts the server when
# source, static assets, or Cargo metadata change.

WATCH_PATHS="${WATCH_PATHS:-src public Cargo.toml Cargo.lock}"
POLL_INTERVAL="${POLL_INTERVAL:-1}"
DEV_CMD="${DEV_CMD:-cargo run}"

if [ "${1:-}" = "--" ]; then
	shift
	DEV_CMD="$*"
fi

child_pid=""

file_stamp() {
	stat -f "%m:%z" "$1" 2>/dev/null || stat -c "%Y:%s" "$1"
}

snapshot() {
	for path in $WATCH_PATHS; do
		if [ -e "$path" ]; then
			find "$path" -type f -exec sh -c '
				file_stamp() {
					stat -f "%m:%z" "$1" 2>/dev/null || stat -c "%Y:%s" "$1"
				}

				for file do
					printf "%s:%s\n" "$file" "$(file_stamp "$file")"
				done
			' sh {} +
		fi
	done | sort
}

stop_server() {
	if [ -n "$child_pid" ] && kill -0 "$child_pid" 2>/dev/null; then
		printf '\n[dev] stopping pid %s\n' "$child_pid"
		kill "$child_pid" 2>/dev/null || true
		wait "$child_pid" 2>/dev/null || true
	fi

	child_pid=""
}

start_server() {
	printf '\n[dev] starting: %s\n' "$DEV_CMD"
	sh -c "exec $DEV_CMD" &
	child_pid="$!"
}

cleanup() {
	trap - INT TERM EXIT
	stop_server
}

trap cleanup INT TERM EXIT

printf '[dev] watching: %s\n' "$WATCH_PATHS"
printf '[dev] poll interval: %ss\n' "$POLL_INTERVAL"

last_snapshot="$(snapshot)"
start_server

while true; do
	sleep "$POLL_INTERVAL"

	if [ -n "$child_pid" ] && ! kill -0 "$child_pid" 2>/dev/null; then
		wait "$child_pid" 2>/dev/null || true
		child_pid=""
		printf '\n[dev] command exited; waiting for changes\n'
	fi

	next_snapshot="$(snapshot)"
	if [ "$next_snapshot" != "$last_snapshot" ]; then
		last_snapshot="$next_snapshot"
		printf '\n[dev] change detected; restarting\n'
		stop_server
		start_server
	fi
done
