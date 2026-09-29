# Roadmap

This is a direction, not a promise that every idea will be implemented exactly as first described.

## Foundation

### zcopyall — working

Copy the complete retained scrollback of the focused pane to the clipboard.

This is the first real toolbox plugin and establishes the WASM build/install path.

### zback — native

Jump to the previously focused pane.

Zellij 0.45 already provides FocusLastPane, so the toolbox only needs to provide a convenient binding.

### zcopycmd — native

Copy the output of the most recent command.

Zellij 0.45 already provides CopyLastCommandOutput, so the toolbox should use the native implementation rather than trying to parse terminal output itself.

## Next utilities

### zgrep

Search retained scrollback across panes and tabs.

Desired flow:

1. open a small search UI;
2. enter text;
3. show matches grouped by pane;
4. choose a result;
5. focus the matching pane and, where practical, highlight or move to the result.

### zmark

Bookmark useful points in terminal history.

Marks should remember enough pane/session context to make returning to the location useful.

### zpaneinfo

Show a compact view of useful current-pane information such as pane identity, tab, command, working directory, and session context.

This is a useful early project because the same metadata will be needed by several later tools.

### zdiffpane

Choose pane A and pane B, then compare their retained output.

Useful for before/after commands, tests, builds, remote machines, and configuration changes.

### zbroadcast

Choose a deliberate set of panes and broadcast input only to those panes.

Safety matters here. Broadcast mode should be visually obvious and easy to leave.

### zalert

Mark work to watch and notify when something meaningful changes or appears finished.

The long-term goal is cross-session awareness: a job in one Zellij session should be able to alert the user while they are working in another.

A small optional native companion may be used to watch all local sessions.

### zcommandpalette

A single front door to the toolbox.

The palette should discover or invoke installed toolbox functions rather than reimplementing them.

## Development order

A reasonable current order is:

1. zcopyall
2. zback / zcopycmd conventions
3. zpaneinfo
4. zgrep
5. zmark
6. zdiffpane
7. zbroadcast
8. zalert
9. zcommandpalette

The order can change when a real daily-work annoyance makes another tool more valuable first.
