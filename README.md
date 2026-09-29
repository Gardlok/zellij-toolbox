# Zellij Toolbox

A growing set of small Zellij utilities for everyday terminal work.

The goal is simple: make common Zellij jobs faster without duplicating features Zellij already provides.

**Baseline: Zellij 0.45.1 or newer.**

## What is here

| Tool | Status | What it does |
| --- | --- | --- |
| **zcopyall** | Working plugin | Copy all retained scrollback from the focused pane |
| **zback** | Native binding | Jump back to the previously focused pane |
| **zcopycmd** | Native binding | Copy the output of the last shell command |
| **zgrep** | Planned | Search retained scrollback across panes |
| **zmark** | Planned | Bookmark useful places in terminal history |
| **zalert** | Planned | Watch work and notify when it finishes or changes |
| **zdiffpane** | Planned | Compare the retained output of two panes |
| **zbroadcast** | Planned | Send input to a chosen set of panes |
| **zpaneinfo** | Planned | Show useful information about the current pane |
| **zcommandpalette** | Planned | One launcher for the whole toolbox |

`zback` and `zcopycmd` use native Zellij 0.45 actions. They do not need their own plugins.

## Install

You need:

- Zellij 0.45.1 or newer
- Rust 1.95 or newer
- Cargo and rustup

Clone the repo and run the installer:

```bash
git clone https://github.com/Gardlok/zellij-toolbox.git
cd zellij-toolbox
./install.sh
```

The installer checks your versions, builds `zcopyall`, installs the WASM plugin, and prints the exact Zellij keybindings for your machine.

It does **not** rewrite your existing `config.kdl`.

## First run

After installation, start `zcopyall` once from inside Zellij using the command printed by the installer.

Zellij will ask for:

- `ReadApplicationState`
- `ReadPaneContents`
- `WriteToClipboard`

Grant those permissions.

Then merge the printed bindings into your existing `keybinds` block.

The default toolbox keys are:

```text
Alt+A   copy all retained pane scrollback
Alt+B   jump back to the previous pane
Alt+C   copy the last command output
```

If your config uses `keybinds clear-defaults=true`, add the toolbox `shared_except "locked"` section inside that existing block.

## zcopyall

`zcopyall` copies everything Zellij still retains for the focused pane, including text above the visible screen.

It cannot recover text that has already fallen out of Zellij's scrollback buffer.

## zcopycmd

Zellij 0.45 understands shell command boundaries through shell integration and provides `CopyLastCommandOutput` directly.

`zcopycmd` is therefore a toolbox keybinding, not another plugin.

If your shell does not provide the prompt markers Zellij needs, command-aware copying may not work as expected.

## zback

Zellij 0.45 provides `FocusLastPane` directly.

The toolbox simply gives it a memorable `Alt+B` binding.

## Development

Run the local qualification script:

```bash
./qualify.sh
```

There is intentionally no GitHub Actions workflow yet. The project starts with simple local qualification and can add CI later if it becomes useful.

See [docs/ROADMAP.md](docs/ROADMAP.md) for what comes next.

## License

MIT. See [LICENSE](LICENSE).
