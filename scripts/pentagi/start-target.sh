#!/bin/sh
# Start the FigTether daemon as the PentAGI assessment target.
#
# The daemon runs on a dedicated port and with a dedicated runtime directory,
# so an assessment never occupies the default bridge port and never touches the
# production runtime directory. The script waits until the health endpoint of
# the daemon answers, then prints the port and the runtime directory.
#
# The script is safe to run twice: when a daemon already answers on the
# assessment port, the existing one is reused and no second daemon is started.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/pentagi/start-target.ps1.
#
# Usage: sh scripts/pentagi/start-target.sh [--port PORT] [--runtime-dir PATH]
#                                          [--binary PATH] [--timeout SECONDS]

set -eu

# The default bridge port of FigTether. The assessment target never uses it.
DEFAULT_BRIDGE_PORT=38947

# The port of the assessment target.
ASSESSMENT_PORT=39047

# Seconds to wait for the health endpoint.
WAIT_TIMEOUT=30

# The repository root, used for the default runtime directory and the default
# binary lookup.
REPO_ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"

# The dedicated runtime directory of the assessment target.
RUNTIME_DIR="$REPO_ROOT/tmp/pentagi-assessment/runtime"

# The daemon binary. Empty means "look it up".
BINARY=""

log() {
	printf '%s\n' "$*"
}

fail() {
	printf 'start-target: %s\n' "$*" >&2
	exit 1
}

usage() {
	log 'usage: sh scripts/pentagi/start-target.sh [--port PORT] [--runtime-dir PATH]'
	log '                                           [--binary PATH] [--timeout SECONDS]'
}

# The platform runtime directory of a production FigTether installation.
platform_runtime_dir() {
	if [ -n "${LOCALAPPDATA:-}" ]; then
		printf '%s' "$LOCALAPPDATA/figtether"
		return 0
	fi
	if [ -n "${XDG_STATE_HOME:-}" ]; then
		printf '%s' "$XDG_STATE_HOME/figtether"
		return 0
	fi
	if [ -n "${HOME:-}" ]; then
		printf '%s' "$HOME/.local/state/figtether"
		return 0
	fi
	printf ''
}

# A comparable form of a path. The comparison only has to recognise the
# production runtime directory, so a false refusal is acceptable and a false
# acceptance is not.
normalize_path() {
	printf '%s' "$1" | tr 'A-Z' 'a-z' | tr '\\' '/' | sed -e 's:/*$::'
}

while [ "$#" -gt 0 ]; do
	case "$1" in
	--port)
		[ "$#" -ge 2 ] || fail "--port needs a value"
		ASSESSMENT_PORT="$2"
		shift 2
		;;
	--runtime-dir)
		[ "$#" -ge 2 ] || fail "--runtime-dir needs a value"
		RUNTIME_DIR="$2"
		shift 2
		;;
	--binary)
		[ "$#" -ge 2 ] || fail "--binary needs a value"
		BINARY="$2"
		shift 2
		;;
	--timeout)
		[ "$#" -ge 2 ] || fail "--timeout needs a value"
		WAIT_TIMEOUT="$2"
		shift 2
		;;
	--help | -h)
		usage
		exit 0
		;;
	*)
		fail "unknown argument: $1"
		;;
	esac
done

case "$ASSESSMENT_PORT" in
'' | *[!0-9]*) fail "the assessment port must be a number: $ASSESSMENT_PORT" ;;
esac
[ "$ASSESSMENT_PORT" -ge 1 ] || fail "the assessment port must be above zero"
[ "$ASSESSMENT_PORT" -le 65535 ] || fail "the assessment port must be below 65536"
if [ "$ASSESSMENT_PORT" -eq "$DEFAULT_BRIDGE_PORT" ]; then
	fail "the assessment target must not use the default bridge port $DEFAULT_BRIDGE_PORT"
fi

case "$WAIT_TIMEOUT" in
'' | *[!0-9]*) fail "the timeout must be a number: $WAIT_TIMEOUT" ;;
esac

PRODUCTION_DIR="$(platform_runtime_dir)"
if [ -n "$PRODUCTION_DIR" ]; then
	if [ "$(normalize_path "$RUNTIME_DIR")" = "$(normalize_path "$PRODUCTION_DIR")" ]; then
		fail "the assessment target must not use the production runtime directory: $PRODUCTION_DIR"
	fi
fi

case "$RUNTIME_DIR" in
'') fail "the runtime directory must not be empty" ;;
esac

