# Native companions

Most Zellij Toolbox features should remain Zellij WASM plugins or native Zellij keybindings.

A native companion belongs here only when a feature genuinely needs machine-wide visibility outside one Zellij session.

The current expected use is zalertd, an optional future helper for zalert so watched jobs can notify and be found across multiple local Zellij sessions.

Nothing in this directory is required for the current toolbox.
