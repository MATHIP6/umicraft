#!/usr/bin/env bash
# Generates include/umicraft.h from umicraft-modapi-sys with cbindgen.
# Usage: tools/gen-header.sh [--check]
#   --check  generate to a temp file and fail (exit 1) if it differs from the
#            committed include/umicraft.h.
# cbindgen is MPL-2.0 and therefore an external tool, never a cargo dependency.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v cbindgen >/dev/null 2>&1; then
    echo "error: cbindgen not found. Install it with: cargo install cbindgen" >&2
    exit 2
fi

config=crates/umicraft-modapi-sys/cbindgen.toml
header=include/umicraft.h

generate() {
    cbindgen --config "$config" --crate umicraft-modapi-sys --output "$1"
}

case "${1:-}" in
    "")
        mkdir -p include
        generate "$header"
        ;;
    --check)
        tmp="$(mktemp)"
        trap 'rm -f "$tmp"' EXIT
        generate "$tmp"
        if ! diff -u "$header" "$tmp"; then
            echo "error: $header is out of date; run tools/gen-header.sh" >&2
            exit 1
        fi
        ;;
    *)
        echo "usage: $0 [--check]" >&2
        exit 2
        ;;
esac
