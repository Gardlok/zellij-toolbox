# Architecture

Zellij Toolbox follows a few simple rules.

## Use Zellij first

If Zellij already has a good native action, the toolbox should expose it with a useful convention instead of rewriting it.

That is why:

- zback uses FocusLastPane
- zcopycmd uses CopyLastCommandOutput

## Use WASM plugins for session-local tools

Tools that need Zellij pane state, scrollback, or UI should normally be small Zellij WASM plugins.

zcopyall is the first example.

Future likely WASM plugins include:

- zgrep
- zmark
- zdiffpane
- zbroadcast
- zpaneinfo
- zcommandpalette

Shared Rust code should move into a common crate only when at least two real tools need it. The project will not create abstractions simply to have abstractions.

## Use a native companion only for machine-wide jobs

A Zellij plugin belongs to one Zellij session.

zalert is intended to watch work across multiple local Zellij sessions, so it may need a small native companion, zalertd, in addition to a session plugin.

The companion should remain optional. Users who only want the normal toolbox plugins should not need a daemon.

## Baseline

The current minimum supported Zellij version is 0.45.1.

This baseline gives the toolbox:

- full pane scrollback access
- direct clipboard access
- current client and pane information
- session list access
- FocusLastPane
- command-aware scrollback actions such as CopyLastCommandOutput
- newer plugin events that can support later tools such as zalert

The current minimum Rust version is 1.95 because Zellij 0.45.1 and its plugin crate require it.

## Key conventions

Initial defaults:

- Alt+A — all: zcopyall
- Alt+B — back: zback
- Alt+C — command: zcopycmd

Future bindings should stay memorable and avoid taking existing Zellij defaults when practical.
