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

## Current development: zgrep-owned destination with native highlight rendering

Refactor zgrep around a strict ownership boundary while preserving literal cross-pane search:

- zgrep owns query, case sensitivity, scope, result ordering, exact selected occurrence, buffered results, centering, and dialog placement;
- explicit Input/List modes keep query editing separate from result navigation;
- buffered results survive hide/reopen and remain valid only while the current query still matches the query that produced them;
- exact jumps are re-resolved against current scrollback and serialized through `ActionComplete`;
- selected rows are placed near the vertical center using coarse page movement plus measured one-line correction;
- native Zellij search is used only to render visible highlights after the first exact positioning pass;
- after native search installs its case option and query, zgrep repeats the exact positioning pass so any native-search viewport movement is overwritten;
- Scroll-mode `n/p` navigation is installed at runtime with direct plugin-id keybind pipes while a valid result buffer exists;
- original Scroll-mode `n/p` bindings are snapshotted from `InitialKeybinds` and restored when the search is edited/reset;
- `Ctrl+F` float/dock transitions are serialized, followed by explicit zgrep refocus and floating resize;
- opening zgrep restores Normal input mode so terminal Scroll/Search modes do not leak into dialog input.

Superseded runtime candidates relied on native search as final destination authority, plugin regex highlighting in retained Scroll-mode history, URL-addressed manual Scroll-mode message bindings, or global next/previous bindings. Those approaches were rejected because they could move to case-equivalent/duplicate matches, failed to render visibly, depended on manual config, or conflicted with established controls.

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
