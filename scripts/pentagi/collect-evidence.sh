#!/bin/sh
# Collect the evidence of a PentAGI assessment.
#
# The script writes one directory per run under security/pentagi/evidence and
# fills it with
#
#   - the raw GraphQL payload of every flow of the assessment (flow-<id>.json),
#   - a readable report of every flow (flow-<id>.md),
#   - the diagnostic log of the FigTether assessment target (daemon.log),
#   - the manifest of the run (manifest.md and manifest.json).
#
# The manifest states that the collected agent output is unauthenticated and
# that it does not count as a finding before a human reviewed it.
#
# The API token is read from the command line or from the environment. It is
# never printed and never written into the evidence.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/pentagi/collect-evidence.ps1.
#
# Usage: sh scripts/pentagi/collect-evidence.sh --base-url <url> --token <token>
#                                                 [--flows-file <file>]
#                                                 [--runtime-dir <dir>]
#                                                 [--out-dir <dir>]
#                                                 [--timeout <seconds>] [--insecure]

set -eu

# Seconds to wait for one API answer.
REQUEST_TIMEOUT=60

BASE_URL="${PENTAGI_BASE_URL:-}"
API_TOKEN="${PENTAGI_API_TOKEN:-}"
FLOWS_FILE=""
RUNTIME_DIR=""
OUT_DIR=""
INSECURE=0

REPO_ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"

if [ -z "$FLOWS_FILE" ]; then
	FLOWS_FILE="$REPO_ROOT/tmp/pentagi-assessment/flows.json"
fi
if [ -z "$RUNTIME_DIR" ]; then
	RUNTIME_DIR="$REPO_ROOT/tmp/pentagi-assessment/runtime"
fi
if [ -z "$OUT_DIR" ]; then
	OUT_DIR="$REPO_ROOT/security/pentagi/evidence"
fi

# The messages that the sibling scripts print as well. Keep them in sync.
TOKEN_MISSING='the PentAGI API token is missing: pass --token <token> or set PENTAGI_API_TOKEN'
URL_MISSING='the PentAGI base URL is missing: pass --base-url <url> or set PENTAGI_BASE_URL'

log() {
	printf '%s\n' "$*"
}

fail() {
	printf 'collect-evidence: %s\n' "$*" >&2
	exit 1
}

fail_usage() {
	printf 'collect-evidence: %s\n' "$*" >&2
	exit 2
}

usage() {
	log 'usage: sh scripts/pentagi/collect-evidence.sh --base-url <url> --token <token>'
	log '                                                [--flows-file <file>] [--runtime-dir <dir>]'
	log '                                                [--out-dir <dir>] [--timeout <seconds>] [--insecure]'
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
	--flows-file)
		[ "$#" -ge 2 ] || fail_usage '--flows-file needs a value'
		FLOWS_FILE="$2"
		shift 2
		;;
	--runtime-dir)
		[ "$#" -ge 2 ] || fail_usage '--runtime-dir needs a value'
		RUNTIME_DIR="$2"
		shift 2
		;;
	--out-dir)
		[ "$#" -ge 2 ] || fail_usage '--out-dir needs a value'
		OUT_DIR="$2"
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

case "$REQUEST_TIMEOUT" in
'' | *[!0-9]*) fail_usage "the timeout must be a number: $REQUEST_TIMEOUT" ;;
esac

case "$API_TOKEN" in
*[!A-Za-z0-9._~+/=-]*) fail_usage 'the PentAGI API token contains unsupported characters' ;;
esac

if ! command -v curl >/dev/null 2>&1; then
	fail 'curl is required to reach the PentAGI API'
fi

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

run_curl() {
	if [ "$INSECURE" -eq 1 ]; then
		curl --silent --show-error --max-time "$REQUEST_TIMEOUT" \
			--config "$CURL_CONFIG" --insecure "$@"
	else
		curl --silent --show-error --max-time "$REQUEST_TIMEOUT" \
			--config "$CURL_CONFIG" "$@"
	fi
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

# The digest of a file, or an empty string when no digest tool is available.
file_digest() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d ' ' -f 1
		return 0
	fi
	if command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
		return 0
	fi
	printf ''
}

file_size() {
	wc -c <"$1" | tr -d ' \t'
}

