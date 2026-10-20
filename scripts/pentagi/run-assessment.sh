#!/bin/sh
# Create one PentAGI flow per FigTether assessment prompt.
#
# The script reads every markdown file of the flow directory, extracts the
# target description of the file, and submits it to the PentAGI GraphQL API
# with the createFlow mutation. Afterwards it lists the flows of the
# instance and writes the identifiers of the created flows into a JSON
# summary file.
#
# The API token is read from the command line or from the environment. It is
# never printed, never written into the summary file, and never written into
# the command line of a child process: the token travels to curl through a
# private configuration file.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/pentagi/run-assessment.ps1.
#
# Usage: sh scripts/pentagi/run-assessment.sh --base-url <url> --token <token>
#                                              [--provider openai]
#                                              [--flow-dir <path>] [--out <file>]
#                                              [--timeout <seconds>] [--insecure]

set -eu

# The model provider used when the caller does not name one.
DEFAULT_PROVIDER=openai

# Seconds to wait for one API answer.
REQUEST_TIMEOUT=60

BASE_URL="${PENTAGI_BASE_URL:-}"
API_TOKEN="${PENTAGI_API_TOKEN:-}"
PROVIDER="$DEFAULT_PROVIDER"
FLOW_DIR=""
OUT_FILE=""
INSECURE=0

REPO_ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"

if [ -z "$FLOW_DIR" ]; then
	FLOW_DIR="$REPO_ROOT/security/pentagi/flows"
fi
if [ -z "$OUT_FILE" ]; then
	OUT_FILE="$REPO_ROOT/tmp/pentagi-assessment/flows.json"
fi

# The messages that the PowerShell twin prints as well. Keep both in sync.
TOKEN_MISSING='the PentAGI API token is missing: pass --token <token> or set PENTAGI_API_TOKEN'
URL_MISSING='the PentAGI base URL is missing: pass --base-url <url> or set PENTAGI_BASE_URL'

log() {
	printf '%s\n' "$*"
}

fail() {
	printf 'run-assessment: %s\n' "$*" >&2
	exit 1
}

fail_usage() {
	printf 'run-assessment: %s\n' "$*" >&2
	exit 2
}

usage() {
	log 'usage: sh scripts/pentagi/run-assessment.sh --base-url <url> --token <token>'
	log '                                              [--provider openai]'
	log '                                              [--flow-dir <path>] [--out <file>]'
	log '                                              [--timeout <seconds>] [--insecure]'
}

while [ "$#" -gt 0 ]; do
	case "$1" in
	--base-url)
		[ "$#" -ge 2 ] || fail_usage '--base-url needs a value'
		BASE_URL="$2"
		shift 2
		;;
	--token)
		[ "$#" -ge 2 ] || fail_usage '--token needs a value'
		API_TOKEN="$2"
		shift 2
		;;
	--provider)
		[ "$#" -ge 2 ] || fail_usage '--provider needs a value'
		PROVIDER="$2"
		shift 2
		;;
	--flow-dir)
		[ "$#" -ge 2 ] || fail_usage '--flow-dir needs a value'
		FLOW_DIR="$2"
		shift 2
		;;
	--out)
		[ "$#" -ge 2 ] || fail_usage '--out needs a value'
		OUT_FILE="$2"
		shift 2
		;;
	--timeout)
		[ "$#" -ge 2 ] || fail_usage '--timeout needs a value'
		REQUEST_TIMEOUT="$2"
		shift 2
		;;
	--insecure)
		INSECURE=1
		shift
		;;
	--help | -h)
		usage
		exit 0
		;;
	*)
		fail_usage "unknown argument: $1"
		;;
	esac
done

if [ -z "$API_TOKEN" ]; then
	fail_usage "$TOKEN_MISSING"
fi
if [ -z "$BASE_URL" ]; then
	fail_usage "$URL_MISSING"
fi

case "$BASE_URL" in
http://* | https://*) ;;
*) fail_usage "the PentAGI base URL must start with http:// or https://: $BASE_URL" ;;
esac
BASE_URL="${BASE_URL%/}"

case "$PROVIDER" in
'' | *[!A-Za-z0-9._-]*) fail_usage "the model provider name is not a plain name: $PROVIDER" ;;
esac

case "$REQUEST_TIMEOUT" in
'' | *[!0-9]*) fail_usage "the timeout must be a number: $REQUEST_TIMEOUT" ;;
esac

# The token travels to curl through a private configuration file, so it never
# shows up in the process list. The character set keeps the configuration
# file unambiguous.
case "$API_TOKEN" in
*[!A-Za-z0-9._~+/=-]*) fail_usage 'the PentAGI API token contains unsupported characters' ;;
esac

