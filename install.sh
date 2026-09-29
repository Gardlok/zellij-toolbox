#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="${ZELLIJ_CONFIG_DIR:-$HOME/.config/zellij}"
PLUGIN_DIR="$CONFIG_DIR/plugins/zellij-toolbox"
MIN_ZELLIJ="0.45.1"
MIN_RUST="1.95.0"
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
command -v rustc >/dev/null 2>&1 || die "rustc is required for source installation"
command -v cargo >/dev/null 2>&1 || die "cargo is required for source installation"
command -v rustup >/dev/null 2>&1 || die "rustup is required for source installation"

ZELLIJ_VERSION="$(zellij --version | awk '{print $2}')"
RUST_VERSION="$(rustc --version | awk '{print $2}')"

version_at_least "$ZELLIJ_VERSION" "$MIN_ZELLIJ" ||
    die "Zellij $ZELLIJ_VERSION is too old; requires >= $MIN_ZELLIJ"

version_at_least "$RUST_VERSION" "$MIN_RUST" ||
    die "Rust $RUST_VERSION is too old for source build; requires >= $MIN_RUST"

printf '==> Zellij %s\n' "$ZELLIJ_VERSION"
printf '==> Rust %s (source build only)\n' "$RUST_VERSION"

printf '==> Ensuring Rust target %s\n' "$WASM_TARGET"
rustup target add "$WASM_TARGET"

printf '==> Building toolbox plugins\n'
cd "$ROOT"
cargo build --release --workspace --target "$WASM_TARGET"

printf '==> Installing toolbox plugins\n'
mkdir -p "$PLUGIN_DIR"
for plugin in "${PLUGINS[@]}"; do
    wasm="$ROOT/target/$WASM_TARGET/release/$plugin.wasm"
    [[ -s "$wasm" ]] || die "missing build artifact: $wasm"
    install -m 0644 "$wasm" "$PLUGIN_DIR/$plugin.wasm"
    printf '    %s\n' "$PLUGIN_DIR/$plugin.wasm"
done

cat <<EOF

Installed Zellij Toolbox plugins.

Merge this section into your existing keybinds block.
If you use keybinds clear-defaults=true, put it inside that block.

shared_except "locked" {
    bind "Alt a" {
        MessagePlugin "file:$PLUGIN_DIR/zcopyall.wasm" { name "copy_all" }
    }

    bind "Alt b" { FocusLastPane }
    bind "Alt c" { CopyLastCommandOutput }

    bind "Alt d" {
        MessagePlugin "file:$PLUGIN_DIR/zdiffpane.wasm" { name "open" }
    }

    bind "Alt g" {
        MessagePlugin "file:$PLUGIN_DIR/zgrep.wasm" { name "open" }
    }

    bind "Alt m" {
        MessagePlugin "file:$PLUGIN_DIR/zmark.wasm" { name "mark" }
    }
    bind "Alt Shift m" {
        MessagePlugin "file:$PLUGIN_DIR/zmark.wasm" { name "open" }
    }

    bind "Alt v" {
        MessagePlugin "file:$PLUGIN_DIR/zpaneinfo.wasm" { name "open" }
    }

    bind "Alt w" {
        MessagePlugin "file:$PLUGIN_DIR/zalert.wasm" { name "watch" }
    }
    bind "Alt Shift w" {
        MessagePlugin "file:$PLUGIN_DIR/zalert.wasm" { name "open" }
    }

    bind "Alt Shift b" {
        MessagePlugin "file:$PLUGIN_DIR/zbroadcast.wasm" { name "open" }
    }

    bind "Alt Space" {
        MessagePlugin "file:$PLUGIN_DIR/zcommandpalette.wasm" { name "open" }
    }
}

The first use of each plugin may trigger a Zellij permission prompt.

Suggested keys:
  Alt+A          copy all scrollback
  Alt+B          previous pane
  Alt+C          last command output
  Alt+D          diff two panes
  Alt+G          search pane history
  Alt+M          add mark
  Alt+Shift+M    list marks
  Alt+V          pane info
  Alt+W          toggle alert
  Alt+Shift+W    list alerts
  Alt+Shift+B    broadcast command
  Alt+Space      command palette

The installer does not edit config.kdl automatically.
EOF
