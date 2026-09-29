#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="${ZELLIJ_CONFIG_DIR:-$HOME/.config/zellij}"
PLUGIN_DIR="$CONFIG_DIR/plugins/zellij-toolbox"
ZCOPYALL_PATH="$PLUGIN_DIR/zcopyall.wasm"

MIN_ZELLIJ="0.45.1"
MIN_RUST="1.95.0"
WASM_TARGET="wasm32-wasip1"

die() {
    printf 'zellij-toolbox: %s\n' "$*" >&2
    exit 1
}

version_at_least() {
    local actual="$1"
    local minimum="$2"
    [[ "$(printf '%s\n%s\n' "$actual" "$minimum" | sort -V | head -n1)" == "$minimum" ]]
}

command -v zellij >/dev/null 2>&1 || die "zellij is required"
command -v rustc >/dev/null 2>&1 || die "rustc is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
command -v rustup >/dev/null 2>&1 || die "rustup is required"

ZELLIJ_VERSION="$(zellij --version | awk '{print $2}')"
RUST_VERSION="$(rustc --version | awk '{print $2}')"

version_at_least "$ZELLIJ_VERSION" "$MIN_ZELLIJ" ||
    die "Zellij $ZELLIJ_VERSION is too old; requires >= $MIN_ZELLIJ"

version_at_least "$RUST_VERSION" "$MIN_RUST" ||
    die "Rust $RUST_VERSION is too old; requires >= $MIN_RUST"

printf '==> Zellij %s\n' "$ZELLIJ_VERSION"
printf '==> Rust %s\n' "$RUST_VERSION"

printf '==> Ensuring Rust target %s\n' "$WASM_TARGET"
rustup target add "$WASM_TARGET"

printf '==> Building zcopyall\n'
cd "$ROOT"
cargo build --release --package zcopyall --target "$WASM_TARGET"

WASM="$ROOT/target/$WASM_TARGET/release/zcopyall.wasm"
[[ -s "$WASM" ]] || die "zcopyall build completed without producing $WASM"

printf '==> Installing zcopyall\n'
mkdir -p "$PLUGIN_DIR"
install -m 0644 "$WASM" "$ZCOPYALL_PATH"

printf '\nInstalled:\n  %s\n\n' "$ZCOPYALL_PATH"

printf '%s\n' 'First run, from inside Zellij:'
printf '  zellij action start-or-reload-plugin "file:%s"\n\n' "$ZCOPYALL_PATH"

cat <<EOF
Grant the three requested zcopyall permissions, then merge this into your existing keybinds block:

shared_except "locked" {
    bind "Alt a" {
        MessagePlugin "file:$ZCOPYALL_PATH" {
            name "copy_all"
        }
    }

    bind "Alt b" {
        FocusLastPane
    }

    bind "Alt c" {
        CopyLastCommandOutput
    }
}

Alt+A  copy all retained pane scrollback
Alt+B  jump back to the previous pane
Alt+C  copy the last command output

The installer does not edit config.kdl automatically.
EOF
