# FigTether task runner.
#
# Install `just` from https://github.com/casey/just and run `just --list`.
#
# Every recipe below is a single portable command so the file behaves the same
# on Linux, macOS, and Windows. The recipes use `sh`, which is the POSIX shell
# that ships with Git for Windows; on Windows, run the tasks from Git Bash (or
# add the Git `bin` directory to `PATH`). The PowerShell equivalents of every
# task live in `scripts/` and are listed in `CONTRIBUTING.md`.
#
# Tooling that must be installed once for the optional tasks:
#   cargo install cargo-deny cargo-audit cargo-cyclonedx
#
# The plugin recipes need `plugins/figma/` to exist. Until the plugin
# workspace lands, `plugin-build` and `plugin-test` fail with a clear error.

set shell := ["sh", "-cu"]

# List the available tasks
default:
    @just --list

# Build the workspace in release mode
build:
    cargo build --workspace --release --locked

# Run the workspace test suite
test:
    cargo test --workspace --locked

# Lint the workspace with clippy, warnings are denied
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format the Rust sources
fmt:
    cargo fmt --all

# Check the Rust formatting
fmt-check:
    cargo fmt --all --check

# Generate the JSON schemas into the schemas directory
schema:
    cargo run -p figtether-model --example export_schemas --locked -- schemas

# Compare the generated schemas with the committed ones
schema-check:
    sh scripts/schema-check.sh

# Run the benchmark suite
bench:
    cargo bench --workspace --locked

# Build the Figma plugin bundle
plugin-build:
    npm --prefix plugins/figma run build

# Typecheck the Figma plugin
plugin-typecheck:
    npm --prefix plugins/figma run typecheck

# Lint the Figma plugin
plugin-lint:
    npm --prefix plugins/figma run lint

# Test the Figma plugin
plugin-test:
    npm --prefix plugins/figma test

# Check the documentation formatting
docs-check:
    npm run format:check

# Scan the documentation for em dashes
docs-scan:
    sh scripts/scan-em-dash.sh

# Audit the dependency tree, licenses, advisories, and sources
audit:
    cargo deny check

# Audit the dependency tree for known vulnerabilities
audit-advisories:
    cargo audit

# Generate the CycloneDX SBOM
sbom:
    cargo cyclonedx --all-features --format json

# Run the end to end harness
e2e:
    cargo test -p figtether-e2e --locked

# Install the toolchain components and the documentation tooling
bootstrap:
    sh scripts/bootstrap.sh

# Run the full local check: format, lint, test, and documentation format
check:
    sh scripts/check.sh

# Print the release evidence template for the current commit
release-evidence:
    sh scripts/release-evidence.sh
