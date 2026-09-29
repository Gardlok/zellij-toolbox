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

Type a search term and press Enter. Results come from selectable terminal panes in the current Zellij session.

Selecting a result re-resolves the chosen line against the pane's current retained scrollback, then hands navigation to Zellij's native search engine. zgrep first searches for the complete selected line as an anchor so duplicate query matches elsewhere in the pane do not determine the destination. Once Zellij has reached that line, zgrep replaces the anchor with the original query, leaving the native search highlight visible on the selected result.

The handoff is sequenced through Zellij `ActionComplete` events rather than issuing scroll commands and immediately assuming they have taken effect. Case and scope controls remain `c` and `s`/Tab.

Zellij 0.45.1's native search input accepts printable ASCII. zgrep can still discover non-ASCII matches, but exact native jump/highlight is currently limited to printable-ASCII queries and anchor lines.

### zmark

Marks are session-local. A mark stores the pane, scroll position, and an anchor line so it can usually return to the same area even after more output appears.

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

The palette is a common front door for the toolbox. The suggested key is `Alt+;` because desktop environments commonly reserve `Alt+Space`. `zback` stays on Alt+B because opening the palette itself changes focus history.

## Development

```bash
./qualify.sh
```

This formats, checks, builds, and verifies every current WASM artifact locally using the committed `Cargo.lock`.

See [docs/ROADMAP.md](docs/ROADMAP.md) for planned follow-up work.

## License

MIT. See [LICENSE](LICENSE).
