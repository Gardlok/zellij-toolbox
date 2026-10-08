use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteView {
    Palette,
    Help,
}

impl Default for PaletteView {
    fn default() -> Self {
        Self::Palette
    }
}

#[derive(Clone, Copy)]
enum EntryAction {
    CopyAll,
    CopyLastCommandOutput,
    PaneInfo,
    Zgrep,
    ZmarkAdd,
    ZmarkList,
    Zdiffpane,
    Zbroadcast,
    ZalertWatch,
    ZalertList,
    ZalertGlobal,
    ShortcutOnly,
}

struct Entry {
    category: &'static str,
    name: &'static str,
    description: &'static str,
    shortcut: &'static str,
    details: &'static str,
    controls: &'static str,
    quick: Option<char>,
    action: EntryAction,
}

const ENTRIES: &[Entry] = &[
    Entry {
        category: "Navigation",
        name: "zback",
        description: "Jump to the previously focused pane",
        shortcut: "Alt+B",
        details: "Use the direct shortcut: opening the palette changes focus history.",
        controls: "Alt+B from a terminal pane.",
        quick: None,
        action: EntryAction::ShortcutOnly,
    },
    Entry {
        category: "Search",
        name: "zgrep",
        description: "Search retained scrollback across panes",
        shortcut: "Alt+G",
        details: "Literal cross-pane search with exact jumps, case/scope controls, and native highlights.",
        controls: "Enter search/jump; c case; s scope; r reset; Ctrl+F float/dock; Scroll n/p next/previous.",
        quick: Some('4'),
        action: EntryAction::Zgrep,
    },
    Entry {
        category: "Bookmarks",
        name: "zmark:add",
        description: "Bookmark the current scrollback position",
        shortcut: "Alt+M",
        details: "Save a durable pane/scrollback anchor for the current Zellij session.",
        controls: "Alt+M saves immediately.",
        quick: Some('5'),
        action: EntryAction::ZmarkAdd,
    },
    Entry {
        category: "Bookmarks",
        name: "zmark:list",
        description: "Browse durable scrollback bookmarks",
        shortcut: "Alt+Shift+M",
        details: "Open saved marks, jump back to them, rename them, or delete stale marks.",
        controls: "Up/Down select; Enter jump; n rename; d delete; Esc close.",
        quick: Some('6'),
        action: EntryAction::ZmarkList,
    },
    Entry {
        category: "Copy",
        name: "zcopyall",
        description: "Copy all retained scrollback",
        shortcut: "Alt+A",
        details: "Copies the entire retained scrollback buffer from the focused terminal pane.",
        controls: "No dialog; result goes to the configured clipboard.",
        quick: Some('1'),
        action: EntryAction::CopyAll,
    },
    Entry {
        category: "Copy",
        name: "zcopycmd",
        description: "Copy the last command output",
        shortcut: "Alt+C",
        details: "Uses Zellij's native last-command-output action; Bash requires OSC 133 integration.",
        controls: "No dialog; run from the terminal pane whose last output you want.",
        quick: Some('2'),
        action: EntryAction::CopyLastCommandOutput,
    },
    Entry {
        category: "Pane tools",
        name: "zpaneinfo",
        description: "Show focused pane information",
        shortcut: "Alt+V",
        details: "Inspect identifiers and useful state for the currently focused pane.",
        controls: "Esc or q closes.",
        quick: Some('3'),
        action: EntryAction::PaneInfo,
    },
    Entry {
        category: "Pane tools",
        name: "zdiffpane",
        description: "Compare two pane histories",
        shortcut: "Alt+D",
        details: "Select two terminal panes and compare their retained output.",
        controls: "Up/Down select; Enter choose; f fullscreen; diff view: c copy, r reset, Esc/q close.",
        quick: Some('7'),
        action: EntryAction::Zdiffpane,
    },
    Entry {
        category: "Alerts",
        name: "zalert:watch",
        description: "Arm a one-shot alert for the focused pane",
        shortcut: "Alt+W",
        details: "Notify when the currently watched foreground command finishes or changes.",
        controls: "Alt+W arms/toggles the focused pane watch.",
        quick: Some('9'),
        action: EntryAction::ZalertWatch,
    },
    Entry {
        category: "Alerts",
        name: "zalert:list",
        description: "Show active alerts in this session",
        shortcut: "Alt+Shift+W",
        details: "Inspect local watches and open the cross-session alert view.",
        controls: "g opens global alerts; Esc/q closes.",
        quick: Some('0'),
        action: EntryAction::ZalertList,
    },
    Entry {
        category: "Alerts",
        name: "zalert:global",
        description: "Show alerts across local Zellij sessions",
        shortcut: "Alt+Shift+W → g",
        details: "Read shared alert state and jump to the originating session/pane.",
        controls: "Up/Down select; Enter jump; r refresh; l local; Esc/q close.",
        quick: None,
        action: EntryAction::ZalertGlobal,
    },
    Entry {
        category: "Multi-pane",
        name: "zbroadcast",
        description: "Send one reviewed command to selected panes",
        shortcut: "Alt+Shift+B",
        details: "Choose target panes, compose one command, review it, then explicitly send.",
        controls: "Up/Down move; Space toggle; Enter compose/review/SEND; Esc goes back.",
        quick: Some('8'),
        action: EntryAction::Zbroadcast,
    },
];