# The flattener turns a JSON document into one record per line. A record is
# the path of the value and the raw body of the value, separated by a tab.
cat >"$TMPDIR_ASSESSMENT/flatten.awk" <<'FLATTEN_AWK'
{ doc = doc $0 "\n" }
END {
	pos = 1
	total = length(doc)
	skip_spaces()
	parse_value("")
}
function skip_spaces(   c) {
	while (pos <= total) {
		c = substr(doc, pos, 1)
		if (c == " " || c == "\t" || c == "\n" || c == "\r") {
			pos++
			continue
		}
		break
	}
}
function emit(path, value) {
	printf "%s\t%s\n", path, value
}
function read_string(   c, out) {
	pos++
	out = ""
	while (pos <= total) {
		c = substr(doc, pos, 1)
		if (c == "\\") {
			out = out c substr(doc, pos + 1, 1)
			pos += 2
			continue
		}
		if (c == "\"") {
			pos++
			break
		}
		out = out c
		pos++
	}
	return out
}
function parse_value(path,   c, start) {
	skip_spaces()
	c = substr(doc, pos, 1)
	if (c == "{") {
		parse_object(path)
		return
	}
	if (c == "[") {
		parse_array(path)
		return
	}
	if (c == "\"") {
		emit(path, read_string())
		return
	}
	start = pos
	while (pos <= total) {
		c = substr(doc, pos, 1)
		if (c == "," || c == "}" || c == "]" || c == " " || c == "\t" || c == "\n" || c == "\r") {
			break
		}
		pos++
	}
	emit(path, substr(doc, start, pos - start))
}
function parse_object(path,   key, c) {
	pos++
	skip_spaces()
	if (substr(doc, pos, 1) == "}") {
		pos++
		return
	}
	while (pos <= total) {
		skip_spaces()
		key = read_string()
		skip_spaces()
		if (substr(doc, pos, 1) == ":") {
			pos++
		}
		if (path == "") {
			parse_value(key)
		} else {
			parse_value(path "." key)
		}
		skip_spaces()
		c = substr(doc, pos, 1)
		if (c == ",") {
			pos++
			continue
		}
		if (c == "}") {
			pos++
		}
		break
	}
}
function parse_array(path,   counter, c) {
	pos++
	skip_spaces()
	if (substr(doc, pos, 1) == "]") {
		pos++
		return
	}
	counter = 0
	while (pos <= total) {
		parse_value(path "[" counter "]")
		counter++
		skip_spaces()
		c = substr(doc, pos, 1)
		if (c == ",") {
			pos++
			continue
		}
		if (c == "]") {
			pos++
		}
		break
	}
}
FLATTEN_AWK

# The renderer turns the flat records into the readable flow report.
cat >"$TMPDIR_ASSESSMENT/render.awk" <<'RENDER_AWK'
BEGIN {
	FS = "\t"
	flow_id = ""
	flow_title = ""
	flow_status = ""
	have_task = 0
	task_count = 0
	subtask_count = 0
}
function hex_value(digits,   i, c, v, total) {
	total = 0
	for (i = 1; i <= length(digits); i++) {
		c = tolower(substr(digits, i, 1))
		v = index("0123456789abcdef", c) - 1
		if (v < 0) {
			return -1
		}
		total = total * 16 + v
	}
	return total
}
function unescape(text,   out, i, c, e, n, code, value) {
	out = ""
	i = 1
	n = length(text)
	while (i <= n) {
		c = substr(text, i, 1)
		if (c != "\\") {
			out = out c
			i++
			continue
		}
		i++
		e = substr(text, i, 1)
		if (e == "n") {
			out = out "\n"
			i++
		} else if (e == "t") {
			out = out "\t"
			i++
		} else if (e == "r") {
			out = out "\r"
			i++
		} else if (e == "b") {
			out = out "\b"
			i++
		} else if (e == "f") {
			out = out "\f"
			i++
		} else if (e == "u") {
			code = substr(text, i + 1, 4)
			value = hex_value(code)
			if (value >= 0 && value < 128) {
				out = out sprintf("%c", value)
			} else {
				out = out "\\u" code
			}
			i += 5
		} else {
			out = out e
			i++
		}
	}
	return out
}
function emoji(status) {
	if (status == "finished") return "OK"
	if (status == "failed") return "FAILED"
	if (status == "running") return "RUNNING"
	if (status == "waiting") return "WAITING"
	return "CREATED"
}
function bracket_index(text,   open_pos, close_pos) {
	open_pos = index(text, "[")
	close_pos = index(text, "]")
	if (open_pos == 0 || close_pos == 0) return ""
	return substr(text, open_pos + 1, close_pos - open_pos - 1)
}
function flush_task(   i) {
	if (!have_task) {
		return
	}
	body = body sprintf("### %s %s. %s\n\n", emoji(task_status), task_id, unescape(task_title))
	if (task_input != "") {
		body = body unescape(task_input) "\n\n"
	}
	if (task_result != "") {
		body = body "---\n\n" unescape(task_result) "\n\n"
	}
	for (i = 1; i <= subtask_count; i++) {
		body = body sprintf("#### %s %s. %s\n\n", emoji(sub_status[i]), sub_id[i], unescape(sub_title[i]))
		if (sub_description[i] != "") {
			body = body unescape(sub_description[i]) "\n\n"
		}
		if (sub_result[i] != "") {
			body = body "---\n\n" unescape(sub_result[i]) "\n\n"
		}
	}
	body = body "---\n\n"
	have_task = 0
	task_count = 0
	subtask_count = 0
}
{
	path = $1
	value = $2
	if (path == "data.flow.id") {
		flow_id = value
		next
	}
	if (path == "data.flow.title") {
		flow_title = value
		next
	}
	if (path == "data.flow.status") {
		flow_status = value
		next
	}
	if (path ~ /^data\.tasks\[[0-9]+\]\./) {
		split(path, segment, "\\.")
		task_number = bracket_index(segment[2])
		if (segment[3] == "subtasks" || segment[3] ~ /^subtasks/) {
			subtask_number = bracket_index(segment[3])
			field = segment[4]
			if (field == "id") {
				subtask_count++
				sub_id[subtask_count] = value
				sub_title[subtask_count] = ""
				sub_status[subtask_count] = ""
				sub_description[subtask_count] = ""
				sub_result[subtask_count] = ""
			} else if (field == "title") {
				sub_title[subtask_count] = value
			} else if (field == "status") {
				sub_status[subtask_count] = value
			} else if (field == "description") {
				sub_description[subtask_count] = value
			} else if (field == "result") {
				sub_result[subtask_count] = value
			}
			next
		}
		field = segment[3]
		if (field == "id") {
			flush_task()
			have_task = 1
			task_count++
			task_id = value
			task_title = ""
			task_status = ""
			task_input = ""
			task_result = ""
		} else if (field == "title") {
			task_title = value
		} else if (field == "status") {
			task_status = value
		} else if (field == "input") {
			task_input = value
		} else if (field == "result") {
			task_result = value
		}
		next
	}
}
END {
	flush_task()
	if (flow_title == "") {
		printf "No flow report was found for this flow.\n"
		exit 0
	}
	printf "# %s %s. %s\n\n", emoji(flow_status), flow_id, unescape(flow_title)
	printf "status: %s\n\n", flow_status
	printf "%s", body
}
RENDER_AWK

