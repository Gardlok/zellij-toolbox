use std::collections::BTreeMap;
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    lines: Vec<String>,
}

impl State {
    fn open(&mut self) {
        self.pending_open = false;
        self.lines.clear();

        match get_focused_pane_info() {
            Ok((tab_index, pane_id)) => {
                self.lines.push(format!("Pane: {}", pane_id));
                self.lines.push(format!("Tab position: {}", tab_index + 1));

                if let Some(info) = get_pane_info(pane_id) {
                    self.lines.push(format!("Title: {}", info.title));
                    self.lines.push(format!(
                        "Type: {}",
                        if info.is_plugin { "plugin" } else { "terminal" }
                    ));
                    self.lines.push(format!(
                        "Layout: {}{}",
                        if info.is_floating {
                            "floating"
                        } else {
                            "tiled"
                        },
                        if info.is_fullscreen {
                            ", fullscreen"
                        } else {
                            ""
                        }
                    ));
                    self.lines.push(format!(
                        "Size: {}x{} content",
                        info.pane_content_columns, info.pane_content_rows
                    ));
                }

                if matches!(pane_id, PaneId::Terminal(_)) {
                    if let Ok(cwd) = get_pane_cwd(pane_id) {
                        self.lines.push(format!("CWD: {}", cwd.display()));
                    }
                    if let Ok(command) = get_pane_running_command(pane_id) {
                        if !command.is_empty() {
                            self.lines.push(format!("Command: {}", command.join(" ")));
                        }
                    }
                    if let Ok(pid) = get_pane_pid(pane_id) {
                        self.lines.push(format!("PID: {}", pid));
                    }
                }
            }
            Err(error) => self
                .lines
                .push(format!("Could not read focused pane: {}", error)),
        }

        self.visible = true;
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[EventType::Key, EventType::PermissionRequestResult]);
        request_permission(&[PermissionType::ReadApplicationState]);
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
            Event::Key(key) if self.visible => {
                if key.has_no_modifiers()
                    && matches!(key.bare_key, BareKey::Esc | BareKey::Char('q'))
                {
                    self.visible = false;
                    hide_self();
                }
                true
            }
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
        println!("zpaneinfo");
        println!();
        for line in &self.lines {
            println!("{}", line);
        }
        println!();
        println!("Esc/q: close");
    }
}
