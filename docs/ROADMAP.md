# Roadmap

## Ready for qualification

- **zcopyall** — copy all retained scrollback
- **zback** — native FocusLastPane binding
- **zcopycmd** — native CopyLastCommandOutput binding
- **zpaneinfo** — focused pane diagnostics
- **zgrep** — session-wide retained-scrollback search
- **zmark** — session-local scrollback bookmarks
- **zdiffpane** — interactive two-pane output diff
- **zbroadcast** — confirmed command broadcast to selected panes
- **zalert** — notify on the next watched foreground-command change
- **zcommandpalette** — common launcher

## Immediate next step

Qualify the whole batch on Midas and Orion:

- compile every plugin against Zellij 0.45.1;
- test permission prompts and keybindings;
- fix API or UI issues found during use;
- commit Cargo.lock;
- switch qualification to --locked;
- settle default keybindings.

## Follow-up ideas

### zgrep

- better exact-line positioning
- optional match highlighting
- tab/pane filters
- case and regex modes

### zmark

- named marks
- optional persistence
- faster exact restoration for very large offsets

### zdiffpane

- configurable history limit
- richer diff navigation
- optional full-buffer comparison

### zalert companion

An optional native companion can provide one view of watches across all local sessions and direct attach/switch behavior back to the originating session and pane.

### zcommandpalette

- searchable commands
- discover installed toolbox plugins
- show configured shortcuts
- expose future companion features
