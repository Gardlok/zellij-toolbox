#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TARGET="wasm32-wasip1"

cd "$ROOT"

printf '=== zellij-toolbox local qualification ===\n'

printf '\n[1/4] formatting\n'
cargo fmt --all -- --check

printf '\n[2/4] zcopyall check\n'
cargo check --package zcopyall --target "$TARGET"

printf '\n[3/4] zcopyall release build\n'
cargo build --release --package zcopyall --target "$TARGET"

WASM="$ROOT/target/$TARGET/release/zcopyall.wasm"

printf '\n[4/4] artifact\n'
[[ -s "$WASM" ]] || {
    printf 'FAIL: missing %s\n' "$WASM" >&2
    exit 1
}
ls -lh "$WASM"

printf '\nZELLIJ TOOLBOX QUALIFICATION PASS\n'