# The timestamp of the evidence directory.
TIMESTAMP="$(date -u '+%Y%m%dT%H%M%SZ' 2>/dev/null || printf 'unknown')"
EVIDENCE_DIR="$OUT_DIR/$TIMESTAMP"
mkdir -p "$EVIDENCE_DIR"

log '==> evidence'
log "base url: $BASE_URL"
log "evidence directory: $EVIDENCE_DIR"

# The flows of the assessment come from the summary file of the run. When
# that file is missing, every flow of the instance is collected.
FLOW_IDS="$TMPDIR_ASSESSMENT/flow-ids.txt"
: >"$FLOW_IDS"
if [ -f "$FLOWS_FILE" ]; then
	extract_ids "$FLOWS_FILE" >"$FLOW_IDS"
	log "flows file: $FLOWS_FILE"
else
	log "flows file: $FLOWS_FILE was not found, every flow of the instance is collected"
fi

LIST_FILE="$EVIDENCE_DIR/flows-api-list.json"
LIST_STATUS="$(run_curl \
	--output "$LIST_FILE" \
	--write-out '%{http_code}' \
	"$BASE_URL/api/v1/flows" 2>"$TMPDIR_ASSESSMENT/curl-list.err" || true)"
if [ "$LIST_STATUS" = "200" ]; then
	log "flow list: $LIST_FILE"
else
	log "flow list: the API answered with HTTP $LIST_STATUS"
fi
if [ ! -s "$FLOW_IDS" ]; then
	extract_ids "$LIST_FILE" >"$FLOW_IDS" 2>/dev/null || true
fi

GRAPHQL_QUERY='query FlowReport($flowId: ID!) { flow(flowId: $flowId) { id title status createdAt updatedAt } tasks(flowId: $flowId) { id title status input result subtasks { id title status description result } } }'

collected=0
while IFS= read -r flow_id; do
	[ -n "$flow_id" ] || continue
	PAYLOAD="$TMPDIR_ASSESSMENT/payload.json"
	{
		printf '{"query":"%s","variables":{"flowId":"%s"}}' \
			"$GRAPHQL_QUERY" "$(json_body_from_string "$flow_id")"
	} >"$PAYLOAD"

	RESPONSE="$EVIDENCE_DIR/flow-$flow_id.json"
	STATUS="$(run_curl \
		--header 'Content-Type: application/json' \
		--data-binary "@$PAYLOAD" \
		--output "$RESPONSE" \
		--write-out '%{http_code}' \
		"$BASE_URL/api/v1/graphql" 2>"$TMPDIR_ASSESSMENT/curl-flow.err" || true)"

	if [ "$STATUS" != "200" ]; then
		log "flow $flow_id: the API answered with HTTP $STATUS, the report is missing"
		rm -f "$RESPONSE"
		continue
	fi

	awk -f "$TMPDIR_ASSESSMENT/flatten.awk" "$RESPONSE" >"$TMPDIR_ASSESSMENT/flat-$flow_id.tsv"
	awk -f "$TMPDIR_ASSESSMENT/render.awk" "$TMPDIR_ASSESSMENT/flat-$flow_id.tsv" \
		>"$EVIDENCE_DIR/flow-$flow_id.md"
	log "flow $flow_id: $EVIDENCE_DIR/flow-$flow_id.md"
	collected=$((collected + 1))
