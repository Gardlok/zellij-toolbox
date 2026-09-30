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

## Current development: zgrep modal picker and exact jump ownership

Refactor zgrep around a strict ownership boundary informed by community picker designs such as zextract, while keeping the existing literal cross-pane search model and dependency set:

- zgrep owns the query, case sensitivity, scope, result ordering, selected exact occurrence, buffered results, next/previous selection, centering target, and dialog placement state;
- use explicit **Input** and **List** modes instead of accumulating overlapping one-off key meanings;
- preserve the buffered result list across hide/reopen and return to Input if the query has been edited since that list was produced;
- keep all result navigation inside the dialog, avoiding global Alt+N/Alt+P conflicts with established pane controls;
- re-resolve the selected result against current scrollback before every jump;
- focus the target pane and serialize scroll mutations through Zellij `ActionComplete` events;
- start from the top, use page scrolling only as a coarse accelerator, re-read the actual viewport offset, then correct one line at a time until the selected row is near the vertical center;
- use native search only as a passive highlight renderer after exact positioning; clear native search state first, rebuild the requested case option deterministically, and never let native search select or navigate an occurrence;
- serialize the in-dialog `Ctrl+F` float/dock transition through `ActionComplete`, refocusing zgrep after the layer change and resizing floating mode only after focus is restored;
- normalize the client back to Zellij Normal mode whenever the zgrep dialog opens so terminal history/search modes do not leak into plugin input.

Earlier runtime candidates were rejected before merge because they relied on fire-and-forget scroll sequencing, native-search-owned occurrence navigation, plugin regex highlights that did not render reliably in Scroll mode, or global next/previous bindings that conflicted with the user's established pane controls.

## Follow-up ideas

### zgrep

- evaluate ratatui for a richer picker UI in a separate dependency/lockfile milestone
- optional fuzzy filtering after literal exact-jump behavior is qualified
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