if ! command -v curl >/dev/null 2>&1; then
	fail 'curl is required to read the health endpoint'
fi

mkdir -p "$RUNTIME_DIR"
RUNTIME_DIR="$(CDPATH='' cd -- "$RUNTIME_DIR" && pwd)"

# Locate the daemon binary.
if [ -z "$BINARY" ]; then
	for candidate in \
		"$REPO_ROOT/target/release/figtether" \
		"$REPO_ROOT/target/release/figtether.exe" \
		"$REPO_ROOT/target/debug/figtether" \
		"$REPO_ROOT/target/debug/figtether.exe" \
		"${CARGO_TARGET_DIR:-/nonexistent}/release/figtether" \
		"${CARGO_TARGET_DIR:-/nonexistent}/release/figtether.exe" \
		"${CARGO_TARGET_DIR:-/nonexistent}/debug/figtether" \
		"${CARGO_TARGET_DIR:-/nonexistent}/debug/figtether.exe"; do
		if [ -x "$candidate" ]; then
			BINARY="$candidate"
			break
		fi
	done
fi
if [ -z "$BINARY" ]; then
	if command -v figtether >/dev/null 2>&1; then
		BINARY="$(command -v figtether)"
	fi
fi
if [ -z "$BINARY" ]; then
	fail 'the figtether binary was not found, build it with "cargo build --release" or pass --binary'
fi
if [ ! -x "$BINARY" ]; then
	fail "the figtether binary is not executable: $BINARY"
fi

HEALTH_URL="http://127.0.0.1:$ASSESSMENT_PORT/health"

# Read the health endpoint. The body is the sanitized status of the daemon.
health_body() {
	curl --silent --max-time 3 --header "Host: 127.0.0.1:$ASSESSMENT_PORT" \
		"$HEALTH_URL" 2>/dev/null || true
}

# Whether the health endpoint answers for this daemon and this port.
health_ok() {
	body="$(health_body)"
	[ -n "$body" ] || return 1
	compact="$(printf '%s' "$body" | tr -d ' \t\r\n')"
	case "$compact" in
	*"\"port\":$ASSESSMENT_PORT"*) return 0 ;;
	esac
	return 1
}

log '==> assessment target'
log "port: $ASSESSMENT_PORT"
log "runtime directory: $RUNTIME_DIR"
log "binary: $BINARY"

if health_ok; then
	log "health: answered (GET $HEALTH_URL)"
	log 'note: a daemon already answers on the assessment port, no second daemon was started'
	printf '%s\n' "$ASSESSMENT_PORT" >"$RUNTIME_DIR/assessment-port"
	printf '%s\n' "$RUNTIME_DIR" >"$RUNTIME_DIR/assessment-runtime-dir"
	exit 0
fi

STDOUT_LOG="$RUNTIME_DIR/daemon.stdout.log"
STDERR_LOG="$RUNTIME_DIR/daemon.stderr.log"
PID_FILE="$RUNTIME_DIR/daemon.pid"

nohup "$BINARY" daemon --port "$ASSESSMENT_PORT" --runtime-dir "$RUNTIME_DIR" --log-level info \
	>"$STDOUT_LOG" 2>"$STDERR_LOG" &
DAEMON_PID=$!
printf '%s\n' "$DAEMON_PID" >"$PID_FILE"

waited=0
while [ "$waited" -lt "$WAIT_TIMEOUT" ]; do
	if health_ok; then
		break
	fi
	if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
		log "health: no answer (GET $HEALTH_URL)"
		log 'the daemon output follows'
		if [ -f "$STDOUT_LOG" ]; then
			sed -n '1,40p' "$STDOUT_LOG" >&2
		fi
		if [ -f "$STDERR_LOG" ]; then
			sed -n '1,40p' "$STDERR_LOG" >&2
		fi
		fail "the assessment target did not start on port $ASSESSMENT_PORT"
	fi
	waited=$((waited + 1))
	sleep 1
done

if ! health_ok; then
	fail "the health endpoint did not answer within ${WAIT_TIMEOUT}s on port $ASSESSMENT_PORT"
fi

log "daemon pid: $DAEMON_PID"
log "health: answered (GET $HEALTH_URL)"
printf '%s\n' "$ASSESSMENT_PORT" >"$RUNTIME_DIR/assessment-port"
printf '%s\n' "$RUNTIME_DIR" >"$RUNTIME_DIR/assessment-runtime-dir"
log 'the assessment target is ready'
