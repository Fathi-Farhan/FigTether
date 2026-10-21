#!/bin/sh
# Scan the repository for em dashes.
#
# PRD 8.12 and the documentation governance rules forbid the em dash
# character. ASCII hyphens are used instead. The scan covers every text file
# that the repository publishes, not only the documentation, so that the rule
# cannot be broken by accident in a source file or a workflow.
#
# The file set comes from Git: every tracked file plus every untracked file
# that is not ignored. Local scratch space, such as a run directory or a
# browser profile dump, is excluded because Git excludes it. When Git is not
# available, the script falls back to a recursive walk of the tree.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/scan-em-dash.ps1.
#
# Usage: sh scripts/scan-em-dash.sh [path]
#        the path defaults to the repository root

set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

scan_path="${1:-.}"

# The em dash is U+2014, encoded as E2 80 94 in UTF-8. The byte sequence is
# used so the scan does not depend on the locale of the shell.
em_dash="$(printf '\342\200\224')"

status=0

# The published file set: tracked files, plus new files that are not ignored.
file_list="$(git ls-files --cached --others --exclude-standard -- "$scan_path" 2>/dev/null || true)"

if [ -n "$file_list" ]; then
	# The published files are scanned in small chunks, so that a long file list
	# does not overflow the argument limit. The output decides the result, not
	# the exit code of the chunk, because a chunk without a match exits with 1.
	matches="$(printf '%s\n' "$file_list" | tr '\n' '\0' | xargs -0 -n 64 grep -I -n -- "$em_dash" 2>/dev/null || true)"
	if [ -n "$matches" ]; then
		printf '%s\n' "$matches"
		status=1
	fi
else
	# Git could not answer, so the tree is walked instead. The build and
	# dependency directories are skipped.
	if grep -r -I -n \
		--exclude-dir=node_modules \
		--exclude-dir=target \
		--exclude-dir=.git \
		--exclude-dir=artifacts \
		--exclude-dir=dist \
		--exclude-dir=coverage \
		-- "$em_dash" "$scan_path"; then
		status=1
	fi
fi

if [ "$status" -ne 0 ]; then
	printf 'em dash scan: failed, replace the em dash with an ASCII hyphen\n' >&2
	exit 1
fi

printf 'em dash scan: no em dash found under %s\n' "$scan_path"
