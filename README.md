# Zellij Toolbox

Small Zellij utilities for everyday terminal work.

**Zellij baseline: 0.45.1 or newer.**

## Included

| Tool | What it does |
| --- | --- |
| **zcopyall** | Copy all retained scrollback from the focused pane |
| **zback** | Jump back to the previously focused pane |
| **zcopycmd** | Copy the output of the last shell command |
| **zpaneinfo** | Show useful information about the focused pane |
| **zgrep** | Search retained scrollback across panes in the current session |
| **zmark** | Bookmark and return to places in pane scrollback |
| **zdiffpane** | Compare the retained output of two panes |
| **zbroadcast** | Safely send one command to selected panes |
| **zalert** | Notify when a watched pane's foreground command changes |
| **zalert companion** | List watches across local sessions and jump back to the originating pane |
| **zcommandpalette** | Open the toolbox from one menu |

`zback` and `zcopycmd` use native Zellij 0.45 actions. The rest are small WASM plugins.

`zcopycmd` requires OSC 133 shell integration. Zellij 0.45 understands the protocol, but Bash and Zsh do not emit it by default; Fish does.

## Requirements

To **run** the installed toolbox:

- Zellij 0.45.1 or newer

To **build/install from this source checkout**:

- Rust 1.95 or newer
- Cargo and rustup

Rust is a build-time requirement, not a toolbox runtime requirement. Future releases can provide prebuilt WASM files so Rust is not needed for installation.

`zalert` uses `notify-send` for desktop notifications when it is available. Source installs also build a small native companion at `~/.local/bin/zellij-toolbox-alert`; the WASM alert plugin still works locally if that helper is unavailable.

## Install

```bash
git clone https://github.com/Gardlok/zellij-toolbox.git
cd zellij-toolbox
./install.sh
```

The installer checks versions, builds all plugins, installs them under `~/.config/zellij/plugins/zellij-toolbox/`, and prints one keybinding block to merge into your Zellij config.

It does **not** edit `config.kdl` automatically.

The first time a plugin runs, Zellij may ask for the permissions that plugin needs.

### Bash shell integration for Alt+C

`zcopycmd` uses Zellij's native `CopyLastCommandOutput` action. Bash needs OSC 133 prompt markers so Zellij can distinguish prompts, commands, and command output.

The following integration was qualified on Midas with Zellij 0.45.1:

```bash
# Zellij OSC 133 shell integration
if [[ -n "${ZELLIJ:-}" ]]; then
    __zellij_osc133_prompt() {
        local status=$?
        printf '\e]133;D;%d\a\e]133;A\a' "$status"
    }

    PROMPT_COMMAND="__zellij_osc133_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    PS0="$(printf '\033]133;C\a')"
fi
```

Add it to `~/.bashrc`, then start a fresh Bash shell or run `source ~/.bashrc`. The installer intentionally does not modify shell startup files.

## Default keys

```text
Alt+A          zcopyall
Alt+B          zback
Alt+C          zcopycmd
Alt+D          zdiffpane
Alt+G          zgrep
Alt+M          add zmark
Alt+Shift+M    list zmarks
Alt+V          zpaneinfo
Alt+W          toggle zalert
Alt+Shift+W    list zalerts
Alt+Shift+B    zbroadcast
Alt+;          zcommandpalette
```

These are suggested defaults. Change them to fit your setup.

## Notes

### zgrep

zgrep searches retained scrollback across selectable terminal panes in the current Zellij session. It owns the query, case mode, scope, result ordering, selected exact occurrence, buffered results, centering target, and dialog state.

The dialog has two explicit modes:

- **Input** — edit the query and press Enter to search. Tab returns to the buffered list only when the current query still matches the query that produced that list.
- **List** — Up/Down or `j`/`k` and `n`/`p` move through buffered results. Enter jumps to the selected exact result. Tab, `/`, or `e` returns to Input. `c` toggles case sensitivity and refreshes, `s` cycles scope and refreshes, and `r` clears the search and restores case-insensitive/all-panes defaults.

Search state survives hide/reopen while the plugin instance lives. After a jump, `Alt+G` resumes the same buffered list and selected row.

Selecting a result re-resolves that exact line against current retained scrollback, focuses the target pane, moves from the top with coarse page steps, re-measures the real viewport offset, and corrects one line at a time until the selected row is near the vertical center. Scroll mutations are serialized through Zellij `ActionComplete` events.