const HELP_CATEGORIES: &[&str] = &[
    "Navigation",
    "Search",
    "Bookmarks",
    "Copy",
    "Pane tools",
    "Alerts",
    "Multi-pane",
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
    view: PaletteView,
    status: String,
    help_offset: usize,
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
                let matches = needle.is_empty()
                    || entry.name.to_lowercase().contains(&needle)
                    || entry.description.to_lowercase().contains(&needle)
                    || entry.shortcut.to_lowercase().contains(&needle)
                    || entry.category.to_lowercase().contains(&needle)
                    || entry.details.to_lowercase().contains(&needle)
                    || entry.controls.to_lowercase().contains(&needle);
                matches.then_some(index)
            })
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn open(&mut self) {
        self.pending_open = false;
        self.selected = 0;
        self.query.clear();
        self.view = PaletteView::Palette;
        self.status.clear();
        self.help_offset = 0;
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

    fn close(&mut self) {
        self.visible = false;
        hide_self();
        self.restore_origin();
    }

    fn send_plugin_message(&self, plugin: &str, message: &str) {
        pipe_message_to_plugin(
            MessageToPlugin::new(message).with_plugin_url(self.plugin_url(plugin)),
        );
    }

    fn activate_entry(&mut self, entry_index: usize) {
        let Some(entry) = ENTRIES.get(entry_index) else {
            return;
        };

        if matches!(entry.action, EntryAction::ShortcutOnly) {
            self.status = entry.details.to_owned();
            return;
        }

        let action = entry.action;
        self.visible = false;
        hide_self();
        self.restore_origin();

        match action {
            EntryAction::CopyAll => self.send_plugin_message("zcopyall", "copy_all"),
            EntryAction::CopyLastCommandOutput => {
                run_action(Action::CopyLastCommandOutput, BTreeMap::new())
            }
            EntryAction::PaneInfo => self.send_plugin_message("zpaneinfo", "open"),
            EntryAction::Zgrep => self.send_plugin_message("zgrep", "open"),
            EntryAction::ZmarkAdd => self.send_plugin_message("zmark", "mark"),
            EntryAction::ZmarkList => self.send_plugin_message("zmark", "open"),
            EntryAction::Zdiffpane => self.send_plugin_message("zdiffpane", "open"),
            EntryAction::Zbroadcast => self.send_plugin_message("zbroadcast", "open"),
            EntryAction::ZalertWatch => self.send_plugin_message("zalert", "watch"),
            EntryAction::ZalertList => self.send_plugin_message("zalert", "open"),
            EntryAction::ZalertGlobal => self.send_plugin_message("zalert", "global"),
            EntryAction::ShortcutOnly => {}
        }
    }

    fn activate_selected(&mut self) {
        let Some(&entry_index) = self.filtered.get(self.selected) else {
            return;
        };
        self.activate_entry(entry_index);
    }

    fn activate_quick(&mut self, key: char) {
        if let Some((index, _entry)) = ENTRIES
            .iter()
            .enumerate()
            .find(|(_index, entry)| entry.quick == Some(key))
        {
            self.activate_entry(index);
        }
    }

    fn toggle_help(&mut self) {
        self.view = match self.view {
            PaletteView::Palette => PaletteView::Help,
            PaletteView::Help => PaletteView::Palette,
        };
        self.help_offset = 0;
        self.status.clear();
    }

    fn handle_palette_key(&mut self, key: KeyWithModifier) {
        match key.bare_key {
            BareKey::Esc => self.close(),
            BareKey::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.status.clear();
            }
            BareKey::Down => {
                if !self.filtered.is_empty() {
                    self.selected = (self.selected + 1).min(self.filtered.len() - 1);
                }
                self.status.clear();
            }
            BareKey::Enter => self.activate_selected(),
            BareKey::Backspace => {
                self.query.pop();
                self.selected = 0;
                self.status.clear();
                self.refresh_filter();
            }
            BareKey::Char(c) if c.is_ascii_digit() && self.query.is_empty() => {
                self.activate_quick(c);
            }
            BareKey::Char(c) => {
                self.query.push(c);
                self.selected = 0;
                self.status.clear();
                self.refresh_filter();
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyWithModifier) {
        match key.bare_key {
            BareKey::Esc => self.close(),
            BareKey::Char('h') => self.toggle_help(),
            BareKey::Up => {
                self.help_offset = self.help_offset.saturating_sub(1);
            }
            BareKey::Down => {
                self.help_offset = (self.help_offset + 1).min(ENTRIES.len());
            }
            BareKey::PageUp => {
                self.help_offset = self.help_offset.saturating_sub(5);
            }
            BareKey::PageDown => {
                self.help_offset = (self.help_offset + 5).min(ENTRIES.len());
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if key.bare_key == BareKey::Char('?') {
            self.toggle_help();
            return true;
        }

        if !key.has_no_modifiers() {
            return true;
        }

        match self.view {
            PaletteView::Palette => self.handle_palette_key(key),
            PaletteView::Help => self.handle_help_key(key),
        }

        true
    }

    fn clipped(value: &str, width: usize) -> String {
        value.chars().take(width).collect()
    }

    fn help_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "{:<16} {:<14} {}",
            "Toolbox", "Alt+;", "Open/search this home screen"
        )];

        for category in HELP_CATEGORIES {
            for entry in ENTRIES.iter().filter(|entry| entry.category == *category) {
                lines.push(format!(
                    "{:<16} {:<14} {} — {}",
                    entry.category, entry.shortcut, entry.name, entry.description
                ));
            }
        }

        lines
    }

    fn render_palette(&self, rows: usize, cols: usize) {
        println!("Zellij Toolbox — Palette");
        println!("Search: {}", self.query);
        println!();

        let available = rows.saturating_sub(9).max(1);
        let start = self.selected.saturating_sub(available.saturating_sub(1));
        let end = (start + available).min(self.filtered.len());

        if self.filtered.is_empty() {
            println!("No matching toolbox commands.");
        } else {
            for (visible_index, entry_index) in self.filtered[start..end].iter().enumerate() {
                let absolute_index = start + visible_index;
                let entry = &ENTRIES[*entry_index];
                let marker = if absolute_index == self.selected {
                    ">"
                } else {
                    " "
                };
                let quick = entry.quick.map(|key| key.to_string()).unwrap_or_else(|| "·".to_owned());
                let prefix = format!(
                    "{} {}  {:<13} {:<13} ",
                    marker, quick, entry.name, entry.shortcut
                );
                let width = cols.saturating_sub(prefix.chars().count());
                println!("{}{}", prefix, Self::clipped(entry.description, width));
            }
        }

        println!();

        if let Some(entry) = self
            .filtered
            .get(self.selected)
            .and_then(|index| ENTRIES.get(*index))
        {
            println!(
                "{}",
                Self::clipped(
                    &format!("{}: {}", entry.category, entry.details),
                    cols
                )
            );
            println!(
                "{}",
                Self::clipped(&format!("Controls: {}", entry.controls), cols)
            );
        } else {
            println!();
            println!();
        }

        if !self.status.is_empty() {
            println!("{}", Self::clipped(&format!("Note: {}", self.status), cols));
        } else {
            println!();
        }

        println!("Type: filter   Up/Down: select   Enter: run   1-0: quick run   ?: cheatsheet   Esc: close");
    }

    fn render_help(&self, rows: usize, cols: usize) {
        println!("Zellij Toolbox — Cheatsheet");
        println!("Everyday shortcuts. Use Up/Down or PgUp/PgDn if the list does not fit.");
        println!();

        let lines = self.help_lines();
        let available = rows.saturating_sub(5).max(1);
        let max_start = lines.len().saturating_sub(available);
        let start = self.help_offset.min(max_start);
        let end = (start + available).min(lines.len());

        for line in &lines[start..end] {
            println!("{}", Self::clipped(line, cols));
        }

        println!();
        println!("h/?: palette   Up/Down/PgUp/PgDn: scroll   Esc: close");
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

    fn render(&mut self, rows: usize, cols: usize) {
        match self.view {
            PaletteView::Palette => self.render_palette(rows, cols),
            PaletteView::Help => self.render_help(rows, cols),
        }
    }
}
