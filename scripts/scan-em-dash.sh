#!/bin/sh
# Scan the repository for em dashes.
#
# PRD 8.12 and the documentation governance rules forbid the em dash
# character. ASCII hyphens are used instead. The scan covers every text file
# in the tree, not only the documentation, so that the rule cannot be broken
# by accident in a source file or a workflow.
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

# A single recursive grep keeps the scan fast on large trees. Binary files
# are ignored, and the build and dependency directories are skipped.
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

if [ "$status" -ne 0 ]; then
	printf 'em dash scan: failed, replace the em dash with an ASCII hyphen\n' >&2
	exit 1
fi

printf 'em dash scan: no em dash found under %s\n' "$scan_path"