if ! command -v curl >/dev/null 2>&1; then
	fail 'curl is required to reach the PentAGI API'
fi
if [ ! -d "$FLOW_DIR" ]; then
	fail "the flow directory does not exist: $FLOW_DIR"
fi

OUT_DIR="$(dirname -- "$OUT_FILE")"
mkdir -p "$OUT_DIR"

umask 077
TMPDIR_ASSESSMENT="$(mktemp -d 2>/dev/null || true)"
if [ -z "$TMPDIR_ASSESSMENT" ]; then
	TMPDIR_ASSESSMENT="$(mktemp -d -t pentagi 2>/dev/null || printf '%s' "$OUT_DIR/pentagi-tmp.$$")"
fi
mkdir -p "$TMPDIR_ASSESSMENT"
trap 'rm -rf "$TMPDIR_ASSESSMENT"' 0 1 2 3 15

CURL_CONFIG="$TMPDIR_ASSESSMENT/curl.config"
{
	printf 'header = "Authorization: Bearer %s"\n' "$API_TOKEN"
} >"$CURL_CONFIG"

# Call the PentAGI API. The token stays in the private configuration file.
run_curl() {
	if [ "$INSECURE" -eq 1 ]; then
		curl --silent --show-error --max-time "$REQUEST_TIMEOUT" \
			--config "$CURL_CONFIG" --insecure "$@"
	else
		curl --silent --show-error --max-time "$REQUEST_TIMEOUT" \
			--config "$CURL_CONFIG" "$@"
	fi
}

# Escape a plain text file into the body of a JSON string.
json_body_from_file() {
	awk '
	BEGIN {
		BS = "\\"
		DQ = "\""
		body = ""
	}
	function esc(text,   out, i, c, n) {
		out = ""
		n = length(text)
		for (i = 1; i <= n; i++) {
			c = substr(text, i, 1)
			if (c == BS) {
				out = out BS BS
			} else if (c == DQ) {
				out = out BS DQ
			} else {
				out = out c
			}
		}
		return out
	}
	{
		if (NR > 1) {
			body = body BS "n"
		}
		body = body esc($0)
	}
	END {
		printf "%s", body
	}
	' "$1"
}

# Escape a plain string into the body of a JSON string.
json_body_from_string() {
	printf '%s' "$1" | awk '
	BEGIN {
		BS = "\\"
		DQ = "\""
	}
	function esc(text,   out, i, c, n) {
		out = ""
		n = length(text)
		for (i = 1; i <= n; i++) {
			c = substr(text, i, 1)
			if (c == BS) {
				out = out BS BS
			} else if (c == DQ) {
				out = out BS DQ
			} else {
				out = out c
			}
		}
		return out
	}
	{
		printf "%s", esc($0)
	}
	'
}

# Print the raw body of the first JSON string that carries the given key.
json_field() {
	awk -v key="$2" '
	{
		text = text $0 "\n"
	}
	END {
		needle = "\"" key "\":"
		place = index(text, needle)
		if (place == 0) {
			exit 0
		}
		i = place + length(needle)
		n = length(text)
		while (i <= n) {
			c = substr(text, i, 1)
			if (c == " " || c == "\t" || c == "\n" || c == "\r" || c == "," || c == "{") {
				i++
				continue
			}
			break
		}
		if (substr(text, i, 1) != "\"") {
			exit 0
		}
		i++
		body = ""
		while (i <= n) {
			c = substr(text, i, 1)
			if (c == "\\") {
				body = body c substr(text, i + 1, 1)
				i += 2
				continue
			}
			if (c == "\"") {
				break
			}
			body = body c
			i++
		}
		printf "%s", body
	}
	' "$1"
}

# Extract the target description of a flow file.
extract_target() {
	sed -n '/^<!-- BEGIN TARGET -->$/,/^<!-- END TARGET -->$/p' "$1" | sed '1d;$d'
}

