#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
WASM_TARGET="wasm32-wasip1"
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

HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$HOST_TARGET" ]] || {
    printf '%s\n' 'FAIL: could not determine Rust host target' >&2
    exit 1
}

PLUGIN_ARGS=()
for plugin in "${PLUGINS[@]}"; do
    PLUGIN_ARGS+=(-p "$plugin")
done

printf '=== zellij-toolbox local qualification ===\n'

printf '\n[1/6] formatting\n'
cargo fmt --all -- --check

printf '\n[2/6] plugin check\n'
cargo check --locked --target "$WASM_TARGET" "${PLUGIN_ARGS[@]}"

printf '\n[3/6] native companion check\n'
cargo check --locked --target "$HOST_TARGET" -p zalertctl -p zmarkctl

printf '\n[4/6] plugin release build\n'
cargo build --locked --release --target "$WASM_TARGET" "${PLUGIN_ARGS[@]}"

printf '\n[5/6] native companion release build\n'
cargo build --locked --release --target "$HOST_TARGET" -p zalertctl -p zmarkctl

printf '\n[6/6] artifacts\n'
for plugin in "${PLUGINS[@]}"; do
    wasm="$ROOT/target/$WASM_TARGET/release/$plugin.wasm"
    [[ -s "$wasm" ]] || {
        printf 'FAIL: missing %s\n' "$wasm" >&2
        exit 1
    }
    ls -lh "$wasm"
done

for companion in \
    "$ROOT/target/$HOST_TARGET/release/zellij-toolbox-alert" \
    "$ROOT/target/$HOST_TARGET/release/zellij-toolbox-zmark"
do
    [[ -x "$companion" ]] || {
        printf 'FAIL: missing %s\n' "$companion" >&2
        exit 1
    }
    ls -lh "$companion"
done

printf '\nZELLIJ TOOLBOX QUALIFICATION PASS\n'
