#!/bin/sh
# Run the local quality gate for FigTether.
#
# Covers the same ground as the CI pipeline: Rust formatting, Rust lint,
# Rust tests, the documentation format check, and the em dash scan.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/check.ps1.
#
# Usage: sh scripts/check.sh

set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

step() {
	printf '\n==> %s\n' "$1"
	shift
	"$@"
}

step "cargo fmt --all --check" cargo fmt --all --check
step "cargo clippy --workspace --all-targets -- -D warnings" cargo clippy --workspace --all-targets -- -D warnings
step "cargo test --workspace" cargo test --workspace

if command -v npm >/dev/null 2>&1; then
	if [ ! -d node_modules ]; then
		step "npm ci" npm ci
	fi
	step "npm run format:check" npm run format:check
else
	printf '\n==> npm was not found on PATH, the documentation format check is skipped\n'
fi

step "em dash scan" sh scripts/scan-em-dash.sh

printf '\nall checks passed\n'