done <"$FLOW_IDS"

# The diagnostic log of the assessment target.
if [ -d "$RUNTIME_DIR" ]; then
	for log_name in figtether.log figtether.log.1 daemon.stdout.log daemon.stderr.log; do
		if [ -f "$RUNTIME_DIR/$log_name" ]; then
			cp "$RUNTIME_DIR/$log_name" "$EVIDENCE_DIR/$log_name"
			log "log: $EVIDENCE_DIR/$log_name"
		fi
	done
else
	log "runtime directory: $RUNTIME_DIR was not found, the daemon log is missing"
fi

COLLECTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || printf 'unknown')"

# The file list of the manifest.
FILE_LIST="$TMPDIR_ASSESSMENT/files.tsv"
: >"$FILE_LIST"
for evidence_file in "$EVIDENCE_DIR"/*; do
	[ -f "$evidence_file" ] || continue
	name="$(basename -- "$evidence_file")"
	case "$name" in
	manifest.md | manifest.json) continue ;;
	esac
	printf '%s\t%s\t%s\n' "$name" "$(file_size "$evidence_file")" "$(file_digest "$evidence_file")" >>"$FILE_LIST"
done

REVIEW_STATEMENT='This evidence is unauthenticated agent output. A PentAGI flow is written by a language model that ran tools on its own, and the collected text is not a statement of the FigTether maintainers. Nothing in this directory counts as a finding before a human reviewer has read the report, replayed the described requests against the assessment target, and confirmed that the observed answers match the behaviour of the daemon. A claim of a vulnerability is not a vulnerability until the reproduction is replayed by hand.'

{
	printf '# PentAGI assessment evidence\n\n'
	printf 'Collected at: %s\n\n' "$COLLECTED_AT"
	printf 'PentAGI base URL: %s\n\n' "$BASE_URL"
	printf 'Assessment target: the FigTether daemon on 127.0.0.1, assessment port only\n\n'
	printf 'Daemon runtime directory: %s\n\n' "$RUNTIME_DIR"
	printf 'Flows collected: %s\n\n' "$collected"
	printf '## Collected files\n\n'
	printf '| File | Bytes | SHA-256 |\n'
	printf '| ---- | ----- | ------- |\n'
	while IFS="$(printf '\t')" read -r name size digest; do
		[ -n "$name" ] || continue
		printf '| %s | %s | %s |\n' "$name" "$size" "${digest:-not available}"
	done <"$FILE_LIST"
	printf '\n## How to read this evidence\n\n'
	printf '1. `flow-<id>.md` is the report of one flow. It holds the task results and the subtask results that the agent wrote.\n'
	printf '2. `flow-<id>.json` is the raw API payload of the same flow. Use it when a line of the report looks truncated.\n'
	printf '3. `figtether.log` is the diagnostic log of the assessment target. It carries error codes, versions, and counters, never design content.\n'
	printf '4. `flows-api-list.json` is the flow list of the PentAGI instance after the assessment.\n\n'
	printf '## Review status\n\n'
	printf '%s\n\n' "$REVIEW_STATEMENT"
} >"$EVIDENCE_DIR/manifest.md"

{
	printf '{\n'
	printf '  "collected_at": "%s",\n' "$COLLECTED_AT"
	printf '  "base_url": "%s",\n' "$(json_body_from_string "$BASE_URL")"
	printf '  "runtime_directory": "%s",\n' "$(json_body_from_string "$RUNTIME_DIR")"
	printf '  "flows_collected": %s,\n' "$collected"
	printf '  "review_statement": "%s",\n' "$(json_body_from_string "$REVIEW_STATEMENT")"
	printf '  "files": [\n'
	first=1
	while IFS="$(printf '\t')" read -r name size digest; do
		[ -n "$name" ] || continue
		if [ "$first" -eq 0 ]; then
			printf ',\n'
		fi
		first=0
		printf '    { "name": "%s", "bytes": %s, "sha256": "%s" }' \
			"$(json_body_from_string "$name")" "$size" "$digest"
	done <"$FILE_LIST"
	printf '\n  ]\n'
	printf '}\n'
} >"$EVIDENCE_DIR/manifest.json"

log "manifest: $EVIDENCE_DIR/manifest.md"
log "collected flows: $collected"
log 'the evidence is unauthenticated agent output and needs a human review'
