# Roadmap

## Qualified baseline

The first toolbox batch is qualified and installed on Midas against Zellij 0.45.1.

Validated on Midas:

- **zcopyall** — copy all retained scrollback
- **zback** — native FocusLastPane binding
- **zcopycmd** — native CopyLastCommandOutput binding with Bash OSC 133 integration
- **zpaneinfo** — focused pane diagnostics
- **zgrep** — session-wide retained-scrollback search
- **zmark** — session-local scrollback bookmarks
- **zdiffpane** — interactive two-pane output diff
- **zbroadcast** — confirmed command broadcast to selected panes
- **zalert** — one-shot foreground-command alerts
- **zcommandpalette** — common launcher

The workspace uses a committed Cargo.lock and locked qualification/source builds.

Orion remains a second-machine qualification target; it is not a blocker for continued Midas development.

## Qualified usability batch 2

Qualified and runtime-verified on Midas without changing the existing dependency set:

- **zcommandpalette** — type-to-filter search and shortcut display
- **zgrep** — all/current-tab/focused-pane scope plus case-sensitivity toggle
- **zmark** — optional names for session bookmarks
- **zalert** — arm at a shell prompt, detect the next foreground command, then alert when it finishes or changes

## Current development: cross-session zalert companion

The first native companion milestone keeps zalert detection in the WASM plugin while adding a dependency-free host helper:

- publish active watch state across local Zellij sessions;
- list all active watches from one CLI;
- jump from the current Zellij client to the originating session and pane;
- prune dead-session state and clear stale state when a session's zalert plugin restarts;
- preserve session-local zalert behavior if the helper is unavailable.

A daemon is intentionally deferred until there is a concrete need for background behavior beyond the plugin lifecycle.

## Follow-up ideas

### zgrep

- better exact-line positioning
- optional match highlighting
- regex mode

### zmark

- optional persistence
- faster exact restoration for very large offsets

### zdiffpane

- configurable history limit
- richer diff navigation
- optional full-buffer comparison

### zcommandpalette

- discover installed toolbox plugins dynamically
- expose future companion features
