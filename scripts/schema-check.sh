#!/bin/sh
# Compare the generated JSON schemas with the committed ones.
#
# The generator is a Cargo example. The contract is:
#   cargo run -p <package> --example <name> -- <output-directory>
# The example writes one file per schema into the output directory and exits
# with a non zero status when the Rust model and the schemas disagree.
#
# The check fails when a committed schema differs from the generated one, when a
# committed schema is missing from the generated set, or when a generated
# schema is not committed yet.
#
# When the generator does not exist yet the check reports a skip and exits with
# zero, so the pipeline works before the schema work lands.
#
# POSIX sh, no bashisms. The PowerShell twin with the same behaviour is
# scripts/schema-check.ps1.
#
# Usage: sh scripts/schema-check.sh
# Environment:
#   FIGTETHER_SCHEMA_EXAMPLE     generator example name, default "export_schemas"
#   FIGTETHER_SCHEMA_DIR         committed schema directory, default "schemas"
#   FIGTETHER_SCHEMA_PACKAGE     workspace package that owns the example

set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

example="${FIGTETHER_SCHEMA_EXAMPLE:-export_schemas}"
schema_dir="${FIGTETHER_SCHEMA_DIR:-schemas}"
package="${FIGTETHER_SCHEMA_PACKAGE:-}"

found=""
for candidate in \
	"crates/model/examples/$example.rs" \
	"crates/protocol/examples/$example.rs" \
	"crates/cli/examples/$example.rs" \
	"crates/core/examples/$example.rs"; do
	if [ -f "$candidate" ]; then
		found="$candidate"
		break
	fi
done

if [ -z "$found" ]; then
	printf 'schema check: skipped, no schema generator example named %s was found\n' "$example"
	printf 'schema check: expected one of crates/{model,protocol,cli,core}/examples/%s.rs\n' "$example"
	exit 0
fi

printf 'schema check: generator %s\n' "$found"

# The workspace package is derived from the crate that owns the example, so the
# check keeps working when the example moves to another crate.
if [ -z "$package" ]; then
	crate="$(printf '%s' "$found" | cut -d/ -f2)"
	case "$crate" in
	figtether-*) package="$crate" ;;
	esac
fi

if [ ! -d "$schema_dir" ]; then
	printf 'schema check: failed, the committed schema directory %s does not exist\n' "$schema_dir" >&2
	exit 1
fi

tmp="$(mktemp -d)"
cleanup() {
	rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

if [ -n "$package" ]; then
	cargo run --package "$package" --example "$example" --locked -- "$tmp"
else
	cargo run --example "$example" --locked -- "$tmp"
fi

status=0

for file in $(cd "$schema_dir" && find . -type f -name '*.schema.json' | sort); do
	if [ ! -f "$tmp/$file" ]; then
		printf 'schema check: %s is committed but was not generated\n' "$file" >&2
		status=1
		continue
	fi
	if ! cmp -s "$schema_dir/$file" "$tmp/$file"; then
		printf 'schema check: %s differs from the generated schema\n' "$file" >&2
		status=1
	fi
done

for file in $(cd "$tmp" && find . -type f -name '*.schema.json' | sort); do
	if [ ! -f "$schema_dir/$file" ]; then
		printf 'schema check: %s was generated but is not committed\n' "$file" >&2
		status=1
	fi
done

if [ "$status" -ne 0 ]; then
	printf 'schema check: failed, regenerate the schemas and commit the result\n' >&2
	exit 1
fi

printf 'schema check: the committed schemas match the generated ones\n'