# Print every identifier of a JSON document that carries an id field.
extract_ids() {
	awk '
	{
		rest = $0
		while (match(rest, /"id"[ \t]*:[ \t]*"?[A-Za-z0-9._:-]+"?/)) {
			piece = substr(rest, RSTART, RLENGTH)
			rest = substr(rest, RSTART + RLENGTH)
			gsub(/^"id"[ \t]*:[ \t]*"?/, "", piece)
			gsub(/"$/, "", piece)
			print piece
		}
	}
	' "$1"
}

# Print the file name and the flow identifier of every entry of a summary.
extract_summary_entries() {
	awk '
	{
		file = ""
		id = ""
		if (match($0, /"file"[ \t]*:[ \t]*"[^"]*"/)) {
			file = substr($0, RSTART, RLENGTH)
			sub(/^"file"[ \t]*:[ \t]*"/, "", file)
			sub(/"$/, "", file)
		}
		if (match($0, /"id"[ \t]*:[ \t]*"?[A-Za-z0-9._:-]+"?/)) {
			id = substr($0, RSTART, RLENGTH)
			sub(/^"id"[ \t]*:[ \t]*"?/, "", id)
			gsub(/"$/, "", id)
		}
		if (file != "" && id != "") {
			print file "\t" id
		}
	}
	' "$1"
}

GRAPHQL_QUERY='mutation CreateFlow($provider: String!, $input: String!) { createFlow(modelProvider: $provider, input: $input) { id title status } }'
GRAPHQL_FLOW_QUERY='query FlowTitle($flowId: ID!) { flow(flowId: $flowId) { id title status } }'

log '==> assessment'
log "base url: $BASE_URL"
log "model provider: $PROVIDER"
log "flow directory: $FLOW_DIR"
log "summary file: $OUT_FILE"

# The flow list of the instance, fetched before the creation round. It tells
# which flows of a previous run still exist.
LIST_FILE="$OUT_DIR/flows-api-list.json"
LIST_STATUS="$(run_curl \
	--output "$LIST_FILE" \
	--write-out '%{http_code}' \
	"$BASE_URL/api/v1/flows" 2>"$TMPDIR_ASSESSMENT/curl-list.err" || true)"
if [ "$LIST_STATUS" = "200" ]; then
	log "flow list: $LIST_FILE"
else
	log "flow list: the API answered with HTTP $LIST_STATUS"
fi

LIVE_IDS="$TMPDIR_ASSESSMENT/live-ids.txt"
: >"$LIVE_IDS"
if [ -f "$LIST_FILE" ]; then
	extract_ids "$LIST_FILE" >"$LIVE_IDS"
fi

# The flows that a previous run of the same summary file created.
PREVIOUS="$TMPDIR_ASSESSMENT/previous.tsv"
: >"$PREVIOUS"
if [ -f "$OUT_FILE" ]; then
	extract_summary_entries "$OUT_FILE" >"$PREVIOUS"
fi

ENTRIES="$TMPDIR_ASSESSMENT/entries.tsv"
: >"$ENTRIES"

created=0
reused=0
failed=0
index=0

for flow_file in "$FLOW_DIR"/*.md; do
	[ -f "$flow_file" ] || continue
	index=$((index + 1))
	name="$(basename -- "$flow_file")"

	TARGET_FILE="$TMPDIR_ASSESSMENT/target-$index.txt"
	extract_target "$flow_file" >"$TARGET_FILE"
	if [ ! -s "$TARGET_FILE" ]; then
		log "skipped $name: no target description between the target markers"
		failed=$((failed + 1))
		continue
	fi

	# A flow of a previous run is reused when it still exists, so that a
	# second run of the script does not start the same flow twice.
	previous_id="$(awk -F "$(printf '\t')" -v wanted="$name" '$1 == wanted { print $2; exit }' "$PREVIOUS")"
	if [ -n "$previous_id" ] && grep -qx "$previous_id" "$LIVE_IDS" 2>/dev/null; then
		TITLE_PAYLOAD="$TMPDIR_ASSESSMENT/title-$index.json"
		{
			printf '{"query":"%s","variables":{"flowId":"%s"}}' \
				"$GRAPHQL_FLOW_QUERY" "$(json_body_from_string "$previous_id")"
		} >"$TITLE_PAYLOAD"
		TITLE_RESPONSE="$TMPDIR_ASSESSMENT/title-response-$index.json"
		TITLE_STATUS="$(run_curl \
			--header 'Content-Type: application/json' \
			--data-binary "@$TITLE_PAYLOAD" \
			--output "$TITLE_RESPONSE" \
			--write-out '%{http_code}' \
			"$BASE_URL/api/v1/graphql" 2>/dev/null || true)"
		if [ "$TITLE_STATUS" = "200" ] && [ -n "$(json_field "$TITLE_RESPONSE" id)" ]; then
			FLOW_TITLE="$(json_field "$TITLE_RESPONSE" title)"
			FLOW_STATUS="$(json_field "$TITLE_RESPONSE" status)"
			[ -n "$FLOW_TITLE" ] || FLOW_TITLE="$name"
			printf '%s\t%s\t%s\t%s\n' "$name" "$FLOW_TITLE" "$previous_id" "$FLOW_STATUS" >>"$ENTRIES"
			log "reused $name -> flow $previous_id ($FLOW_STATUS)"
			reused=$((reused + 1))
			continue
		fi
	fi

	PAYLOAD="$TMPDIR_ASSESSMENT/payload-$index.json"
	{
		printf '{"query":"%s","variables":{"provider":"%s","input":"%s"}}' \
			"$GRAPHQL_QUERY" \
			"$(json_body_from_string "$PROVIDER")" \
			"$(json_body_from_file "$TARGET_FILE")"
	} >"$PAYLOAD"

	RESPONSE="$TMPDIR_ASSESSMENT/response-$index.json"
	STATUS="$(run_curl \
		--header 'Content-Type: application/json' \
		--data-binary "@$PAYLOAD" \
		--output "$RESPONSE" \
		--write-out '%{http_code}' \
		"$BASE_URL/api/v1/graphql" 2>"$TMPDIR_ASSESSMENT/curl-$index.err" || true)"

	if [ "$STATUS" != "200" ]; then
		log "failed $name: the API answered with HTTP $STATUS"
		if [ -s "$TMPDIR_ASSESSMENT/curl-$index.err" ]; then
			sed -n '1,5p' "$TMPDIR_ASSESSMENT/curl-$index.err" >&2
		fi
		if [ -s "$RESPONSE" ]; then
			sed -n '1,5p' "$RESPONSE" >&2
		fi
		failed=$((failed + 1))
		continue
	fi

	FLOW_ID="$(json_field "$RESPONSE" id)"
	if [ -z "$FLOW_ID" ]; then
		log "failed $name: the answer carries no flow identifier"
		sed -n '1,5p' "$RESPONSE" >&2
		failed=$((failed + 1))
		continue
	fi
	FLOW_TITLE="$(json_field "$RESPONSE" title)"
	FLOW_STATUS="$(json_field "$RESPONSE" status)"
	[ -n "$FLOW_TITLE" ] || FLOW_TITLE="$name"

	printf '%s\t%s\t%s\t%s\n' "$name" "$FLOW_TITLE" "$FLOW_ID" "$FLOW_STATUS" >>"$ENTRIES"
	log "created $name -> flow $FLOW_ID ($FLOW_STATUS)"
	created=$((created + 1))
done

# Refresh the flow list of the instance after the creation round.
LIST_STATUS="$(run_curl \
	--output "$LIST_FILE" \
	--write-out '%{http_code}' \
	"$BASE_URL/api/v1/flows" 2>"$TMPDIR_ASSESSMENT/curl-list.err" || true)"
if [ "$LIST_STATUS" = "200" ]; then
	log "flow list: $LIST_FILE"
else
	log "flow list: the API answered with HTTP $LIST_STATUS"
	failed=$((failed + 1))
fi

GENERATED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || printf 'unknown')"
TOTAL_ENTRIES="$(wc -l <"$ENTRIES" | tr -d ' \t')"

{
	printf '{\n'
	printf '  "generated_at": "%s",\n' "$GENERATED_AT"
	printf '  "base_url": "%s",\n' "$(json_body_from_string "$BASE_URL")"
	printf '  "model_provider": "%s",\n' "$(json_body_from_string "$PROVIDER")"
	printf '  "flow_directory": "%s",\n' "$(json_body_from_string "$FLOW_DIR")"
	printf '  "flows": [\n'
	awk -v total="$TOTAL_ENTRIES" '
	BEGIN {
		FS = "\t"
		BS = "\\"
		DQ = "\""
	}
	function esc(text,   out, i, c, n) {
		out = ""
		n = length(text)
		for (i = 1; i <= n; i++) {
			c = substr(text, i, 1)
			if (c == BS) {
				out = out BS BS
			} else if (c == DQ) {
				out = out BS DQ
			} else {
				out = out c
			}
		}
		return out
	}
	{
		printf "    { \"file\": \"%s\", \"title\": \"%s\", \"id\": \"%s\", \"status\": \"%s\" }", \
			esc($1), esc($2), esc($3), esc($4)
		if (FNR < total) {
			printf ","
		}
		printf "\n"
	}
	' "$ENTRIES"
	printf '  ]\n'
	printf '}\n'
} >"$OUT_FILE"

log "created flows: $created"
log "reused flows: $reused"
log "failed flows: $failed"
log "summary file: $OUT_FILE"

if [ "$created" -eq 0 ] && [ "$reused" -eq 0 ]; then
	fail 'no flow was created, see the messages above'
fi
