#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TARGET="wasm32-wasip1"
PLUGINS=(
    zcopyall
    zpaneinfo
    zgrep
    zmark
    zdiffpane
    zbroadcast
    zalert
    zcommandpalette
)

cd "$ROOT"

printf '=== zellij-toolbox local qualification ===\n'

printf '\n[1/4] formatting\n'
cargo fmt --all -- --check

printf '\n[2/4] workspace check\n'
cargo check --locked --workspace --target "$TARGET"

printf '\n[3/4] release build\n'
cargo build --locked --release --workspace --target "$TARGET"

printf '\n[4/4] artifacts\n'
for plugin in "${PLUGINS[@]}"; do
    wasm="$ROOT/target/$TARGET/release/$plugin.wasm"
    [[ -s "$wasm" ]] || {
        printf 'FAIL: missing %s\n' "$wasm" >&2
        exit 1
    }
    ls -lh "$wasm"
done

printf '\nZELLIJ TOOLBOX QUALIFICATION PASS\n'
