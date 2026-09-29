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

`zalert` uses `notify-send` for desktop notifications when it is available.

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

Type a search term and press Enter. Results come from selectable terminal panes in the current Zellij session. Selecting a result focuses that pane and moves toward the matching line in retained scrollback.

### zmark

Marks are session-local. A mark stores the pane, scroll position, and an anchor line so it can usually return to the same area even after more output appears.

### zdiffpane

Select two panes. The first version compares up to the most recent 300 retained lines from each pane and lets you copy the diff. Press `f` inside the plugin to toggle no-UI fullscreen.

### zbroadcast

Select target panes, type one command, review it, then press Enter again to send. The confirmation step is intentional.

### zalert

Use Alt+W while a long-running command is active. zalert watches for that pane's foreground command to change and then calls `notify-send`.

A watch is intentionally one-shot: after it fires, it is removed. This avoids repeated notifications as the pane returns to a shell and starts later commands. Very short commands can finish before there is time to arm them.

Desktop notifications can appear while you are working in another Zellij session. Automatically jumping back to the originating session is planned for a later native companion.

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
