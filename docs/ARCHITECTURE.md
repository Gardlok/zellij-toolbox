# Architecture

## Baseline

Zellij Toolbox currently targets Zellij 0.45.1 or newer.

The installed WASM plugins do not require Rust at runtime. Rust 1.95 is required by the current source-build workflow because the toolbox is compiled against zellij-tile 0.45.1.

A future release installer can use prebuilt WASM artifacts and remove Rust as an installation requirement.

## Prefer native Zellij actions

The toolbox does not reimplement features Zellij already provides well.

- zback uses FocusLastPane
- zcopycmd uses CopyLastCommandOutput

## Session-local plugins

The current WASM plugins are:

- zcopyall
- zpaneinfo
- zgrep
- zmark
- zdiffpane
- zbroadcast
- zalert
- zcommandpalette

They are separate so each utility can stay small and request only the permissions it needs.

Shared Rust code should be introduced only after multiple real plugins need the same behavior.

## zalert

The first zalert implementation is session-local but can send desktop notifications through notify-send. Alerts can therefore remain visible while the user works in another Zellij session.

A future optional native companion can add machine-wide watch management and direct jump/attach behavior back to the originating session and pane.

## Safety

zbroadcast does not blindly mirror keystrokes. It requires explicit pane selection, explicit command entry, and a second Enter confirmation before writing to any pane.

## Key conventions

- Alt+A — zcopyall
- Alt+B — zback
- Alt+C — zcopycmd
- Alt+D — zdiffpane
- Alt+G — zgrep
- Alt+M / Alt+Shift+M — add/list zmark
- Alt+V — zpaneinfo
- Alt+W / Alt+Shift+W — toggle/list zalert
- Alt+Shift+B — zbroadcast
- Alt+Space — zcommandpalette