For visual highlighting, zgrep centers the exact selected result first, clears native search state, installs the query, applies the requested case mode after query installation, then activates one visible native search result so Zellij paints the search highlight. zgrep immediately re-measures and re-centers its own exact selected destination afterward, so native search is used as a renderer rather than as final navigation authority.

Plain `n` and `p` in Scroll mode are installed session-locally by zgrep after permission is granted. zgrep uses Zellij's runtime `reconfigure(..., false)` path with `MessagePluginId`, so the bindings target the already-running zgrep instance directly and do not modify `config.kdl`. Those bindings cycle buffered matches in the currently focused pane and rerun the exact centering engine.

While the dialog is open, `Ctrl+F` toggles the same plugin pane between floating and docked. The transition is serialized: toggle the focused plugin pane, wait for completion, refocus zgrep, then resize floating mode to a centered 80% × 80% window. Reopening zgrep restores Zellij Normal input mode so terminal history/search modes do not leak into plugin input.

The current refactor remains dependency-free. Its Input/List state model was informed by community picker plugins such as `codingfragments/zellij-zextract`, while zgrep keeps its own literal cross-pane search and exact-jump engine. A future UI-only milestone can evaluate `ratatui` and fuzzy filtering with a separately generated and qualified lockfile.

### zmark

zmark stores bookmark metadata durably through the installed `zellij-toolbox-zmark` helper, so named marks can survive zmark plugin unload/reload without granting the WASM plugin full hard-drive access. No daemon is started.

Marks remain scoped to the Zellij session name. A mark stores the pane, scroll position, and an anchor line. Live marks retain the existing offset fallback, while restored marks must re-resolve their saved anchor before zmark will jump; if the pane is gone or the anchor cannot be found, zmark reports the mark as stale instead of jumping blindly.

The helper stores state under `$XDG_STATE_HOME/zellij-toolbox` when `XDG_STATE_HOME` is set, otherwise under `~/.local/state/zellij-toolbox`. You can inspect the stored marks with:

```bash
~/.local/bin/zellij-toolbox-zmark list
```

### zdiffpane

Select two panes. The first version compares up to the most recent 300 retained lines from each pane and lets you copy the diff. Press `f` inside the plugin to toggle no-UI fullscreen.

### zbroadcast

Select target panes, type one command, review it, then press Enter again to send. The confirmation step is intentional.

### zalert

Use Alt+W while a long-running command is active. zalert watches for that pane's foreground command to change and then calls `notify-send`.

A watch is intentionally one-shot: after it fires, it is removed. This avoids repeated notifications as the pane returns to a shell and starts later commands. Very short commands can finish before there is time to arm them.

Desktop notifications can appear while you are working in another Zellij session.

The native companion keeps a shared view of active watches across local sessions:

```bash
~/.local/bin/zellij-toolbox-alert list
```

Each row has a numeric index. From inside any active Zellij client, jump directly to the originating session and pane with:

```bash
~/.local/bin/zellij-toolbox-alert jump 1
```

The same cross-session view is available inside Zellij. Open the session-local alert list with Alt+Shift+W and press `g`, or open the command palette with Alt+; and search for `zalert:global`. In the global view, use Up/Down to select, Enter to jump, `r` to refresh, and `l` to return to the current-session list.

The global UI uses Zellij's native session switching API; the companion is only used to read shared watch state. A first use after upgrading may request the additional Zellij permission needed to change application state.

The helper prunes watches whose Zellij sessions are no longer running. Companion state is additive: if the helper is missing or fails, session-local zalert behavior continues to work.

### zcommandpalette

The palette is the toolbox home screen. The suggested key is `Alt+;` because desktop environments commonly reserve `Alt+Space`.

- Type to search tool names, shortcuts, descriptions, categories, and plugin-local controls.
- Up/Down selects a result and Enter runs it.
- The selected row shows contextual details and the tool's important internal controls.
- Press `?` for a compact everyday cheatsheet of toolbox shortcuts; Up/Down or PgUp/PgDn scrolls it when needed.
- Number keys `1`–`0` retain the existing quick-run mappings when the search box is empty.
- `zback` is shown for discoverability but remains shortcut-only on `Alt+B`, because opening the palette itself changes focus history.

The goal is that `Alt+;` is the only toolbox shortcut you need to remember.

## Development

```bash
./qualify.sh
```

This formats, checks, builds, and verifies every current WASM artifact locally using the committed `Cargo.lock`.

See [docs/ROADMAP.md](docs/ROADMAP.md) for planned follow-up work.

## License

MIT. See [LICENSE](LICENSE).
