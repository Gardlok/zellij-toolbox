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

## Qualified cross-session zalert companion

Qualified and runtime-verified on Midas. The first native companion milestone keeps zalert detection in the WASM plugin while adding a dependency-free host helper:

- publish active watch state across local Zellij sessions;
- list all active watches from one CLI;
- jump from the current Zellij client to the originating session and pane;
- prune dead-session state and reject stale asynchronous watch mutations;
- preserve session-local zalert behavior if the helper is unavailable.

A daemon is intentionally deferred until there is a concrete need for background behavior beyond the plugin lifecycle.

## Qualified in-Zellij global alert UI

Qualified and runtime-verified on Midas without adding another service:

- query shared cross-session watch state from the zalert WASM plugin through `RunCommandResult`;
- expose a global-watch view from the local zalert list;
- expose the global view through the searchable command palette;
- refresh the global list on demand;
- jump with Zellij's native session/pane switching API rather than invoking the helper for navigation;
- ignore stale asynchronous refresh responses and stale session generations.

## Current development: zgrep precision and serialized positioning

Improve zgrep navigation without changing its cross-pane discovery model or dependency set:

- re-resolve a selected result against current scrollback before jumping;
- focus the target pane and serialize scroll mutations through Zellij `ActionComplete` events;
- start from the top, use page scrolling only as a coarse accelerator, then re-read the actual viewport offset;
- correct any remaining offset one line at a time until the selected row is near the vertical center of the pane;
- switch the client into Zellij Scroll mode after the jump so scrollback state is explicit without launching an external scrollback editor;
- clear prior native search state, apply zgrep's case setting, and install the query only after centering so Zellij's built-in search renderer supplies the highlight without choosing the destination;
- preserve zgrep's existing case and scope controls.

Earlier runtime candidates were rejected before merge: one assumed fire-and-forget scroll commands had completed, another let native search choose navigation and therefore failed exact-result selection, and the plugin-owned regex highlight path did not render reliably in Scroll mode.

## Follow-up ideas

### zgrep

- regex search mode

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
