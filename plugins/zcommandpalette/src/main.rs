use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_tile::prelude::actions::Action;

register_plugin!(State);

const ENTRIES: &[(&str, &str)] = &[
    ("zcopyall", "Copy all retained scrollback"),
    ("zcopycmd", "Copy the last command output"),
    ("zpaneinfo", "Show focused pane information"),
    ("zgrep", "Search scrollback across panes"),
    ("zmark:add", "Bookmark the current scrollback position"),
    ("zmark:list", "Show session bookmarks"),
    ("zdiffpane", "Compare two pane histories"),
    ("zbroadcast", "Send one command to selected panes"),
    ("zalert:watch", "Toggle alert on the focused pane"),
    ("zalert:list", "Show active alerts"),
];

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    selected: usize,
    origin_pane: Option<u32>,
    plugin_dir: String,
}

impl State {
    fn refresh_plugin_dir(&mut self) {
        let env = get_session_environment_variables();
        let config_dir = env
            .get("ZELLIJ_CONFIG_DIR")
            .cloned()
            .or_else(|| env.get("HOME").map(|home| format!("{}/.config/zellij", home)))
            .unwrap_or_else(|| ".config/zellij".to_owned());

        self.plugin_dir = format!("{}/plugins/zellij-toolbox", config_dir);
    }

    fn open(&mut self) {
        self.pending_open = false;
        self.selected = 0;
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

    fn activate(&mut self) {
        self.visible = false;
        hide_self();
        self.restore_origin();

        match self.selected {
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

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return true;
        }

        match key.bare_key {
            BareKey::Esc | BareKey::Char('q') => {
                self.visible = false;
                hide_self();
                self.restore_origin();
            }
            BareKey::Up | BareKey::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            BareKey::Down | BareKey::Char('j') => {
                self.selected = (self.selected + 1).min(ENTRIES.len().saturating_sub(1));
            }
            BareKey::Enter => self.activate(),
            BareKey::Char(c) if c.is_ascii_digit() => {
                let index = if c == '0' {
                    9
                } else {
                    c.to_digit(10).unwrap_or(1) as usize - 1
                };
                if index < ENTRIES.len() {
                    self.selected = index;
                    self.activate();
                }
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

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!("Zellij Toolbox");
        println!();

        for (index, (name, description)) in ENTRIES.iter().enumerate() {
            let marker = if index == self.selected { ">" } else { " " };
            let key = if index == 9 {
                "0".to_owned()
            } else {
                (index + 1).to_string()
            };
            println!("{} {}  {:<13} {}", marker, key, name, description);
        }

        println!();
        println!("Up/Down: select   Enter: run   1-0: quick run   Esc: close");
    }
}
