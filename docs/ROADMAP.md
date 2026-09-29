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

### zalert companion

An optional native companion can provide one view of watches across all local sessions and direct attach/switch behavior back to the originating session and pane.

### zcommandpalette

- discover installed toolbox plugins dynamically
- expose future companion features
