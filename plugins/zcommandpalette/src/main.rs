use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

register_plugin!(State);

struct Entry {
    name: &'static str,
    description: &'static str,
    shortcut: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        name: "zcopyall",
        description: "Copy all retained scrollback",
        shortcut: "Alt+A",
    },
    Entry {
        name: "zcopycmd",
        description: "Copy the last command output",
        shortcut: "Alt+C",
    },
    Entry {
        name: "zpaneinfo",
        description: "Show focused pane information",
        shortcut: "Alt+V",
    },
    Entry {
        name: "zgrep",
        description: "Search scrollback across panes",
        shortcut: "Alt+G",
    },
    Entry {
        name: "zmark:add",
        description: "Bookmark the current scrollback position",
        shortcut: "Alt+M",
    },
    Entry {
        name: "zmark:list",
        description: "Show session bookmarks",
        shortcut: "Alt+Shift+M",
    },
    Entry {
        name: "zdiffpane",
        description: "Compare two pane histories",
        shortcut: "Alt+D",
    },
    Entry {
        name: "zbroadcast",
        description: "Send one command to selected panes",
        shortcut: "Alt+Shift+B",
    },
    Entry {
        name: "zalert:watch",
        description: "Arm/watch the focused pane",
        shortcut: "Alt+W",
    },
    Entry {
        name: "zalert:list",
        description: "Show active alerts",
        shortcut: "Alt+Shift+W",
    },
];

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    selected: usize,
    origin_pane: Option<u32>,
    plugin_dir: String,
    query: String,
    filtered: Vec<usize>,
}

impl State {
    fn refresh_plugin_dir(&mut self) {
        let env = get_session_environment_variables();
        let config_dir = env
            .get("ZELLIJ_CONFIG_DIR")
            .cloned()
            .or_else(|| {
                env.get("HOME")
                    .map(|home| format!("{}/.config/zellij", home))
            })
            .unwrap_or_else(|| ".config/zellij".to_owned());

        self.plugin_dir = format!("{}/plugins/zellij-toolbox", config_dir);
    }

    fn refresh_filter(&mut self) {
        let needle = self.query.trim().to_lowercase();
        self.filtered = ENTRIES
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if needle.is_empty()
                    || entry.name.to_lowercase().contains(&needle)
                    || entry.description.to_lowercase().contains(&needle)
                    || entry.shortcut.to_lowercase().contains(&needle)
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn open(&mut self) {
        self.pending_open = false;
        self.selected = 0;
        self.query.clear();
        self.refresh_filter();
        self.origin_pane = match get_focused_pane_info() {
            Ok((_tab, PaneId::Terminal(pane_id))) => Some(pane_id),
            _ => None,
        };
        self.refresh_plugin_dir();
        self.visible = true;
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }

    fn plugin_url(&self, plugin: &str) -> String {
        format!("file:{}/{}.wasm", self.plugin_dir, plugin)
    }

    fn restore_origin(&self) {
        if let Some(pane_id) = self.origin_pane {
            focus_terminal_pane(pane_id, false, false);
        }
    }

    fn send_plugin_message(&self, plugin: &str, message: &str) {
        pipe_message_to_plugin(
            MessageToPlugin::new(message).with_plugin_url(self.plugin_url(plugin)),
        );
    }

    fn activate_entry(&mut self, entry_index: usize) {
        self.visible = false;
        hide_self();
        self.restore_origin();

        match entry_index {
            0 => self.send_plugin_message("zcopyall", "copy_all"),
            1 => run_action(Action::CopyLastCommandOutput, BTreeMap::new()),
            2 => self.send_plugin_message("zpaneinfo", "open"),
            3 => self.send_plugin_message("zgrep", "open"),
            4 => self.send_plugin_message("zmark", "mark"),
            5 => self.send_plugin_message("zmark", "open"),
            6 => self.send_plugin_message("zdiffpane", "open"),
            7 => self.send_plugin_message("zbroadcast", "open"),
            8 => self.send_plugin_message("zalert", "watch"),
            9 => self.send_plugin_message("zalert", "open"),
            _ => {}
        }
    }

    fn activate_selected(&mut self) {
        let Some(&entry_index) = self.filtered.get(self.selected) else {
            return;
        };
        self.activate_entry(entry_index);
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return true;
        }

        match key.bare_key {
            BareKey::Esc => {
                self.visible = false;
                hide_self();
                self.restore_origin();
            }
            BareKey::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            BareKey::Down => {
                if !self.filtered.is_empty() {
                    self.selected = (self.selected + 1).min(self.filtered.len() - 1);
                }
            }
            BareKey::Enter => self.activate_selected(),
            BareKey::Backspace => {
                self.query.pop();
                self.refresh_filter();
            }
            BareKey::Char(c) if c.is_ascii_digit() && self.query.is_empty() => {
                let index = if c == '0' {
                    9
                } else {
                    c.to_digit(10).unwrap_or(1) as usize - 1
                };
                if index < ENTRIES.len() {
                    self.activate_entry(index);
                }
            }
            BareKey::Char(c) => {
                self.query.push(c);
                self.selected = 0;
                self.refresh_filter();
            }
            _ => {}
        }

        true
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[EventType::Key, EventType::PermissionRequestResult]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::MessageAndLaunchOtherPlugins,
            PermissionType::RunActionsAsUser,
            PermissionType::ReadSessionEnvironmentVariables,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;
                if self.pending_open {
                    self.open();
                    return true;
                }
                false
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_open = false;
                false
            }
            Event::Key(key) if self.visible => self.handle_key(key),
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "open" {
            self.request_open();
            return true;
        }
        false
    }

    fn render(&mut self, rows: usize, _cols: usize) {
        println!("Zellij Toolbox");
        println!("Search: {}", self.query);
        println!();

        if self.filtered.is_empty() {
            println!("No matching toolbox commands.");
        } else {
            let available = rows.saturating_sub(6).max(1);
            let start = self.selected.saturating_sub(available.saturating_sub(1));
            let end = (start + available).min(self.filtered.len());

            for (visible_index, entry_index) in self.filtered[start..end].iter().enumerate() {
                let absolute_index = start + visible_index;
                let entry = &ENTRIES[*entry_index];
                let marker = if absolute_index == self.selected {
                    ">"
                } else {
                    " "
                };
                let key = if *entry_index == 9 {
                    "0".to_owned()
                } else {
                    (*entry_index + 1).to_string()
                };
                println!(
                    "{} {}  {:<13} {:<11} {}",
                    marker, key, entry.name, entry.shortcut, entry.description
                );
            }
        }

        println!();
        println!("Type: filter   Up/Down: select   Enter: run   1-0: quick run   Esc: close");
    }
}
