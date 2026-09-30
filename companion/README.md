# Native companions

Most Zellij Toolbox features remain Zellij WASM plugins or native Zellij keybindings. Native companions are used only where a plugin needs narrowly scoped host-side state or cross-session visibility that the WASM sandbox cannot provide cleanly.

Current companions:

- `zellij-toolbox-alert` — maintains cross-session zalert watch state and provides CLI list/jump support.
- `zellij-toolbox-zmark` — stores durable zmark bookmark metadata under the user's state directory so marks can survive zmark plugin unload/reload without granting the WASM plugin full hard-drive access.

Neither companion is a daemon. They are short-lived commands invoked by toolbox plugins or explicitly from the shell.
