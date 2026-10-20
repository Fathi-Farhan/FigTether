#!/bin/sh
# Prepare a local FigTether development environment.
#
# POSIX sh, no bashisms, no network downloads beyond what the package manager
# already does. The PowerShell twin with the same behaviour is
# scripts/bootstrap.ps1.
#
# Usage: sh scripts/bootstrap.sh

set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

log() {
	printf '%s\n' "$*"
}

log "==> repository root: $root"

log "==> rustup components (rustfmt, clippy)"
if command -v rustup >/dev/null 2>&1; then
	rustup component add rustfmt clippy
	rustup show
else
	log "rustup was not found on PATH, skipping the toolchain setup"
	log "install it from https://rustup.rs and rerun this script"
fi

log "==> documentation tooling (npm ci)"
if command -v npm >/dev/null 2>&1; then
	npm ci
else
	log "npm was not found on PATH, skipping the documentation tooling"
	log "install Node.js LTS from https://nodejs.org and rerun this script"
fi

log "==> optional release tooling"
log "the audit, SBOM, and release tasks need these tools:"
log "  cargo install cargo-deny cargo-audit cargo-cyclonedx"

log "bootstrap finished"
